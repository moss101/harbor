//! Skill eval harness (`harbor.skill_eval/v1`).
//!
//! Three tiers share one case format:
//!
//! 1. **Node/graph tier (CI, no weights):** the graph runs end to end with a
//!    cassette answering every model node, so tools, approvals, budgets,
//!    cancellation and replay are proven deterministically.
//! 2. **Live tier (qualification machine):** the same cases run against a
//!    real provider; results carry the provider/model identity.
//! 3. **Record:** a live run writes the cassette for tier 1.
//!
//! Assertions are typed and machine-checked; prose expectations from
//! `harbor.skill/v1` eval cases are documentation, not tests.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use harbor_agent::{EventLog, EventPayload};
use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_inference::{Cassette, RecordReplayProvider};

use crate::executor::{lease_db_path, Executor, FileStateStore, Host, RunRequest, RunStatus};
use crate::jsonschema;
use crate::pointer;
use crate::skills::SkillManifest;
use crate::tools::{MemoryArtifacts, ToolRegistry};

pub const SCHEMA: &str = "harbor.skill_eval/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Assertion {
    RunState {
        equals: String,
    },
    Outcome {
        equals: String,
    },
    StateEquals {
        pointer: String,
        value: Value,
    },
    StateExists {
        pointer: String,
    },
    StateMissing {
        pointer: String,
    },
    StateNonempty {
        pointer: String,
    },
    StateEmpty {
        pointer: String,
    },
    ArrayLen {
        pointer: String,
        #[serde(default)]
        min: Option<usize>,
        #[serde(default)]
        max: Option<usize>,
    },
    /// Every string selected by `pointer` (wildcards allowed) is at most
    /// `max` characters.
    StringLen {
        pointer: String,
        max: usize,
    },
    Contains {
        pointer: String,
        text: String,
    },
    NotContains {
        pointer: String,
        text: String,
    },
    /// Every non-null string selected by `pointer` occurs verbatim in the
    /// text at `source` — the "no invented owners" check.
    ValuesAppearIn {
        pointer: String,
        source: String,
        #[serde(default = "default_true")]
        allow_null: bool,
    },
    StateMatchesSchema {
        pointer: String,
        schema: Value,
    },
    ApprovalRequested,
    BatchProposed {
        pointer: String,
        #[serde(default = "default_one")]
        min_ops: usize,
    },
    /// Structural: every tool recorded in the run's events is a tool the
    /// graph declares.
    ToolsWithinGraph,
    StepsLe {
        max: u64,
    },
    ToolCallsLe {
        max: u64,
    },
    /// The hash chain replays and its final state matches the report.
    ReplayVerified,
    /// The cassette answered every model call (no misses).
    NoCassetteMisses,
}

fn default_true() -> bool {
    true
}

fn default_one() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactFixture {
    /// Path relative to the eval root (the repository root for built-ins).
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvalCase {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub inputs: Value,
    #[serde(default)]
    pub host_inputs: Value,
    #[serde(default)]
    pub artifacts: std::collections::BTreeMap<String, ArtifactFixture>,
    /// Cassette path relative to the case file's directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cassette: Option<String>,
    /// Tiers the case is meaningful in. A guard case that *requires* a
    /// misbehaving model (an invented figure, a hallucinated fix) is a
    /// contract test of the deterministic node and lists `replay` only;
    /// the default is both tiers.
    #[serde(default = "default_tiers")]
    pub tiers: Vec<String>,
    pub assertions: Vec<Assertion>,
}

fn default_tiers() -> Vec<String> {
    vec!["replay".into(), "live".into()]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvalSuite {
    pub schema: String,
    pub skill: String,
    pub cases: Vec<EvalCase>,
}

impl EvalSuite {
    pub fn parse(json: &str) -> Result<EvalSuite, String> {
        let s: EvalSuite = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if s.schema != SCHEMA {
            return Err(format!(
                "eval suite schema must be {SCHEMA}, got {}",
                s.schema
            ));
        }
        let mut ids = BTreeSet::new();
        for c in &s.cases {
            if !ids.insert(c.id.clone()) {
                return Err(format!("duplicate case id {}", c.id));
            }
            if c.assertions.is_empty() {
                return Err(format!("case {} has no assertions", c.id));
            }
        }
        Ok(s)
    }

    pub fn load(path: &Path) -> Result<EvalSuite, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssertionResult {
    pub kind: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelIdentity {
    pub provider_id: String,
    pub tier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cassette_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseReport {
    pub skill: String,
    pub case: String,
    pub passed: bool,
    pub run_id: String,
    pub run_state: String,
    pub status: RunStatus,
    pub steps: u64,
    pub tool_calls: u64,
    pub context_tokens: u64,
    pub elapsed_ms: u64,
    pub model: ModelIdentity,
    pub assertions: Vec<AssertionResult>,
    #[serde(default)]
    pub cassette_misses: Vec<String>,
    /// Final blackboard for inspection when a case fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blackboard: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuiteReport {
    pub schema: String,
    pub skill: String,
    pub graph_id: String,
    pub graph_version: u32,
    pub runtime_revision: String,
    pub passed: usize,
    pub failed: usize,
    pub cases: Vec<CaseReport>,
}

/// Which provider answers model nodes.
pub enum Tier<'a> {
    /// Cassette replay: CI, no weights.
    Replay,
    /// Live provider; results bound to its identity.
    Live {
        provider: &'a dyn ModelProvider,
        model: ModelRef,
    },
    /// Live provider, recording a cassette to `out` (relative to the case dir).
    Record {
        provider: Arc<dyn ModelProvider>,
        model: ModelRef,
        out: PathBuf,
    },
}

pub struct HarnessOptions<'a> {
    /// Root for artifact fixture paths (repo root for built-in suites).
    pub fixture_root: PathBuf,
    /// Directory of the case file (cassette paths resolve against it).
    pub case_dir: PathBuf,
    pub tier: Tier<'a>,
    /// Include the final blackboard in every report (not only failures).
    pub include_blackboard: bool,
}

/// Run every case of a suite against a graph-bearing skill.
pub fn run_suite(
    skill: &SkillManifest,
    suite: &EvalSuite,
    opts: &HarnessOptions<'_>,
) -> Result<SuiteReport, String> {
    let graph = skill.graph.as_ref().ok_or_else(|| {
        format!(
            "skill {} has no graph; prose skills are not evaluable",
            skill.id
        )
    })?;
    if suite.skill != skill.id {
        return Err(format!(
            "suite is for {} but skill is {}",
            suite.skill, skill.id
        ));
    }
    let mut cases = Vec::new();
    for c in &suite.cases {
        cases.push(run_case(skill, c, opts)?);
    }
    let passed = cases.iter().filter(|c| c.passed).count();
    Ok(SuiteReport {
        schema: "harbor.skill_eval_report/v1".into(),
        skill: skill.id.clone(),
        graph_id: graph.id.clone(),
        graph_version: graph.version,
        runtime_revision: harbor_inference::runtime_identity().to_string(),
        passed,
        failed: cases.len() - passed,
        cases,
    })
}

pub fn run_case(
    skill: &SkillManifest,
    case: &EvalCase,
    opts: &HarnessOptions<'_>,
) -> Result<CaseReport, String> {
    let graph = skill
        .graph
        .as_ref()
        .ok_or_else(|| format!("skill {} has no graph", skill.id))?;
    let started = Instant::now();
    // Fixtures.
    let mut artifacts = MemoryArtifacts::new();
    for (id, fx) in &case.artifacts {
        let path = opts.fixture_root.join(&fx.path);
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("case {}: fixture {}: {e}", case.id, path.display()))?;
        let name = fx.name.clone().unwrap_or_else(|| {
            path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        artifacts.insert(id, &name, bytes);
    }
    // Sandboxed durable substrate per case.
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.path().join("db")).map_err(|e| e.to_string())?;
    let log = Arc::new(EventLog::open(lease_db_path(dir.path())).map_err(|e| e.to_string())?);
    let store = FileStateStore::new(dir.path().join("runs"));
    let registry = ToolRegistry::builtin();
    let cancel = AtomicBool::new(false);

    // Provider per tier.
    let cassette_path = case.cassette.as_ref().map(|c| opts.case_dir.join(c));
    let cassette_sha = cassette_path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| harbor_canonical::sha256_hex(&b));
    let replay: Option<RecordReplayProvider> = match &opts.tier {
        Tier::Replay => {
            let cassette = match &cassette_path {
                Some(p) if p.exists() => Cassette::load(p).map_err(|e| e.to_string())?,
                _ => Cassette::default(),
            };
            Some(RecordReplayProvider::replay(cassette))
        }
        Tier::Record { provider, .. } => Some(RecordReplayProvider::record(
            provider.clone(),
            Cassette::default(),
        )),
        Tier::Live { .. } => None,
    };
    let (provider, model, tier_name): (Option<&dyn ModelProvider>, Option<ModelRef>, &str) =
        match &opts.tier {
            Tier::Replay => (
                replay.as_ref().map(|p| p as &dyn ModelProvider),
                Some(ModelRef::InstalledPackage {
                    package_id: "cassette".into(),
                }),
                "replay",
            ),
            Tier::Live { provider, model } => (Some(*provider), Some(model.clone()), "live"),
            Tier::Record { model, .. } => (
                replay.as_ref().map(|p| p as &dyn ModelProvider),
                Some(model.clone()),
                "record",
            ),
        };
    let provider_id = provider
        .map(|p| p.id().to_string())
        .unwrap_or_else(|| "none".into());

    let exec = Executor::new(Host {
        log: log.clone(),
        lease_db: lease_db_path(dir.path()),
        store: &store,
        registry: &registry,
        provider,
        artifacts: &artifacts,
        knowledge: None,
        workspace_root: None,
        cancel: &cancel,
        executor_id: "harness".into(),
        commit_journal: None,
    });
    let report = exec
        .start(RunRequest {
            run_id: None,
            workspace_id: "eval".into(),
            graph: graph.clone(),
            skill_id: Some(skill.id.clone()),
            skill_instructions: Some(skill.instructions.clone()),
            inputs: case.inputs.clone(),
            host_inputs: case.host_inputs.clone(),
            model,
        })
        .map_err(|e| format!("case {}: {e}", case.id))?;

    // Record tier writes the cassette next to the case file.
    if let (Tier::Record { out, .. }, Some(rr)) = (&opts.tier, &replay) {
        let path = opts.case_dir.join(out);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        rr.cassette().save(&path).map_err(|e| e.to_string())?;
    }
    let misses: Vec<String> = replay
        .as_ref()
        .map(|p| {
            p.misses()
                .into_iter()
                .map(|m| m.trace_key.unwrap_or(m.request_hash))
                .collect()
        })
        .unwrap_or_default();

    // Assertions.
    let graph_tools = graph.tools();
    let events = log.load_stream(&report.run_id).map_err(|e| e.to_string())?;
    let tools_used: Vec<String> = events
        .iter()
        .filter_map(|e| match &e.payload {
            EventPayload::StepCompleted { tool: Some(t), .. } => Some(t.clone()),
            _ => None,
        })
        .collect();
    let replay_report = log.replay(&report.run_id).map_err(|e| e.to_string())?;
    let state = &report.blackboard;
    let mut results = Vec::new();
    for a in &case.assertions {
        let (passed, detail) = check(
            a,
            state,
            &report,
            &graph_tools,
            &tools_used,
            &replay_report,
            &misses,
        );
        results.push(AssertionResult {
            kind: kind_name(a),
            passed,
            detail,
        });
    }
    let passed = results.iter().all(|r| r.passed);
    let executed_on = report
        .trail
        .iter()
        .filter_map(|t| t.executed_on.clone())
        .next();
    Ok(CaseReport {
        skill: skill.id.clone(),
        case: case.id.clone(),
        passed,
        run_id: report.run_id.clone(),
        run_state: report.state.clone(),
        status: report.status.clone(),
        steps: report.steps,
        tool_calls: report.tool_calls,
        context_tokens: report.context_tokens,
        elapsed_ms: started.elapsed().as_millis() as u64,
        model: ModelIdentity {
            provider_id,
            tier: tier_name.into(),
            executed_on,
            cassette_sha256: cassette_sha,
        },
        assertions: results,
        cassette_misses: misses,
        blackboard: if opts.include_blackboard || !passed {
            Some(report.blackboard.clone())
        } else {
            None
        },
    })
}

fn kind_name(a: &Assertion) -> String {
    serde_json::to_value(a)
        .ok()
        .and_then(|v| v.get("kind").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

fn strings_at<'a>(state: &'a Value, pointer: &str) -> Vec<&'a str> {
    pointer::get_all(state, pointer)
        .into_iter()
        .filter_map(Value::as_str)
        .collect()
}

fn check(
    a: &Assertion,
    state: &Value,
    report: &crate::executor::RunReport,
    graph_tools: &BTreeSet<String>,
    tools_used: &[String],
    replay: &harbor_agent::log::ReplayReport,
    misses: &[String],
) -> (bool, String) {
    match a {
        Assertion::RunState { equals } => (
            report.state == *equals,
            format!("run state {}", report.state),
        ),
        Assertion::Outcome { equals } => {
            let actual = state.get("outcome").and_then(Value::as_str).unwrap_or("-");
            (actual == equals, format!("outcome {actual}"))
        }
        Assertion::StateEquals { pointer, value } => {
            let actual = pointer::get(state, pointer);
            (
                actual == Some(value),
                format!(
                    "{pointer} = {}",
                    actual.map(|v| v.to_string()).unwrap_or("(missing)".into())
                ),
            )
        }
        Assertion::StateExists { pointer } => {
            let ok = pointer::get(state, pointer)
                .map(|v| !v.is_null())
                .unwrap_or(false);
            (
                ok,
                format!("{pointer} {}", if ok { "exists" } else { "missing" }),
            )
        }
        Assertion::StateMissing { pointer } => {
            let ok = pointer::get(state, pointer)
                .map(|v| v.is_null())
                .unwrap_or(true);
            (
                ok,
                format!("{pointer} {}", if ok { "missing" } else { "present" }),
            )
        }
        Assertion::StateNonempty { pointer } => {
            let v = pointer::get(state, pointer);
            let ok = match v {
                Some(Value::String(s)) => !s.trim().is_empty(),
                Some(Value::Array(a)) => !a.is_empty(),
                Some(Value::Object(m)) => !m.is_empty(),
                Some(Value::Null) | None => false,
                Some(_) => true,
            };
            (
                ok,
                format!("{pointer} {}", if ok { "nonempty" } else { "empty" }),
            )
        }
        Assertion::StateEmpty { pointer } => {
            let v = pointer::get(state, pointer);
            let ok = match v {
                Some(Value::String(s)) => s.trim().is_empty(),
                Some(Value::Array(a)) => a.is_empty(),
                Some(Value::Object(m)) => m.is_empty(),
                Some(Value::Null) | None => true,
                Some(_) => false,
            };
            (
                ok,
                format!("{pointer} {}", if ok { "empty" } else { "nonempty" }),
            )
        }
        Assertion::ArrayLen { pointer, min, max } => {
            let n = pointer::get(state, pointer)
                .and_then(Value::as_array)
                .map(|a| a.len());
            let ok = n
                .map(|n| min.map(|m| n >= m).unwrap_or(true) && max.map(|m| n <= m).unwrap_or(true))
                .unwrap_or(false);
            (
                ok,
                format!(
                    "{pointer} has {} items",
                    n.map(|n| n.to_string()).unwrap_or("no array".into())
                ),
            )
        }
        Assertion::StringLen { pointer, max } => {
            let strs = strings_at(state, pointer);
            let worst = strs.iter().map(|s| s.chars().count()).max().unwrap_or(0);
            (
                worst <= *max,
                format!("{} strings, longest {worst} chars (max {max})", strs.len()),
            )
        }
        Assertion::Contains { pointer, text } => {
            let ok = strings_at(state, pointer)
                .iter()
                .any(|s| s.contains(text.as_str()))
                || pointer::get(state, pointer)
                    .map(|v| v.to_string().contains(text.as_str()))
                    .unwrap_or(false);
            (
                ok,
                format!(
                    "{pointer} {} {text:?}",
                    if ok { "contains" } else { "lacks" }
                ),
            )
        }
        Assertion::NotContains { pointer, text } => {
            let found = strings_at(state, pointer)
                .iter()
                .any(|s| s.contains(text.as_str()))
                || pointer::get(state, pointer)
                    .map(|v| v.to_string().contains(text.as_str()))
                    .unwrap_or(false);
            (
                !found,
                format!(
                    "{pointer} {} {text:?}",
                    if found { "contains" } else { "lacks" }
                ),
            )
        }
        Assertion::ValuesAppearIn {
            pointer,
            source,
            allow_null,
        } => {
            let src: String = match pointer::get(state, source) {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            let values = pointer::get_all(state, pointer);
            let mut missing = Vec::new();
            for v in &values {
                match v {
                    Value::Null => {
                        if !allow_null {
                            missing.push("null".to_string());
                        }
                    }
                    Value::String(s) => {
                        if !src.contains(s.as_str()) {
                            missing.push(s.clone());
                        }
                    }
                    other => missing.push(other.to_string()),
                }
            }
            (
                missing.is_empty(),
                if missing.is_empty() {
                    format!("{} values all appear in {source}", values.len())
                } else {
                    format!("not in {source}: {missing:?}")
                },
            )
        }
        Assertion::StateMatchesSchema { pointer, schema } => {
            let v = pointer::get(state, pointer).cloned().unwrap_or(Value::Null);
            let violations = jsonschema::validate(schema, &v);
            (
                violations.is_empty(),
                if violations.is_empty() {
                    format!("{pointer} matches schema")
                } else {
                    violations
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                },
            )
        }
        Assertion::ApprovalRequested => {
            let ok = matches!(report.status, RunStatus::WaitingApproval { .. });
            (ok, format!("run status {}", report.state))
        }
        Assertion::BatchProposed { pointer, min_ops } => {
            let n = pointer::get(state, pointer)
                .and_then(|b| b.get("operations"))
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            (n >= *min_ops, format!("{pointer} has {n} operations"))
        }
        Assertion::ToolsWithinGraph => {
            let outside: Vec<&String> = tools_used
                .iter()
                .filter(|t| !graph_tools.contains(*t))
                .collect();
            (
                outside.is_empty(),
                if outside.is_empty() {
                    format!("{} tool calls, all declared", tools_used.len())
                } else {
                    format!("undeclared tools: {outside:?}")
                },
            )
        }
        Assertion::StepsLe { max } => (
            report.steps <= *max,
            format!("{} steps (max {max})", report.steps),
        ),
        Assertion::ToolCallsLe { max } => (
            report.tool_calls <= *max,
            format!("{} tool calls (max {max})", report.tool_calls),
        ),
        Assertion::ReplayVerified => {
            let ok = replay.verified_events as u64 == report.events
                && replay
                    .final_state
                    .map(|s| s.as_str() == report.state)
                    .unwrap_or(false);
            (
                ok,
                format!(
                    "{} of {} events verified, final {:?}",
                    replay.verified_events, report.events, replay.final_state
                ),
            )
        }
        Assertion::NoCassetteMisses => (
            misses.is_empty(),
            if misses.is_empty() {
                "no misses".into()
            } else {
                format!("misses: {misses:?}")
            },
        ),
    }
}

/// Built-in suite files live under `evals/skills/<skill-id>/cases.json`.
pub fn builtin_suite_path(repo_root: &Path, skill_id: &str) -> PathBuf {
    repo_root
        .join("evals")
        .join("skills")
        .join(skill_id)
        .join("cases.json")
}

/// Convenience: run a built-in skill's suite from the repository layout.
pub fn run_builtin_suite(
    repo_root: &Path,
    skill: &SkillManifest,
    tier: Tier<'_>,
) -> Result<SuiteReport, String> {
    let path = builtin_suite_path(repo_root, &skill.id);
    let suite = EvalSuite::load(&path)?;
    let opts = HarnessOptions {
        fixture_root: repo_root.to_path_buf(),
        case_dir: path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| repo_root.to_path_buf()),
        tier,
        include_blackboard: false,
    };
    run_suite(skill, &suite, &opts)
}

pub fn report_json(r: &SuiteReport) -> Value {
    serde_json::to_value(r).unwrap_or_else(|_| json!({}))
}
