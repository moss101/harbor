//! Graph executor over the durable run substrate.
//!
//! Authority: `03_Architecture_Contracts.md` §4/§5, `02_Runtime_Effect_and_Artifact_Contracts.md`,
//! decision 0006. One run executes one skill graph:
//!
//! - lifecycle on `harbor_agent`: `CREATED → PLANNING → RUNNING → …` with a
//!   generation-fenced lease and a hash-chained event log; every node
//!   execution is a `run.step_started/completed` pair carrying the node id
//!   and input/output hashes, so `run.replay` is a structural check;
//! - the blackboard (`serde_json::Value`) is persisted through a
//!   [`RunStateStore`] after every node, so a killed process resumes from
//!   the last completed node and an approval can be decided later;
//! - budgets are checked before every node against the durable counters;
//! - an `approval` node prepares the effect (canonical args hash, base and
//!   proposed output hashes), requests approval and parks the run in
//!   `WAITING_APPROVAL`; nothing is committed here — the host performs the
//!   protected effect under the receipt after [`Executor::decide`];
//! - the model is called only inside `model.*` nodes, always through the
//!   provider contract, with structured outputs constrained by schema when
//!   the provider supports it and validated below the model regardless.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use harbor_agent::budgets::{BudgetDelta, Budgets};
use harbor_agent::{
    Actor, Counters, EventLog, EventPayload, EventType, ExecutorLease, LeaseManager, PauseReason,
    ReplaySemantics, RunEvent, RunState,
};
use harbor_canonical::JsonValue;
use harbor_inference::provider::{Capabilities, ChatRequest, ModelProvider, ModelRef};

use crate::graph::{self, Edge, Graph, Node, Outcome};
use crate::jsonschema;
use crate::pointer;
use crate::tools::{ArtifactSource, KnowledgeSearch, ToolContext, ToolError, ToolRegistry};

pub const SNAPSHOT_SCHEMA: &str = "harbor.run_snapshot/v1";

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("agent log: {0}")]
    Log(#[from] harbor_agent::LogError),
    #[error("lease: {0}")]
    Lease(#[from] harbor_agent::LeaseError),
    #[error("graph: {0}")]
    Graph(#[from] graph::GraphError),
    #[error("planning failed: {0}")]
    Planning(String),
    #[error("run {0} not found in the state store")]
    NoSnapshot(String),
    #[error("run {run_id} is {state}, expected {expected}")]
    WrongState {
        run_id: String,
        state: String,
        expected: String,
    },
    #[error("state store: {0}")]
    Store(String),
    /// Refused before anything durable happened (original untouched, run
    /// still WAITING_APPROVAL).
    #[error("commit refused: {0}")]
    Commit(String),
    /// The dispatch was recorded and the write did not publish; the run is
    /// FAILED with the reason and the original is untouched.
    #[error("commit {outcome}: {error}")]
    CommitFailed {
        outcome: String,
        error: String,
        report: Box<RunReport>,
    },
    #[error("{0}")]
    Other(String),
}

/// Serializable model reference (the provider enum has no serde derive).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelRefJson {
    InstalledPackage {
        package_id: String,
    },
    SystemManaged {
        provider_id: String,
        model_id: String,
    },
    RemoteEndpoint {
        profile_id: String,
        endpoint: String,
        model: String,
    },
}

impl From<&ModelRef> for ModelRefJson {
    fn from(m: &ModelRef) -> Self {
        match m {
            ModelRef::InstalledPackage { package_id } => ModelRefJson::InstalledPackage {
                package_id: package_id.clone(),
            },
            ModelRef::SystemManaged {
                provider_id,
                model_id,
            } => ModelRefJson::SystemManaged {
                provider_id: provider_id.clone(),
                model_id: model_id.clone(),
            },
            ModelRef::RemoteEndpoint {
                profile_id,
                endpoint,
                model,
            } => ModelRefJson::RemoteEndpoint {
                profile_id: profile_id.clone(),
                endpoint: endpoint.clone(),
                model: model.clone(),
            },
        }
    }
}

impl From<&ModelRefJson> for ModelRef {
    fn from(m: &ModelRefJson) -> Self {
        match m {
            ModelRefJson::InstalledPackage { package_id } => ModelRef::InstalledPackage {
                package_id: package_id.clone(),
            },
            ModelRefJson::SystemManaged {
                provider_id,
                model_id,
            } => ModelRef::SystemManaged {
                provider_id: provider_id.clone(),
                model_id: model_id.clone(),
            },
            ModelRefJson::RemoteEndpoint {
                profile_id,
                endpoint,
                model,
            } => ModelRef::RemoteEndpoint {
                profile_id: profile_id.clone(),
                endpoint: endpoint.clone(),
                model: model.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeTrace {
    pub step_id: String,
    pub node_id: String,
    pub kind: String,
    pub iteration: u32,
    pub input_hash: String,
    pub output_hash: Option<String>,
    pub elapsed_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingApproval {
    pub node_id: String,
    pub effect_id: String,
    pub receipt_id: String,
    pub effect_class: String,
    pub canonical_args_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_output_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    pub batch: Value,
    /// Before/after view of the batch against the base bytes (computed
    /// while the artifact is still attached, so review needs no bytes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<crate::tools::builtin::DiffEntry>,
    /// When the approval was requested; the receipt it implies is valid
    /// for `RECEIPT_VALIDITY` from here (02 contract: ≤ 15 minutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Single-effect receipts are valid for at most 15 minutes (02 contract,
/// "Receipts"). A commit after that window needs a fresh approval.
pub const RECEIPT_VALIDITY: chrono::Duration = chrono::Duration::minutes(15);

/// Where an approved proposal is written.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CommitTarget {
    /// Write the approved output to `destination`, which must not exist;
    /// the original is never touched. The default.
    SaveNewCopy { destination: PathBuf },
    /// Replace `destination` in place, only if its bytes still hash to the
    /// approved base (compare-and-swap inside the protected interval).
    Overwrite { destination: PathBuf },
}

impl CommitTarget {
    pub fn destination(&self) -> &Path {
        match self {
            CommitTarget::SaveNewCopy { destination } | CommitTarget::Overwrite { destination } => {
                destination
            }
        }
    }

    pub fn mode(&self) -> &'static str {
        match self {
            CommitTarget::SaveNewCopy { .. } => "new_copy",
            CommitTarget::Overwrite { .. } => "provider_compare_and_swap",
        }
    }
}

/// What a commit did, bound to the approval it consumed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitReport {
    pub run_id: String,
    pub node_id: String,
    pub effect_id: String,
    pub receipt_id: String,
    pub attempt_id: String,
    pub batch_id: String,
    pub artifact_id: String,
    pub mode: String,
    pub destination: PathBuf,
    pub version_id: String,
    pub bytes_written: u64,
    pub base_content_hash: String,
    pub proposed_output_hash: String,
    /// `committed` or `already_committed` (idempotent replay of the batch).
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutcomeRecord {
    pub outcome: Outcome,
    pub outputs: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Position {
    /// Node to execute next (None once the run is terminal).
    pub next_node: Option<String>,
    /// Traversal counts of bounded edges, keyed `from->to`.
    #[serde(default)]
    pub edge_counts: BTreeMap<String, u32>,
    /// Executions per node (1-based iteration numbers for trace keys).
    #[serde(default)]
    pub node_runs: BTreeMap<String, u32>,
}

/// Durable picture of a run between nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunSnapshot {
    pub schema: String,
    pub run_id: String,
    pub workspace_id: String,
    pub graph: Graph,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRefJson>,
    pub state: Value,
    pub position: Position,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_approval: Option<PendingApproval>,
    #[serde(default)]
    pub trail: Vec<NodeTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<OutcomeRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Hash of `state` at the last save (replay/consistency checks).
    pub state_hash: String,
}

pub trait RunStateStore: Send + Sync {
    fn save(&self, snapshot: &RunSnapshot) -> Result<(), ExecError>;
    fn load(&self, run_id: &str) -> Result<Option<RunSnapshot>, ExecError>;
}

/// Plaintext JSON files (`<dir>/<run_id>.json`): harness, tests and
/// development. The product path wraps snapshots in the encrypted
/// workspace blob store (policy 13) — see the FFI.
pub struct FileStateStore {
    dir: PathBuf,
}

impl FileStateStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        FileStateStore { dir: dir.into() }
    }
}

impl RunStateStore for FileStateStore {
    fn save(&self, snapshot: &RunSnapshot) -> Result<(), ExecError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| ExecError::Store(e.to_string()))?;
        let path = self.dir.join(format!("{}.json", snapshot.run_id));
        let tmp = self.dir.join(format!("{}.json.tmp", snapshot.run_id));
        let text =
            serde_json::to_string_pretty(snapshot).map_err(|e| ExecError::Store(e.to_string()))?;
        std::fs::write(&tmp, text).map_err(|e| ExecError::Store(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| ExecError::Store(e.to_string()))
    }

    fn load(&self, run_id: &str) -> Result<Option<RunSnapshot>, ExecError> {
        let path = self.dir.join(format!("{run_id}.json"));
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| ExecError::Store(e.to_string()))?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| ExecError::Store(e.to_string()))
    }
}

/// Everything needed to start a run.
pub struct RunRequest {
    pub run_id: Option<String>,
    pub workspace_id: String,
    pub graph: Graph,
    pub skill_id: Option<String>,
    /// Skill-level prose (system preamble for every model node).
    pub skill_instructions: Option<String>,
    pub inputs: Value,
    pub host_inputs: Value,
    pub model: Option<ModelRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RunStatus {
    Completed { outcome: Outcome, outputs: Value },
    WaitingApproval { approval: Box<PendingApproval> },
    Failed { error: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunReport {
    pub run_id: String,
    pub state: String,
    pub status: RunStatus,
    pub steps: u64,
    pub tool_calls: u64,
    pub context_tokens: u64,
    pub events: u64,
    pub trail: Vec<NodeTrace>,
    pub state_hash: String,
    /// Final blackboard (inputs included) for evals and display.
    pub blackboard: Value,
}

/// Host resources for one executor instance.
pub struct Host<'a> {
    pub log: Arc<EventLog>,
    /// Path of the lease database (`<data_root>/db/agent.db`).
    pub lease_db: PathBuf,
    pub store: &'a dyn RunStateStore,
    pub registry: &'a ToolRegistry,
    pub provider: Option<&'a dyn ModelProvider>,
    pub artifacts: &'a dyn ArtifactSource,
    pub knowledge: Option<&'a dyn KnowledgeSearch>,
    pub workspace_root: Option<PathBuf>,
    pub cancel: &'a AtomicBool,
    pub executor_id: String,
    /// Commit journal database (`commit_journal_path`); `None` on hosts
    /// that never commit (the eval harness).
    pub commit_journal: Option<PathBuf>,
    /// Called with each node id as the run reaches it. A long-running
    /// host reports it as the operation's phase, so a run in flight says
    /// which step it is on rather than a flat "running" — useful to the
    /// surface above it, and the difference between "it stopped" and "it
    /// stopped HERE" if one ever does. `None` for hosts with nothing to
    /// report to (tests, the harness).
    pub step: Option<&'a dyn Fn(&str)>,
}

pub struct Executor<'a> {
    host: Host<'a>,
    lease_ttl: chrono::Duration,
}

struct RunCursor {
    lease: ExecutorLease,
    seq: u64,
    head_hash: String,
    counters: Counters,
}

/// Whether the executor loop should stop, and why.
enum Flow {
    Continue(Option<String>),
    Stop(RunStatus),
}

fn now() -> chrono::DateTime<chrono::Utc> {
    crate::Workspace::now()
}

fn semantics_for(t: &EventType) -> ReplaySemantics {
    match t {
        EventType::RunCreated | EventType::RunTransition => ReplaySemantics::StateAffecting,
        EventType::RunStepStarted | EventType::RunStepCompleted | EventType::RunNote => {
            ReplaySemantics::StateAffecting
        }
        _ => ReplaySemantics::AuthorityAffecting,
    }
}

fn budgets_of(g: &Graph) -> Budgets {
    Budgets {
        active_compute_ms: None,
        tool_calls: Some(g.budgets.max_tool_calls as u64),
        context_tokens: g.budgets.max_context_tokens,
        steps: Some(g.budgets.max_steps as u64),
    }
}

/// First JSON object/array inside `text` (models often wrap JSON in prose
/// or code fences even when asked not to).
pub fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let fenced = t
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(v) = serde_json::from_str::<Value>(fenced) {
        return Some(v);
    }
    // Salvage only a value that starts where the FIRST structural opener
    // is, and give up if that span does not parse.
    //
    // This used to try ('{','}') and then ('[',']') independently. On a
    // grammar-constrained object truncated at the token budget the outer
    // `{` never closes, so the object attempt failed and the array attempt
    // matched a COMPLETE array nested inside it — which was then handed to
    // the validator as the model's answer. The run failed with
    // `/: expected type "object", got array`, an error that describes the
    // model's shape and says nothing about the budget that actually caused
    // it. A substructure is never a correct reading of a truncated value.
    //
    // The cost is a contrived case like `See [1] here: {"a":1}`, where the
    // first opener belongs to prose rather than to the value. Returning
    // None there is the right trade: with a schema in force the model emits
    // JSON and nothing else, and a wrong value is worse than no value.
    let first = t
        .char_indices()
        .find(|(_, c)| *c == '{' || *c == '[')
        .map(|(i, c)| (i, if c == '{' { '}' } else { ']' }));
    if let Some((s, close)) = first {
        if let Some(e) = t.rfind(close) {
            if e > s {
                if let Ok(v) = serde_json::from_str::<Value>(&t[s..=e]) {
                    return Some(v);
                }
            }
        }
    }
    None
}

impl<'a> Executor<'a> {
    pub fn new(host: Host<'a>) -> Self {
        Executor {
            host,
            lease_ttl: chrono::Duration::minutes(10),
        }
    }

    // --- event plumbing ----------------------------------------------------

    fn open_cursor(&self, run_id: &str) -> Result<RunCursor, ExecError> {
        let mut leases = LeaseManager::open(&self.host.lease_db)?;
        let lease = leases.acquire(run_id, &self.host.executor_id, self.lease_ttl, now())?;
        let stream = self.host.log.load_stream(run_id)?;
        let head = stream
            .last()
            .ok_or_else(|| ExecError::Other("run has no events".into()))?;
        let head_hash = head
            .hash()
            .map_err(|e| ExecError::Other(format!("hash: {e}")))?;
        Ok(RunCursor {
            lease,
            seq: stream.len() as u64,
            head_hash,
            counters: head.counters,
        })
    }

    /// Every lease acquisition is an authority-affecting event: a restart
    /// (`resume`) or a decision (`decide`) is a new executor generation and
    /// must be visible in the stream, not only in the lease table.
    fn lease_acquired(&self, cur: &mut RunCursor, run_id: &str) -> Result<(), ExecError> {
        let generation = cur.lease.generation;
        self.emit(
            cur,
            run_id,
            Actor::Executor,
            EventType::RunLeaseAcquired,
            EventPayload::LeaseAcquired { generation },
            BudgetDelta {
                active_compute_ms: 0,
                steps: 0,
                tool_calls: 0,
                context_tokens: 0,
            },
        )
    }

    /// Release the executor lease when this executor returns control:
    /// a parked, completed, failed or cancelled run must be resumable by
    /// another executor (next generation), never blocked by a stale lease.
    fn release(&self, cur: &RunCursor) {
        if let Ok(mut leases) = LeaseManager::open(&self.host.lease_db) {
            let _ = leases.release(&cur.lease, now());
        }
    }

    fn emit(
        &self,
        cur: &mut RunCursor,
        run_id: &str,
        actor: Actor,
        event_type: EventType,
        payload: EventPayload,
        delta: BudgetDelta,
    ) -> Result<(), ExecError> {
        let counters = Counters {
            active_compute_ms_total: cur.counters.active_compute_ms_total + delta.active_compute_ms,
            step_count_total: cur.counters.step_count_total + delta.steps,
            tool_count_total: cur.counters.tool_count_total + delta.tool_calls,
            context_tokens_total: cur.counters.context_tokens_total + delta.context_tokens,
        };
        let created_at = now();
        let event = RunEvent {
            run_id: run_id.to_string(),
            event_id: format!(
                "evt-{}",
                harbor_canonical::sha256_hex(
                    format!(
                        "{run_id}-{}-{}-{}",
                        event_type.as_str(),
                        cur.seq,
                        created_at.to_rfc3339()
                    )
                    .as_bytes()
                )
                .get(..16)
                .unwrap_or("evt")
            ),
            seq: cur.seq,
            replay_semantics: semantics_for(&event_type),
            event_type,
            actor,
            lease_generation: cur.lease.generation,
            counters,
            payload,
            created_at,
            prev_event_hash: Some(cur.head_hash.clone()),
        };
        let hash = self
            .host
            .log
            .append(event, cur.lease.generation, Some(delta))?;
        cur.head_hash = hash;
        cur.seq += 1;
        cur.counters = counters;
        Ok(())
    }

    fn transition(
        &self,
        cur: &mut RunCursor,
        run_id: &str,
        from: RunState,
        to: RunState,
        reason: Option<PauseReason>,
    ) -> Result<(), ExecError> {
        self.emit(
            cur,
            run_id,
            Actor::Executor,
            EventType::RunTransition,
            EventPayload::Transition {
                from_state: from,
                to_state: to,
                reason,
            },
            BudgetDelta {
                active_compute_ms: 0,
                steps: 0,
                tool_calls: 0,
                context_tokens: 0,
            },
        )
    }

    fn note(&self, cur: &mut RunCursor, run_id: &str, text: String) -> Result<(), ExecError> {
        self.emit(
            cur,
            run_id,
            Actor::Executor,
            EventType::RunNote,
            EventPayload::Note { text },
            BudgetDelta {
                active_compute_ms: 0,
                steps: 0,
                tool_calls: 0,
                context_tokens: 0,
            },
        )
    }

    fn save(&self, snap: &mut RunSnapshot) -> Result<(), ExecError> {
        snap.state_hash = pointer::stable_hash(&snap.state);
        self.host.store.save(snap)
    }

    fn report(
        &self,
        snap: &RunSnapshot,
        cur: &RunCursor,
        status: RunStatus,
        state: RunState,
    ) -> RunReport {
        RunReport {
            run_id: snap.run_id.clone(),
            state: state.as_str().to_string(),
            status,
            steps: cur.counters.step_count_total,
            tool_calls: cur.counters.tool_count_total,
            context_tokens: cur.counters.context_tokens_total,
            events: cur.seq,
            trail: snap.trail.clone(),
            state_hash: snap.state_hash.clone(),
            blackboard: snap.state.clone(),
        }
    }

    // --- lifecycle ---------------------------------------------------------

    /// Create and execute a run until it completes, fails, is cancelled or
    /// waits for an approval.
    pub fn start(&self, req: RunRequest) -> Result<RunReport, ExecError> {
        req.graph.validate()?;
        let run_id = req
            .run_id
            .clone()
            .unwrap_or_else(|| harbor_security::HarborId::generate("run").to_string());
        self.host
            .log
            .create_run(&run_id, &req.workspace_id, now())?;
        let mut cur = self.open_cursor(&run_id)?;
        self.lease_acquired(&mut cur, &run_id)?;
        self.transition(
            &mut cur,
            &run_id,
            RunState::Created,
            RunState::Planning,
            None,
        )?;

        let mut snap = RunSnapshot {
            schema: SNAPSHOT_SCHEMA.into(),
            run_id: run_id.clone(),
            workspace_id: req.workspace_id.clone(),
            graph: req.graph.clone(),
            skill_id: req.skill_id.clone(),
            skill_instructions: req.skill_instructions.clone(),
            model: req.model.as_ref().map(ModelRefJson::from),
            state: json!({}),
            position: Position {
                next_node: Some(req.graph.entry.clone()),
                ..Default::default()
            },
            pending_approval: None,
            trail: Vec::new(),
            outcome: None,
            last_error: None,
            state_hash: String::new(),
        };

        // PLANNING: bind inputs and prove the graph can run on this host
        // before any node executes.
        if let Err(e) = self.plan(&req) {
            self.note(&mut cur, &run_id, format!("planning failed: {e}"))?;
            self.transition(
                &mut cur,
                &run_id,
                RunState::Planning,
                RunState::Failed,
                None,
            )?;
            snap.last_error = Some(e.to_string());
            snap.position.next_node = None;
            self.save(&mut snap)?;
            self.release(&cur);
            return Ok(self.report(
                &snap,
                &cur,
                RunStatus::Failed {
                    error: e.to_string(),
                },
                RunState::Failed,
            ));
        }
        snap.state = json!({ "input": req.inputs, "host": req.host_inputs });
        self.transition(
            &mut cur,
            &run_id,
            RunState::Planning,
            RunState::Running,
            None,
        )?;
        self.save(&mut snap)?;
        self.run_loop(&mut cur, &mut snap)
    }

    fn plan(&self, req: &RunRequest) -> Result<(), ExecError> {
        let g = &req.graph;
        let v = jsonschema::validate(&g.inputs, &req.inputs);
        if !v.is_empty() {
            return Err(ExecError::Planning(format!(
                "inputs do not satisfy the graph input schema: {}",
                v.iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        let tools = g.tools();
        for t in &tools {
            if self.host.registry.spec(t).is_none() {
                return Err(ExecError::Planning(format!(
                    "tool {t} is not registered on this host"
                )));
            }
        }
        let missing = self.host.registry.missing_requirements(&tools);
        if !missing.is_empty() {
            return Err(ExecError::Planning(format!(
                "host lacks required capabilities: {}",
                missing.join(", ")
            )));
        }
        if !g.model_nodes().is_empty() {
            let provider = self.host.provider.ok_or_else(|| {
                ExecError::Planning("graph has model nodes but no provider is bound".into())
            })?;
            let model = req.model.as_ref().ok_or_else(|| {
                ExecError::Planning("graph has model nodes but no model is bound".into())
            })?;
            if !provider.supports(model, &Capabilities::Chat) {
                return Err(ExecError::Planning(format!(
                    "provider {} does not support chat for the bound model",
                    provider.id()
                )));
            }
            provider
                .load(model)
                .map_err(|e| ExecError::Planning(format!("model load: {e}")))?;
        }
        Ok(())
    }

    /// Decide a pending approval and continue the run.
    pub fn decide(&self, run_id: &str, approved: bool) -> Result<RunReport, ExecError> {
        let mut snap = self
            .host
            .store
            .load(run_id)?
            .ok_or_else(|| ExecError::NoSnapshot(run_id.into()))?;
        let (state, _, _) = self.host.log.run_state(run_id)?;
        if state != RunState::WaitingApproval {
            return Err(ExecError::WrongState {
                run_id: run_id.into(),
                state: state.as_str().into(),
                expected: RunState::WaitingApproval.as_str().into(),
            });
        }
        let pending = snap
            .pending_approval
            .clone()
            .ok_or_else(|| ExecError::Other("run is waiting but has no pending approval".into()))?;
        let mut cur = self.open_cursor(run_id)?;
        self.lease_acquired(&mut cur, run_id)?;
        let zero = BudgetDelta {
            active_compute_ms: 0,
            steps: 0,
            tool_calls: 0,
            context_tokens: 0,
        };
        self.emit(
            &mut cur,
            run_id,
            Actor::User,
            EventType::RunApprovalDecided,
            EventPayload::ApprovalDecided {
                effect_id: pending.effect_id.clone(),
                approved,
            },
            zero,
        )?;
        if !approved {
            self.emit(
                &mut cur,
                run_id,
                Actor::Executor,
                EventType::RunEffectResolved,
                EventPayload::EffectResolved {
                    effect_id: pending.effect_id.clone(),
                    outcome: "aborted".into(),
                },
                zero,
            )?;
        }
        self.transition(
            &mut cur,
            run_id,
            RunState::WaitingApproval,
            RunState::Running,
            None,
        )?;
        // Record the decision on the blackboard for downstream nodes.
        let _ = pointer::set_unchecked(
            &mut snap.state,
            &format!("/approvals/{}", pending.node_id),
            json!({
                "approved": approved,
                "effect_id": pending.effect_id,
                "receipt_id": pending.receipt_id,
                "canonical_args_hash": pending.canonical_args_hash,
                "proposed_output_hash": pending.proposed_output_hash,
            }),
        );
        if let Some(t) = snap
            .trail
            .iter_mut()
            .rev()
            .find(|t| t.node_id == pending.node_id)
        {
            t.decision = Some(if approved {
                "approved".into()
            } else {
                "rejected".into()
            });
        }
        let next = match snap.graph.node(&pending.node_id) {
            Some(Node::Approval {
                next_approved,
                next_rejected,
                ..
            }) => {
                let edge = if approved {
                    next_approved.clone()
                } else {
                    next_rejected.clone()
                };
                edge.map(|e| self.follow(&mut snap.position, &pending.node_id, &e))
            }
            _ => None,
        };
        snap.pending_approval = None;
        snap.position.next_node = next;
        self.save(&mut snap)?;
        self.run_loop(&mut cur, &mut snap)
    }

    /// Before/after entries for a proposal while its artifact is attached.
    fn proposal_diff(&self, batch_value: &Value) -> Vec<crate::tools::builtin::DiffEntry> {
        let Some(artifact_id) = batch_value.get("artifact_id").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(bytes) = self.host.artifacts.get(artifact_id) else {
            return Vec::new();
        };
        match crate::tools::builtin::batch_from_value(batch_value) {
            Ok(batch) => crate::tools::builtin::proposal_diff(&bytes.bytes, &batch),
            Err(_) => Vec::new(),
        }
    }

    /// Approve the pending proposal and commit it as one protected effect,
    /// then continue the graph from the approval node's `next_approved`
    /// edge. This is the host effect of 02 §"Artifact batches and safe
    /// save", executed under the run's lease while its receipt is valid:
    ///
    /// 1. the base bytes (re-supplied by the host through `artifacts`) must
    ///    hash to the approved `base_content_hash`; the batch is re-applied
    ///    and the result must hash to the approved `proposed_output_hash`
    ///    before anything touches the destination;
    /// 2. `run.approval_decided` + `run.effect_dispatched` are durable
    ///    before the write; `run.effect_resolved` records the outcome;
    /// 3. `SafeCommitter` stages, journals and publishes once (new copy:
    ///    exclusive create, no overwrite; overwrite: base revalidated inside
    ///    the protected interval); replaying a committed batch returns its
    ///    version;
    /// 4. a conflict, hash mismatch or expired receipt leaves the original
    ///    untouched and fails the run with the reason, so the user re-runs
    ///    the skill against the current file.
    pub fn decide_and_commit(
        &self,
        run_id: &str,
        target: CommitTarget,
    ) -> Result<(RunReport, CommitReport), ExecError> {
        let mut snap = self
            .host
            .store
            .load(run_id)?
            .ok_or_else(|| ExecError::NoSnapshot(run_id.into()))?;
        let (state, _, _) = self.host.log.run_state(run_id)?;
        if state != RunState::WaitingApproval {
            return Err(ExecError::WrongState {
                run_id: run_id.into(),
                state: state.as_str().into(),
                expected: RunState::WaitingApproval.as_str().into(),
            });
        }
        let pending = snap
            .pending_approval
            .clone()
            .ok_or_else(|| ExecError::Other("run is waiting but has no pending approval".into()))?;
        if pending.effect_class != "artifact.commit" {
            return Err(ExecError::Commit(format!(
                "pending effect is {}, not artifact.commit",
                pending.effect_class
            )));
        }
        // --- pre-flight (nothing durable yet) ------------------------------
        if let Some(t) = pending.requested_at {
            if now() - t > RECEIPT_VALIDITY {
                return Err(ExecError::Commit(format!(
                    "approval receipt expired ({} minutes); run the skill again",
                    RECEIPT_VALIDITY.num_minutes()
                )));
            }
        }
        let batch = crate::tools::builtin::batch_from_value(&pending.batch)
            .map_err(|e| ExecError::Commit(format!("bound batch: {e}")))?;
        let base_hash = pending
            .base_content_hash
            .clone()
            .unwrap_or_else(|| batch.base_content_hash.clone());
        let proposed_hash = pending
            .proposed_output_hash
            .clone()
            .ok_or_else(|| ExecError::Commit("approval binds no proposed_output_hash".into()))?;
        let base = self.host.artifacts.get(&batch.artifact_id).ok_or_else(|| {
            ExecError::Commit(format!(
                "base bytes for artifact {} were not supplied",
                batch.artifact_id
            ))
        })?;
        let base_now = harbor_canonical::sha256_hex(&base.bytes);
        if base_now != base_hash {
            return Err(ExecError::Commit(format!(
                "base file changed since the proposal (approved {base_hash}, found {base_now})"
            )));
        }
        let output = crate::tools::builtin::apply_batch(&base.bytes, &batch)
            .map_err(|e| ExecError::Commit(format!("re-applying the batch: {e}")))?;
        let output_hash = harbor_canonical::sha256_hex(&output);
        if output_hash != proposed_hash {
            return Err(ExecError::Commit(format!(
                "re-applied output {output_hash} does not match the approved {proposed_hash}"
            )));
        }
        let destination = target.destination().to_path_buf();
        if matches!(target, CommitTarget::SaveNewCopy { .. }) && destination.exists() {
            return Err(ExecError::Commit(format!(
                "destination {} already exists; Save New Copy never overwrites",
                destination.display()
            )));
        }
        let journal_db = self
            .host
            .commit_journal
            .clone()
            .ok_or_else(|| ExecError::Commit("host has no commit journal".into()))?;
        let committer = harbor_artifacts::SafeCommitter::new(&journal_db)
            .map_err(|e| ExecError::Commit(format!("commit journal: {e}")))?;

        // --- durable decision + dispatch ----------------------------------
        let mut cur = self.open_cursor(run_id)?;
        self.lease_acquired(&mut cur, run_id)?;
        let zero = BudgetDelta {
            active_compute_ms: 0,
            steps: 0,
            tool_calls: 0,
            context_tokens: 0,
        };
        self.emit(
            &mut cur,
            run_id,
            Actor::User,
            EventType::RunApprovalDecided,
            EventPayload::ApprovalDecided {
                effect_id: pending.effect_id.clone(),
                approved: true,
            },
            zero,
        )?;
        let attempt_id = harbor_security::HarborId::generate("attempt").to_string();
        self.emit(
            &mut cur,
            run_id,
            Actor::Executor,
            EventType::RunEffectDispatched,
            EventPayload::EffectDispatched {
                effect_id: pending.effect_id.clone(),
                attempt_id: attempt_id.clone(),
            },
            zero,
        )?;
        self.transition(
            &mut cur,
            run_id,
            RunState::WaitingApproval,
            RunState::Running,
            None,
        )?;
        let result = match &target {
            CommitTarget::SaveNewCopy { destination } => committer.commit_new_copy(
                &batch.batch_id,
                &batch.artifact_id,
                destination,
                &proposed_hash,
                &output,
            ),
            CommitTarget::Overwrite { destination } => committer.commit_external(
                &batch.batch_id,
                &batch.artifact_id,
                destination,
                &base_hash,
                &proposed_hash,
                &output,
            ),
        };
        let (outcome, version_id, bytes_written) = match &result {
            Ok(harbor_artifacts::CommitOutcome::Committed {
                version_id,
                bytes_written,
            }) => ("committed", version_id.clone(), *bytes_written),
            Ok(harbor_artifacts::CommitOutcome::CopiedNew { version_id, .. }) => {
                ("committed", version_id.clone(), output.len() as u64)
            }
            Err(harbor_artifacts::SafeCommitError::AlreadyCommitted(v)) => {
                ("already_committed", v.clone(), 0)
            }
            Err(harbor_artifacts::SafeCommitError::BaseChanged { .. })
            | Err(harbor_artifacts::SafeCommitError::Conflict) => ("conflict", String::new(), 0),
            Err(harbor_artifacts::SafeCommitError::OutcomeUnknown) => {
                ("outcome_unknown", String::new(), 0)
            }
            Err(_) => ("failed", String::new(), 0),
        };
        self.emit(
            &mut cur,
            run_id,
            Actor::Executor,
            EventType::RunEffectResolved,
            EventPayload::EffectResolved {
                effect_id: pending.effect_id.clone(),
                outcome: outcome.into(),
            },
            zero,
        )?;
        let record = json!({
            "approved": true,
            "effect_id": pending.effect_id,
            "receipt_id": pending.receipt_id,
            "canonical_args_hash": pending.canonical_args_hash,
            "proposed_output_hash": proposed_hash,
            "commit": {
                "outcome": outcome,
                "attempt_id": attempt_id,
                "mode": target.mode(),
                "destination": destination.to_string_lossy(),
                "version_id": version_id,
                "bytes_written": bytes_written,
            },
        });
        let _ = pointer::set_unchecked(
            &mut snap.state,
            &format!("/approvals/{}", pending.node_id),
            record,
        );
        if let Some(t) = snap
            .trail
            .iter_mut()
            .rev()
            .find(|t| t.node_id == pending.node_id)
        {
            t.decision = Some(format!("approved:{outcome}"));
        }
        snap.pending_approval = None;
        if let Err(e) = result {
            // The original is untouched (journal: conflict / staged); the
            // run records why and stops. No automatic retry (02 contract).
            self.note(
                &mut cur,
                run_id,
                format!(
                    "commit {outcome}: {e} (destination {})",
                    destination.display()
                ),
            )?;
            snap.position.next_node = None;
            let report = self.fail(&mut cur, &mut snap, format!("commit {outcome}: {e}"))?;
            self.release(&cur);
            return Err(ExecError::CommitFailed {
                outcome: outcome.into(),
                error: e.to_string(),
                report: Box::new(report),
            });
        }
        self.note(
            &mut cur,
            run_id,
            format!(
                "committed batch {} as {version_id} ({}) to {}",
                batch.batch_id,
                target.mode(),
                destination.display()
            ),
        )?;
        let commit = CommitReport {
            run_id: run_id.into(),
            node_id: pending.node_id.clone(),
            effect_id: pending.effect_id.clone(),
            receipt_id: pending.receipt_id.clone(),
            attempt_id,
            batch_id: batch.batch_id.clone(),
            artifact_id: batch.artifact_id.clone(),
            mode: target.mode().into(),
            destination,
            version_id,
            bytes_written,
            base_content_hash: base_hash,
            proposed_output_hash: proposed_hash,
            outcome: outcome.into(),
        };
        let next = match snap.graph.node(&pending.node_id) {
            Some(Node::Approval { next_approved, .. }) => next_approved
                .clone()
                .map(|e| self.follow(&mut snap.position, &pending.node_id, &e)),
            _ => None,
        };
        snap.position.next_node = next;
        self.save(&mut snap)?;
        let report = self.run_loop(&mut cur, &mut snap)?;
        Ok((report, commit))
    }

    /// Resume a `RUNNING` run whose process died between nodes.
    pub fn resume(&self, run_id: &str) -> Result<RunReport, ExecError> {
        let mut snap = self
            .host
            .store
            .load(run_id)?
            .ok_or_else(|| ExecError::NoSnapshot(run_id.into()))?;
        let (state, _, _) = self.host.log.run_state(run_id)?;
        if state != RunState::Running {
            return Err(ExecError::WrongState {
                run_id: run_id.into(),
                state: state.as_str().into(),
                expected: RunState::Running.as_str().into(),
            });
        }
        let mut cur = self.open_cursor(run_id)?;
        self.lease_acquired(&mut cur, run_id)?;
        self.note(&mut cur, run_id, "resumed from snapshot".into())?;
        self.run_loop(&mut cur, &mut snap)
    }

    fn follow(&self, pos: &mut Position, from: &str, edge: &Edge) -> String {
        match edge {
            Edge::To(t) => t.clone(),
            Edge::Bounded {
                to,
                max_iterations,
                exhausted,
            } => {
                let key = format!("{from}->{to}");
                let n = pos.edge_counts.entry(key).or_insert(0);
                if *n >= *max_iterations {
                    exhausted.clone()
                } else {
                    *n += 1;
                    to.clone()
                }
            }
        }
    }

    // --- the loop ----------------------------------------------------------

    fn run_loop(
        &self,
        cur: &mut RunCursor,
        snap: &mut RunSnapshot,
    ) -> Result<RunReport, ExecError> {
        let r = self.run_loop_inner(cur, snap);
        self.release(cur);
        r
    }

    fn run_loop_inner(
        &self,
        cur: &mut RunCursor,
        snap: &mut RunSnapshot,
    ) -> Result<RunReport, ExecError> {
        let run_id = snap.run_id.clone();
        let budgets = budgets_of(&snap.graph);
        loop {
            // Cancellation is acknowledged by the executor itself.
            if self.host.cancel.load(Ordering::Relaxed) {
                return self.cancel_run(cur, snap);
            }
            let Some(node_id) = snap.position.next_node.clone() else {
                // No declared end node: the graph fell off its last edge.
                let outcome = OutcomeRecord {
                    outcome: Outcome::Completed,
                    outputs: json!({}),
                };
                self.transition(cur, &run_id, RunState::Running, RunState::Completed, None)?;
                snap.outcome = Some(outcome.clone());
                self.save(snap)?;
                return Ok(self.report(
                    snap,
                    cur,
                    RunStatus::Completed {
                        outcome: outcome.outcome,
                        outputs: outcome.outputs,
                    },
                    RunState::Completed,
                ));
            };
            if let Some(step) = self.host.step {
                step(&node_id);
            }
            let node = snap
                .graph
                .node(&node_id)
                .cloned()
                .ok_or_else(|| ExecError::Other(format!("node {node_id} vanished")))?;
            let is_tool = matches!(node, Node::ToolCall { .. });
            let admission = BudgetDelta {
                active_compute_ms: 0,
                steps: 1,
                tool_calls: if is_tool { 1 } else { 0 },
                context_tokens: 0,
            };
            if let Err(e) = admission.within(&cur.counters, &budgets) {
                return self.fail(cur, snap, format!("{e} before node {node_id}"));
            }
            match self.execute_node(cur, snap, &node, &budgets) {
                Ok(Flow::Continue(next)) => {
                    snap.position.next_node = next;
                    self.save(snap)?;
                }
                Ok(Flow::Stop(RunStatus::Cancelled)) => return self.cancel_run(cur, snap),
                Ok(Flow::Stop(RunStatus::Failed { error })) => return self.fail(cur, snap, error),
                Ok(Flow::Stop(status)) => {
                    let state = match &status {
                        RunStatus::WaitingApproval { .. } => RunState::WaitingApproval,
                        _ => RunState::Completed,
                    };
                    self.save(snap)?;
                    return Ok(self.report(snap, cur, status, state));
                }
                Err(e) => return self.fail(cur, snap, e.to_string()),
            }
        }
    }

    /// Cancellation observed between nodes: the executor is the one
    /// acknowledging, so CANCELLING is passed through immediately (an
    /// unacknowledged cancel would instead pause the run — 03 §4).
    fn cancel_run(
        &self,
        cur: &mut RunCursor,
        snap: &mut RunSnapshot,
    ) -> Result<RunReport, ExecError> {
        let run_id = snap.run_id.clone();
        let zero = BudgetDelta {
            active_compute_ms: 0,
            steps: 0,
            tool_calls: 0,
            context_tokens: 0,
        };
        self.emit(
            cur,
            &run_id,
            Actor::User,
            EventType::RunCancelRequested,
            EventPayload::CancelRequested {
                requested_by: "user".into(),
            },
            zero,
        )?;
        self.transition(cur, &run_id, RunState::Running, RunState::Cancelling, None)?;
        self.transition(
            cur,
            &run_id,
            RunState::Cancelling,
            RunState::Cancelled,
            None,
        )?;
        snap.position.next_node = None;
        self.save(snap)?;
        Ok(self.report(snap, cur, RunStatus::Cancelled, RunState::Cancelled))
    }

    fn fail(
        &self,
        cur: &mut RunCursor,
        snap: &mut RunSnapshot,
        error: String,
    ) -> Result<RunReport, ExecError> {
        let run_id = snap.run_id.clone();
        self.note(cur, &run_id, format!("failed: {error}"))?;
        self.transition(cur, &run_id, RunState::Running, RunState::Failed, None)?;
        snap.last_error = Some(error.clone());
        snap.position.next_node = None;
        self.save(snap)?;
        Ok(self.report(snap, cur, RunStatus::Failed { error }, RunState::Failed))
    }

    fn step_started(
        &self,
        cur: &mut RunCursor,
        snap: &RunSnapshot,
        step_id: &str,
        node: &Node,
        input_hash: &str,
    ) -> Result<(), ExecError> {
        self.emit(
            cur,
            &snap.run_id,
            Actor::Executor,
            EventType::RunStepStarted,
            EventPayload::StepStarted {
                step_id: step_id.into(),
                description: node.kind().into(),
                node_id: Some(node.id().into()),
                input_hash: Some(input_hash.into()),
            },
            BudgetDelta {
                active_compute_ms: 0,
                steps: 1,
                tool_calls: 0,
                context_tokens: 0,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn step_completed(
        &self,
        cur: &mut RunCursor,
        snap: &RunSnapshot,
        step_id: &str,
        node: &Node,
        summary: String,
        output_hash: Option<String>,
        tool: Option<String>,
        elapsed_ms: u64,
        context_tokens: u64,
    ) -> Result<(), ExecError> {
        self.emit(
            cur,
            &snap.run_id,
            Actor::Executor,
            EventType::RunStepCompleted,
            EventPayload::StepCompleted {
                step_id: step_id.into(),
                summary,
                node_id: Some(node.id().into()),
                output_hash,
                tool: tool.clone(),
            },
            BudgetDelta {
                active_compute_ms: elapsed_ms,
                steps: 0,
                tool_calls: if tool.is_some() { 1 } else { 0 },
                context_tokens,
            },
        )
    }

    fn execute_node(
        &self,
        cur: &mut RunCursor,
        snap: &mut RunSnapshot,
        node: &Node,
        budgets: &Budgets,
    ) -> Result<Flow, ExecError> {
        let iteration = {
            let n = snap
                .position
                .node_runs
                .entry(node.id().to_string())
                .or_insert(0);
            *n += 1;
            *n
        };
        let step_id = format!("{}:{iteration}", node.id());
        let started = Instant::now();
        match node {
            Node::ToolCall {
                id,
                tool,
                args,
                out,
                next,
                ..
            } => {
                let resolved = graph::resolve_refs(args, &snap.state);
                let input_hash = pointer::stable_hash(&resolved);
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                let output = self.call_tool(snap, id, tool, &resolved, iteration)?;
                pointer::set(&mut snap.state, out, output.clone())
                    .map_err(|e| ExecError::Other(e.to_string()))?;
                let output_hash = pointer::stable_hash(&output);
                let elapsed = started.elapsed().as_millis() as u64;
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    format!("{tool} ok"),
                    Some(output_hash.clone()),
                    Some(tool.clone()),
                    elapsed,
                    0,
                )?;
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash,
                    output_hash: Some(output_hash),
                    elapsed_ms: elapsed,
                    tool: Some(tool.clone()),
                    prompt_tokens: None,
                    completion_tokens: None,
                    structured_mode: None,
                    executed_on: None,
                    decision: None,
                });
                Ok(Flow::Continue(
                    next.as_ref()
                        .map(|e| self.follow(&mut snap.position, id, e)),
                ))
            }
            Node::ModelStructured {
                id,
                instructions,
                context,
                output_schema,
                out,
                max_tokens,
                max_retries,
                next,
                ..
            } => {
                let ctx_value = gather_context(context, &snap.state);
                let input_hash = pointer::stable_hash(
                    &json!({"instructions": instructions, "context": ctx_value}),
                );
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                let r = self.call_model(
                    snap,
                    id,
                    instructions,
                    context,
                    Some(output_schema),
                    max_tokens.unwrap_or(1024),
                    max_retries.unwrap_or(1),
                    iteration,
                    budgets,
                    cur,
                )?;
                pointer::set(&mut snap.state, out, r.value.clone())
                    .map_err(|e| ExecError::Other(e.to_string()))?;
                let output_hash = pointer::stable_hash(&r.value);
                let elapsed = started.elapsed().as_millis() as u64;
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    format!("structured output on {}", r.executed_on),
                    Some(output_hash.clone()),
                    None,
                    elapsed,
                    r.prompt_tokens + r.completion_tokens,
                )?;
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash,
                    output_hash: Some(output_hash),
                    elapsed_ms: elapsed,
                    tool: None,
                    prompt_tokens: Some(r.prompt_tokens),
                    completion_tokens: Some(r.completion_tokens),
                    structured_mode: Some(r.mode),
                    executed_on: Some(r.executed_on),
                    decision: None,
                });
                Ok(Flow::Continue(
                    next.as_ref()
                        .map(|e| self.follow(&mut snap.position, id, e)),
                ))
            }
            Node::ModelText {
                id,
                instructions,
                context,
                out,
                max_tokens,
                next,
                ..
            } => {
                let ctx_value = gather_context(context, &snap.state);
                let input_hash = pointer::stable_hash(
                    &json!({"instructions": instructions, "context": ctx_value}),
                );
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                let r = self.call_model(
                    snap,
                    id,
                    instructions,
                    context,
                    None,
                    max_tokens.unwrap_or(1024),
                    0,
                    iteration,
                    budgets,
                    cur,
                )?;
                pointer::set(&mut snap.state, out, r.value.clone())
                    .map_err(|e| ExecError::Other(e.to_string()))?;
                let output_hash = pointer::stable_hash(&r.value);
                let elapsed = started.elapsed().as_millis() as u64;
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    format!("text on {}", r.executed_on),
                    Some(output_hash.clone()),
                    None,
                    elapsed,
                    r.prompt_tokens + r.completion_tokens,
                )?;
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash,
                    output_hash: Some(output_hash),
                    elapsed_ms: elapsed,
                    tool: None,
                    prompt_tokens: Some(r.prompt_tokens),
                    completion_tokens: Some(r.completion_tokens),
                    structured_mode: None,
                    executed_on: Some(r.executed_on),
                    decision: None,
                });
                Ok(Flow::Continue(
                    next.as_ref()
                        .map(|e| self.follow(&mut snap.position, id, e)),
                ))
            }
            Node::Branch {
                id, cases, default, ..
            } => {
                let probe: Vec<Value> = cases
                    .iter()
                    .map(|c| {
                        pointer::get(&snap.state, &c.when.from)
                            .cloned()
                            .unwrap_or(Value::Null)
                    })
                    .collect();
                let input_hash = pointer::stable_hash(&Value::Array(probe));
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                let chosen = cases
                    .iter()
                    .enumerate()
                    .find(|(_, c)| graph::eval_predicate(&c.when, &snap.state))
                    .map(|(i, c)| (format!("case {i}"), c.next.clone()))
                    .unwrap_or_else(|| ("default".into(), default.clone()));
                let elapsed = started.elapsed().as_millis() as u64;
                let decision_hash = pointer::stable_hash(&json!(chosen.1.target()));
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    format!("branch → {} ({})", chosen.1.target(), chosen.0),
                    Some(decision_hash.clone()),
                    None,
                    elapsed,
                    0,
                )?;
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash,
                    output_hash: Some(decision_hash),
                    elapsed_ms: elapsed,
                    tool: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    structured_mode: None,
                    executed_on: None,
                    decision: Some(chosen.1.target().to_string()),
                });
                let next = self.follow(&mut snap.position, id, &chosen.1);
                Ok(Flow::Continue(Some(next)))
            }
            Node::Map {
                id,
                over,
                item,
                body,
                collect,
                max_items,
                next,
                ..
            } => {
                let items: Vec<Value> = pointer::get(&snap.state, over)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let input_hash = pointer::stable_hash(&Value::Array(items.clone()));
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                let body_node = snap
                    .graph
                    .node(body)
                    .cloned()
                    .ok_or_else(|| ExecError::Other(format!("map body {body} missing")))?;
                let mut results = Vec::new();
                for (i, it) in items.iter().take(*max_items as usize).enumerate() {
                    if self.host.cancel.load(Ordering::Relaxed) {
                        return Ok(Flow::Stop(RunStatus::Cancelled));
                    }
                    let admission = BudgetDelta {
                        active_compute_ms: 0,
                        steps: 1,
                        tool_calls: if matches!(body_node, Node::ToolCall { .. }) {
                            1
                        } else {
                            0
                        },
                        context_tokens: 0,
                    };
                    admission
                        .within(&cur.counters, budgets)
                        .map_err(|e| ExecError::Other(format!("{e} in map {id} item {i}")))?;
                    pointer::set(&mut snap.state, item, it.clone())
                        .map_err(|e| ExecError::Other(e.to_string()))?;
                    match self.execute_node(cur, snap, &body_node, budgets)? {
                        Flow::Continue(_) => {}
                        Flow::Stop(s) => return Ok(Flow::Stop(s)),
                    }
                    let produced = body_node
                        .out_pointer()
                        .and_then(|p| pointer::get(&snap.state, p).cloned())
                        .unwrap_or(Value::Null);
                    results.push(produced);
                }
                let truncated = items.len() > *max_items as usize;
                pointer::set(&mut snap.state, collect, Value::Array(results.clone()))
                    .map_err(|e| ExecError::Other(e.to_string()))?;
                let output_hash = pointer::stable_hash(&Value::Array(results));
                let elapsed = started.elapsed().as_millis() as u64;
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    format!(
                        "map over {} items{}",
                        items.len().min(*max_items as usize),
                        if truncated { " (truncated)" } else { "" }
                    ),
                    Some(output_hash.clone()),
                    None,
                    elapsed,
                    0,
                )?;
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash,
                    output_hash: Some(output_hash),
                    elapsed_ms: elapsed,
                    tool: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    structured_mode: None,
                    executed_on: None,
                    decision: None,
                });
                Ok(Flow::Continue(
                    next.as_ref()
                        .map(|e| self.follow(&mut snap.position, id, e)),
                ))
            }
            Node::Const {
                id,
                value,
                out,
                next,
                ..
            } => {
                let input_hash = pointer::stable_hash(value);
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                pointer::set(&mut snap.state, out, value.clone())
                    .map_err(|e| ExecError::Other(e.to_string()))?;
                let elapsed = started.elapsed().as_millis() as u64;
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    "const".into(),
                    Some(input_hash.clone()),
                    None,
                    elapsed,
                    0,
                )?;
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash: input_hash.clone(),
                    output_hash: Some(input_hash),
                    elapsed_ms: elapsed,
                    tool: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    structured_mode: None,
                    executed_on: None,
                    decision: None,
                });
                Ok(Flow::Continue(
                    next.as_ref()
                        .map(|e| self.follow(&mut snap.position, id, e)),
                ))
            }
            Node::Approval {
                id,
                effect_class,
                batch,
                ..
            } => {
                let batch_value = pointer::get(&snap.state, batch)
                    .cloned()
                    .unwrap_or(Value::Null);
                let input_hash = pointer::stable_hash(&batch_value);
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                if batch_value.is_null() {
                    return Err(ExecError::Other(format!(
                        "approval node {id}: nothing at {batch} to approve"
                    )));
                }
                // The effect is durable before anything is dispatched
                // (03 §5): prepared + approval requested, then park.
                let canonical_args_hash = harbor_canonical::canonical_sha256(&batch_value)
                    .unwrap_or_else(|_| pointer::stable_hash(&batch_value));
                let effect_id = harbor_security::HarborId::generate("effect").to_string();
                let receipt_id = harbor_security::HarborId::generate("rcpt").to_string();
                let zero = BudgetDelta {
                    active_compute_ms: 0,
                    steps: 0,
                    tool_calls: 0,
                    context_tokens: 0,
                };
                self.emit(
                    cur,
                    &snap.run_id,
                    Actor::Executor,
                    EventType::RunEffectPrepared,
                    EventPayload::EffectPrepared {
                        effect_id: effect_id.clone(),
                        canonical_args_hash: canonical_args_hash.clone(),
                    },
                    zero,
                )?;
                self.emit(
                    cur,
                    &snap.run_id,
                    Actor::Executor,
                    EventType::RunApprovalRequested,
                    EventPayload::ApprovalRequested {
                        effect_id: effect_id.clone(),
                        receipt_id: receipt_id.clone(),
                    },
                    zero,
                )?;
                let elapsed = started.elapsed().as_millis() as u64;
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    format!("approval requested ({})", effect_class.as_str()),
                    Some(canonical_args_hash.clone()),
                    None,
                    elapsed,
                    0,
                )?;
                self.transition(
                    cur,
                    &snap.run_id,
                    RunState::Running,
                    RunState::WaitingApproval,
                    None,
                )?;
                let pending = PendingApproval {
                    node_id: id.clone(),
                    effect_id,
                    receipt_id,
                    effect_class: effect_class.as_str().into(),
                    canonical_args_hash: canonical_args_hash.clone(),
                    base_content_hash: batch_value
                        .get("base_content_hash")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    // Propose tools return {batch, proposed_output_hash, …};
                    // the approval binds the sibling hash next to the batch.
                    proposed_output_hash: batch
                        .rsplit_once('/')
                        .and_then(|(parent, _)| pointer::get(&snap.state, parent))
                        .and_then(|parent| parent.get("proposed_output_hash"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    batch_id: batch_value
                        .get("batch_id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    artifact_id: batch_value
                        .get("artifact_id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    diff: self.proposal_diff(&batch_value),
                    requested_at: Some(now()),
                    batch: batch_value,
                };
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash,
                    output_hash: Some(canonical_args_hash),
                    elapsed_ms: elapsed,
                    tool: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    structured_mode: None,
                    executed_on: None,
                    decision: Some("pending".into()),
                });
                snap.pending_approval = Some(pending.clone());
                snap.position.next_node = None;
                Ok(Flow::Stop(RunStatus::WaitingApproval {
                    approval: Box::new(pending),
                }))
            }
            Node::End {
                id,
                outcome,
                outputs,
                ..
            } => {
                let input_hash = pointer::stable_hash(&json!(outputs));
                self.step_started(cur, snap, &step_id, node, &input_hash)?;
                let mut out = serde_json::Map::new();
                for p in outputs {
                    let key = p.trim_start_matches('/').replace('/', ".");
                    out.insert(
                        key,
                        pointer::get(&snap.state, p).cloned().unwrap_or(Value::Null),
                    );
                }
                let outputs_value = Value::Object(out);
                let output_hash = pointer::stable_hash(&outputs_value);
                let elapsed = started.elapsed().as_millis() as u64;
                self.step_completed(
                    cur,
                    snap,
                    &step_id,
                    node,
                    format!("end: {}", outcome.as_str()),
                    Some(output_hash.clone()),
                    None,
                    elapsed,
                    0,
                )?;
                self.transition(
                    cur,
                    &snap.run_id,
                    RunState::Running,
                    RunState::Completed,
                    None,
                )?;
                let _ =
                    pointer::set_unchecked(&mut snap.state, "/outcome", json!(outcome.as_str()));
                snap.trail.push(NodeTrace {
                    step_id,
                    node_id: id.clone(),
                    kind: node.kind().into(),
                    iteration,
                    input_hash,
                    output_hash: Some(output_hash),
                    elapsed_ms: elapsed,
                    tool: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    structured_mode: None,
                    executed_on: None,
                    decision: Some(outcome.as_str().into()),
                });
                snap.outcome = Some(OutcomeRecord {
                    outcome: *outcome,
                    outputs: outputs_value.clone(),
                });
                snap.position.next_node = None;
                Ok(Flow::Stop(RunStatus::Completed {
                    outcome: *outcome,
                    outputs: outputs_value,
                }))
            }
        }
    }

    fn call_tool(
        &self,
        snap: &RunSnapshot,
        node_id: &str,
        tool: &str,
        args: &Value,
        iteration: u32,
    ) -> Result<Value, ExecError> {
        let host_inputs = snap.state.get("host").cloned().unwrap_or(json!({}));
        let model: Option<ModelRef> = snap.model.as_ref().map(ModelRef::from);
        let ctx = ToolContext {
            artifacts: self.host.artifacts,
            knowledge: self.host.knowledge,
            provider: self.host.provider,
            model,
            workspace_root: self.host.workspace_root.clone(),
            host_inputs: &host_inputs,
            cancel: self.host.cancel,
            trace_key: Some(format!("{}/{node_id}#{iteration}", snap.graph.id)),
            deadline: None,
        };
        let allowlist: BTreeSet<String> = snap.graph.tools();
        match self.host.registry.call(&ctx, tool, args, &allowlist) {
            Ok(r) => Ok(r.output),
            Err(ToolError::Cancelled(_)) => {
                Err(ExecError::Other("cancelled during tool call".into()))
            }
            Err(e) => Err(ExecError::Other(e.to_string())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call_model(
        &self,
        snap: &RunSnapshot,
        node_id: &str,
        instructions: &str,
        context: &[graph::ContextItem],
        schema: Option<&Value>,
        max_tokens: u32,
        max_retries: u32,
        iteration: u32,
        budgets: &Budgets,
        cur: &RunCursor,
    ) -> Result<ModelOutcome, ExecError> {
        let provider = self
            .host
            .provider
            .ok_or_else(|| ExecError::Other("no provider".into()))?;
        let model: ModelRef = snap
            .model
            .as_ref()
            .map(ModelRef::from)
            .ok_or_else(|| ExecError::Other("no model".into()))?;
        let structured = schema.is_some();
        let constrained = structured && provider.supports(&model, &Capabilities::StructuredOutput);
        let mut system = String::new();
        if let Some(s) = &snap.skill_instructions {
            system.push_str(s.trim());
            system.push_str("\n\n");
        }
        system.push_str(instructions.trim());
        if let Some(sc) = schema {
            system.push_str("\n\nRespond with a single JSON value that conforms to this JSON Schema and nothing else:\n");
            system.push_str(&serde_json::to_string(sc).unwrap_or_default());
        }
        let user = render_context(context, &snap.state);
        let mut messages = vec![
            JsonValue::object([
                ("role", JsonValue::str("system")),
                ("content", JsonValue::str(system)),
            ]),
            JsonValue::object([
                ("role", JsonValue::str("user")),
                ("content", JsonValue::str(user)),
            ]),
        ];
        // A schema that cannot be canonicalised cannot become a grammar.
        // This used to be `.ok()`, which turned that into `None`: the
        // request went out with no `response_schema`, llama.cpp sampled
        // unconstrained, and a result that happened to validate was still
        // recorded as `mode: "grammar_constrained"` — the label asserting
        // exactly the guarantee that had just been dropped. Canonical JSON
        // rejects floats, so any schema carrying `multipleOf: 0.5` or a
        // fractional bound silently lost its grammar. Fail loudly instead:
        // callers asked for constrained decoding and must not be told they
        // got it when they did not.
        let schema_canonical: Option<JsonValue> = if constrained {
            match schema {
                Some(sc) => Some(harbor_canonical::convert(sc.clone()).map_err(|e| {
                    ExecError::Other(format!(
                        "model node {node_id}: output_schema cannot be canonicalised \
                         ({e}), so decoding cannot be grammar-constrained"
                    ))
                })?),
                None => None,
            }
        } else {
            None
        };
        let mut requires = vec![Capabilities::Chat];
        if schema_canonical.is_some() {
            requires.push(Capabilities::StructuredOutput);
        }
        let mut prompt_tokens = 0u64;
        let mut completion_tokens = 0u64;
        let mut last_error = String::new();
        for attempt in 0..=max_retries {
            let req = ChatRequest {
                model: model.clone(),
                messages: messages.clone(),
                max_tokens,
                temperature: 0.0,
                requires: requires.clone(),
                response_schema: schema_canonical.clone(),
                trace_key: Some(if attempt == 0 {
                    format!("{}/{node_id}#{iteration}", snap.graph.id)
                } else {
                    format!("{}/{node_id}#{iteration}/retry{attempt}", snap.graph.id)
                }),
            };
            let resp = provider
                .generate(req)
                .map_err(|e| ExecError::Other(format!("model node {node_id}: {e}")))?;
            prompt_tokens += resp.usage.prompt_tokens;
            completion_tokens += resp.usage.completion_tokens;
            // Context budget is enforced on the durable totals.
            let delta = BudgetDelta {
                active_compute_ms: 0,
                steps: 0,
                tool_calls: 0,
                context_tokens: prompt_tokens + completion_tokens,
            };
            delta
                .within(&cur.counters, budgets)
                .map_err(|e| ExecError::Other(format!("{e} in model node {node_id}")))?;
            if let Some(sc) = schema {
                match extract_json(&resp.content) {
                    Some(v) => {
                        let violations = jsonschema::validate(sc, &v);
                        if violations.is_empty() {
                            return Ok(ModelOutcome {
                                value: v,
                                prompt_tokens,
                                completion_tokens,
                                executed_on: resp.executed_on,
                                mode: if constrained {
                                    "grammar_constrained".into()
                                } else {
                                    "validated_only".into()
                                },
                            });
                        }
                        last_error = violations
                            .iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<_>>()
                            .join("; ");
                    }
                    None => {
                        // Distinguish "the model rambled" from "we cut it
                        // off". The executor knows the budget it set and
                        // the tokens that came back; when they meet, the
                        // output is a prefix of a valid answer and saying
                        // "not JSON" sends the reader after the wrong
                        // thing entirely.
                        last_error = if resp.usage.completion_tokens >= max_tokens as u64 {
                            format!(
                                "output was truncated at the {max_tokens}-token budget \
                                 for this node, so it is an incomplete JSON value"
                            )
                        } else {
                            "output is not JSON".into()
                        }
                    }
                }
                for (role, content) in retry_turns(
                    resp.usage.completion_tokens >= max_tokens as u64,
                    max_tokens,
                    &last_error,
                    &resp.content,
                ) {
                    messages.push(JsonValue::object([
                        ("role", JsonValue::str(role)),
                        ("content", JsonValue::str(content)),
                    ]));
                }
            } else {
                return Ok(ModelOutcome {
                    value: Value::String(resp.content),
                    prompt_tokens,
                    completion_tokens,
                    executed_on: resp.executed_on,
                    mode: "text".into(),
                });
            }
        }
        Err(ExecError::Other(format!("model node {node_id}: output did not satisfy the schema after {} attempts: {last_error}", max_retries + 1)))
    }
}

struct ModelOutcome {
    value: Value,
    prompt_tokens: u64,
    completion_tokens: u64,
    executed_on: String,
    mode: String,
}

fn gather_context(items: &[graph::ContextItem], state: &Value) -> Value {
    Value::Array(
        items
            .iter()
            .map(|c| json!({"label": c.label, "value": pointer::get(state, &c.from).cloned().unwrap_or(Value::Null)}))
            .collect(),
    )
}

/// Render context items as labelled sections; strings verbatim, other
/// JSON pretty-printed; each section bounded by `max_chars`.
/// The turns appended before a structured retry.
///
/// A truncation and a schema violation need different handling, and
/// treating them alike is what made the second attempt worthless:
///
/// - The old code echoed the whole rejected output back as an assistant
///   turn. For a violation that is useful context. For a TRUNCATION it is
///   up to `max_tokens` of text spent re-reading the answer we just could
///   not fit, which leaves the retry less room than the attempt that had
///   already run out — so it truncates again, at the same place, for the
///   same reason.
/// - "That output did not satisfy the schema" is not actionable when the
///   schema was never the problem. The model cannot know it was cut off,
///   or that the fix is to say less.
///
/// On a truncation the retry therefore drops the fragment and asks for a
/// materially shorter answer. That is the only variable the model
/// controls; the budget is fixed by the node.
pub fn retry_turns(
    truncated: bool,
    max_tokens: u32,
    last_error: &str,
    rejected: &str,
) -> Vec<(&'static str, String)> {
    if truncated {
        return vec![(
            "user",
            format!(
                "Your previous answer was cut off at this node's \
                 {max_tokens}-token limit, so it was incomplete. Answer \
                 again and make it materially shorter: fewer array items, \
                 shorter strings, and only what the source states. Reply \
                 with JSON only."
            ),
        )];
    }
    vec![
        ("assistant", rejected.to_string()),
        (
            "user",
            format!(
                "That output did not satisfy the schema: {last_error}. \
                 Reply with corrected JSON only."
            ),
        ),
    ]
}

pub fn render_context(items: &[graph::ContextItem], state: &Value) -> String {
    let mut out = String::new();
    for c in items {
        let v = pointer::get(state, &c.from);
        let body = match v {
            None | Some(Value::Null) => "(none)".to_string(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => serde_json::to_string_pretty(other).unwrap_or_default(),
        };
        let max = c.max_chars.unwrap_or(20_000);
        let clipped: String = if body.chars().count() > max {
            let mut s: String = body.chars().take(max).collect();
            s.push_str("\n…[truncated]");
            s
        } else {
            body
        };
        out.push_str(&format!("## {}\n{}\n\n", c.label, clipped));
    }
    out
}

/// Convenience for hosts: the lease database path convention.
pub fn lease_db_path(data_root: &Path) -> PathBuf {
    data_root.join("db").join("agent.db")
}

/// Durable commit journal (same filesystem as the agent database).
pub fn commit_journal_path(data_root: &Path) -> PathBuf {
    data_root.join("db").join("commit_journal.db")
}

/// Snapshot store over the workspace's encrypted blob store (policy 13:
/// run state carries document content and must not sit in plaintext).
/// Blobs are content-addressed, so a plaintext index file maps run ids to
/// the current blob id; superseded blobs are deleted on save.
pub struct BlobStateStore {
    blobs: Arc<harbor_store::BlobStore>,
    workspace_id: String,
    index_path: PathBuf,
    lock: std::sync::Mutex<()>,
}

impl BlobStateStore {
    pub fn new(blobs: Arc<harbor_store::BlobStore>, workspace_id: &str, data_root: &Path) -> Self {
        BlobStateStore {
            blobs,
            workspace_id: workspace_id.to_string(),
            index_path: data_root
                .join("runs")
                .join(format!("{workspace_id}.index.json")),
            lock: std::sync::Mutex::new(()),
        }
    }

    fn read_index(&self) -> Result<BTreeMap<String, String>, ExecError> {
        if !self.index_path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = std::fs::read_to_string(&self.index_path)
            .map_err(|e| ExecError::Store(e.to_string()))?;
        serde_json::from_str(&text).map_err(|e| ExecError::Store(e.to_string()))
    }

    fn write_index(&self, index: &BTreeMap<String, String>) -> Result<(), ExecError> {
        if let Some(parent) = self.index_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ExecError::Store(e.to_string()))?;
        }
        let tmp = self.index_path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec(index).map_err(|e| ExecError::Store(e.to_string()))?,
        )
        .map_err(|e| ExecError::Store(e.to_string()))?;
        std::fs::rename(&tmp, &self.index_path).map_err(|e| ExecError::Store(e.to_string()))
    }

    fn opts(&self, _run_id: &str) -> harbor_store::blob::PutOptions {
        // Isolation comes from the per-workspace key; the store reads with
        // empty AAD, so callers must write with it too.
        harbor_store::blob::PutOptions::default()
    }
}

impl RunStateStore for BlobStateStore {
    fn save(&self, snapshot: &RunSnapshot) -> Result<(), ExecError> {
        let _g = self.lock.lock().unwrap();
        let bytes = serde_json::to_vec(snapshot).map_err(|e| ExecError::Store(e.to_string()))?;
        let r = self
            .blobs
            .put(&self.workspace_id, &bytes, &self.opts(&snapshot.run_id))
            .map_err(|e| ExecError::Store(e.to_string()))?;
        let mut index = self.read_index()?;
        let previous = index.insert(snapshot.run_id.clone(), r.id.clone());
        self.write_index(&index)?;
        if let Some(old) = previous {
            if old != r.id {
                let _ = self.blobs.delete_blob(&self.workspace_id, &old);
            }
        }
        Ok(())
    }

    fn load(&self, run_id: &str) -> Result<Option<RunSnapshot>, ExecError> {
        let _g = self.lock.lock().unwrap();
        let index = self.read_index()?;
        let Some(id) = index.get(run_id) else {
            return Ok(None);
        };
        let bytes = self
            .blobs
            .get(&self.workspace_id, id, &self.opts(run_id))
            .map_err(|e| ExecError::Store(e.to_string()))?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| ExecError::Store(e.to_string()))
    }
}
