//! Graph executor over the real durable substrate: SQLite event log,
//! lease manager, tool registry with Office fixtures, and the cassette
//! provider (no model weights involved).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde_json::{json, Value};

use harbor_agent::{EventLog, EventPayload, EventType, RunState};
use harbor_core::executor::{lease_db_path, Executor, FileStateStore, Host, RunRequest, RunStatus};
use harbor_core::graph::{Graph, Outcome};
use harbor_core::tools::{MemoryArtifacts, ToolRegistry};
use harbor_inference::provider::ModelRef;
use harbor_inference::{Cassette, RecordReplayProvider};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(repo_root().join("fixtures/office").join(name)).unwrap()
}

/// A DOCX with placeholders, derived from the structured.docx fixture by
/// applying a typed text.replace (the artifact crate has no writer).
fn placeholder_docx() -> Vec<u8> {
    let bytes = fixture("structured.docx");
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    doc.apply(
        &bytes,
        &[harbor_artifacts::DocxOp::TextReplace {
            index: 1,
            new_text: "Dear {{name}}, your reference is [[ref]].".into(),
        }],
    )
    .unwrap()
}

struct Rig {
    _dir: tempfile::TempDir,
    log: Arc<EventLog>,
    store: FileStateStore,
    registry: ToolRegistry,
    artifacts: MemoryArtifacts,
    cancel: AtomicBool,
    lease_db: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("db")).unwrap();
        let lease_db = lease_db_path(dir.path());
        let log = Arc::new(EventLog::open(&lease_db).unwrap());
        Rig {
            store: FileStateStore::new(dir.path().join("runs")),
            registry: ToolRegistry::builtin(),
            artifacts: MemoryArtifacts::new()
                .with("doc", "structured.docx", fixture("structured.docx"))
                .with("tpl", "letter.docx", placeholder_docx())
                .with("wb", "board_demo.xlsx", fixture("board_demo.xlsx")),
            cancel: AtomicBool::new(false),
            lease_db,
            log,
            _dir: dir,
        }
    }

    /// Same host, with a `step` sink so a test can see which nodes the
    /// run reached and in what order.
    fn executor_reporting<'a>(
        &'a self,
        provider: Option<&'a RecordReplayProvider>,
        step: &'a dyn Fn(&str),
    ) -> Executor<'a> {
        Executor::new(Host {
            log: self.log.clone(),
            lease_db: self.lease_db.clone(),
            store: &self.store,
            registry: &self.registry,
            provider: provider.map(|p| p as &dyn harbor_inference::ModelProvider),
            artifacts: &self.artifacts,
            knowledge: None,
            workspace_root: None,
            cancel: &self.cancel,
            executor_id: "test-executor".into(),
            commit_journal: None,
            step: Some(step),
        })
    }

    fn executor<'a>(&'a self, provider: Option<&'a RecordReplayProvider>) -> Executor<'a> {
        Executor::new(Host {
            log: self.log.clone(),
            lease_db: self.lease_db.clone(),
            store: &self.store,
            registry: &self.registry,
            provider: provider.map(|p| p as &dyn harbor_inference::ModelProvider),
            artifacts: &self.artifacts,
            knowledge: None,
            workspace_root: None,
            cancel: &self.cancel,
            executor_id: "test-executor".into(),
            commit_journal: None,
            step: None,
        })
    }

    fn request(&self, graph: Graph, inputs: Value) -> RunRequest {
        RunRequest {
            run_id: None,
            workspace_id: "ws-test".into(),
            graph,
            skill_id: Some("test-skill".into()),
            skill_instructions: Some("You are Harbor. Never invent facts.".into()),
            inputs,
            host_inputs: json!({}),
            model: Some(ModelRef::InstalledPackage {
                package_id: "cassette".into(),
            }),
        }
    }
}

fn graph(id: &str, entry: &str, budgets: (u32, u32), nodes: Value) -> Graph {
    Graph::from_value(&json!({
        "schema": "harbor.graph/v1",
        "id": id,
        "version": 1,
        "inputs": {"type": "object"},
        "entry": entry,
        "budgets": {"max_steps": budgets.0, "max_tool_calls": budgets.1},
        "nodes": nodes
    }))
    .unwrap()
}

fn step_events(log: &EventLog, run_id: &str) -> Vec<(String, Option<String>, Option<String>)> {
    log.load_stream(run_id)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            EventPayload::StepStarted {
                step_id,
                node_id,
                input_hash,
                ..
            } => Some((step_id, node_id, input_hash)),
            _ => None,
        })
        .collect()
}

/// A long-running host reports each node so a stalled run says WHERE it
/// stopped. A skill run hung for 120 s in CI and the only evidence was
/// the flat phase "running"; the sink below is what turns that into a
/// node name.
#[test]
fn the_step_sink_reports_every_node_in_order() {
    let rig = Rig::new();
    let g = graph(
        "steps",
        "read",
        (10, 5),
        json!([
            {"id": "read", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": {"$state": "/input/artifact_id"}}, "out": "/doc", "next": "pick"},
            {"id": "pick", "kind": "branch", "cases": [{"when": {"from": "/doc/paragraphs", "op": "gt", "value": 0}, "next": "done"}], "default": "done"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/doc"]}
        ]),
    );
    let seen = std::sync::Mutex::new(Vec::<String>::new());
    let sink = |node_id: &str| seen.lock().unwrap().push(node_id.to_string());
    let exec = rig.executor_reporting(None, &sink);
    let report = exec
        .start(rig.request(g, json!({"artifact_id": "doc"})))
        .unwrap();

    assert_eq!(report.state, "COMPLETED");
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["read".to_string(), "pick".to_string(), "done".to_string()],
        "every node must be reported, in the order the run reached them"
    );
}

#[test]
fn tool_model_branch_end_completes_with_replayable_chain() {
    let rig = Rig::new();
    let g = graph(
        "summ",
        "read",
        (10, 5),
        json!([
            {"id": "read", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": {"$state": "/input/artifact_id"}}, "out": "/doc", "next": "sum"},
            {"id": "sum", "kind": "model.structured", "instructions": "Summarize the paragraphs.",
             "context": [{"label": "Paragraphs", "from": "/doc/paragraphs"}],
             "output_schema": {"type": "object", "properties": {"summary": {"type": "string", "minLength": 1}, "paragraphs": {"type": "integer"}}, "required": ["summary", "paragraphs"], "additionalProperties": false},
             "out": "/summary", "next": "check"},
            {"id": "check", "kind": "branch", "cases": [{"when": {"from": "/summary/paragraphs", "op": "gt", "value": 3}, "next": "done"}], "default": "short"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/summary"]},
            {"id": "short", "kind": "end", "outcome": "abstained"}
        ]),
    );
    let cassette = Cassette::default().with_entry(
        "summ/sum#1",
        r#"{"summary": "A plan with items.", "paragraphs": 5}"#,
    );
    let provider = RecordReplayProvider::replay(cassette);
    let exec = rig.executor(Some(&provider));
    let report = exec
        .start(rig.request(g, json!({"artifact_id": "doc"})))
        .unwrap();

    assert_eq!(report.state, "COMPLETED");
    match &report.status {
        RunStatus::Completed { outcome, outputs } => {
            assert_eq!(*outcome, Outcome::Completed);
            assert_eq!(outputs["summary"]["paragraphs"], 5);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(report.steps, 4);
    assert_eq!(report.tool_calls, 1);
    assert_eq!(provider.hits(), 1);
    // Trail: every node ran once with io hashes; the model node records
    // its mode and executor identity.
    let kinds: Vec<&str> = report.trail.iter().map(|t| t.kind.as_str()).collect();
    assert_eq!(kinds, ["tool.call", "model.structured", "branch", "end"]);
    assert!(report.trail.iter().all(|t| t.input_hash.len() == 64
        && t.output_hash
            .as_ref()
            .map(|h| h.len() == 64)
            .unwrap_or(false)));
    assert_eq!(
        report.trail[1].structured_mode.as_deref(),
        Some("grammar_constrained")
    );
    assert_eq!(report.trail[1].executed_on.as_deref(), Some("cassette"));
    assert_eq!(report.trail[2].decision.as_deref(), Some("done"));
    // Events carry node identity; the chain replays and verifies.
    let steps = step_events(&rig.log, &report.run_id);
    assert_eq!(
        steps
            .iter()
            .map(|s| s.1.clone().unwrap())
            .collect::<Vec<_>>(),
        ["read", "sum", "check", "done"]
    );
    assert!(steps
        .iter()
        .all(|s| s.2.as_ref().map(|h| h.len() == 64).unwrap_or(false)));
    let replay = rig.log.replay(&report.run_id).unwrap();
    assert_eq!(replay.final_state, Some(RunState::Completed));
    assert_eq!(replay.verified_events as u64, report.events);
    assert_eq!(replay.counters.step_count_total, 4);
    // The blackboard is persisted and its hash matches the report.
    let snap = harbor_core::executor::RunStateStore::load(&rig.store, &report.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(snap.state_hash, report.state_hash);
    assert_eq!(snap.state["outcome"], "completed");
}

#[test]
fn approval_node_parks_the_run_and_decide_continues_it() {
    let rig = Rig::new();
    let g = graph(
        "fill",
        "inv",
        (12, 6),
        json!([
            {"id": "inv", "kind": "tool.call", "tool": "artifact.placeholders", "args": {"artifact_id": "tpl"}, "out": "/inventory", "next": "fill"},
            {"id": "fill", "kind": "tool.call", "tool": "artifact.fill_placeholders", "args": {"artifact_id": "tpl", "values": {"$state": "/input/values"}}, "out": "/fill", "next": "gate"},
            {"id": "gate", "kind": "branch", "cases": [{"when": {"from": "/fill/batch", "op": "exists"}, "next": "approve"}], "default": "needs"},
            {"id": "approve", "kind": "approval", "effect_class": "artifact.commit", "batch": "/fill/batch", "next_approved": "done", "next_rejected": "rejected"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/fill/preview", "/approvals/approve"]},
            {"id": "rejected", "kind": "end", "outcome": "abstained"},
            {"id": "needs", "kind": "end", "outcome": "needs_input", "outputs": ["/fill/unmapped"]}
        ]),
    );
    let exec = rig.executor(None);
    let report = exec
        .start(rig.request(
            g.clone(),
            json!({"values": {"name": "Amina", "ref": "HB-42"}}),
        ))
        .unwrap();
    assert_eq!(report.state, "WAITING_APPROVAL");
    let pending = match &report.status {
        RunStatus::WaitingApproval { approval } => (**approval).clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(pending.effect_class, "artifact.commit");
    assert_eq!(pending.canonical_args_hash.len(), 64);
    assert!(pending.proposed_output_hash.as_ref().unwrap().len() == 64);
    assert_eq!(pending.batch["operations"].as_array().unwrap().len(), 1);
    assert_eq!(pending.artifact_id.as_deref(), Some("tpl"));
    // Effect prepared + approval requested are durable authority events.
    let types: Vec<String> = rig
        .log
        .load_stream(&report.run_id)
        .unwrap()
        .iter()
        .map(|e| e.event_type.as_str().to_string())
        .collect();
    assert!(types.contains(&"run.effect_prepared".to_string()));
    assert!(types.contains(&"run.approval_requested".to_string()));
    let (state, _, _) = rig.log.run_state(&report.run_id).unwrap();
    assert_eq!(state, RunState::WaitingApproval);
    // Deciding a run that is not waiting is a typed error.
    let exec2 = rig.executor(None);
    assert!(exec2.decide("no-such-run", true).is_err());
    // Approve → continues to the approved edge and completes.
    let after = exec2.decide(&report.run_id, true).unwrap();
    assert_eq!(after.state, "COMPLETED");
    match &after.status {
        RunStatus::Completed { outputs, .. } => {
            assert_eq!(outputs["approvals.approve"]["approved"], true);
            assert_eq!(
                outputs["fill.preview"][0]["after"],
                "Dear Amina, your reference is HB-42."
            );
        }
        other => panic!("{other:?}"),
    }
    let types: Vec<String> = rig
        .log
        .load_stream(&report.run_id)
        .unwrap()
        .iter()
        .map(|e| e.event_type.as_str().to_string())
        .collect();
    assert!(types.contains(&"run.approval_decided".to_string()));
    assert_eq!(
        rig.log.replay(&report.run_id).unwrap().final_state,
        Some(RunState::Completed)
    );
    // A second run rejected at the gate takes the rejected edge.
    let exec3 = rig.executor(None);
    let r2 = exec3
        .start(rig.request(g.clone(), json!({"values": {"name": "B", "ref": "1"}})))
        .unwrap();
    let after2 = rig.executor(None).decide(&r2.run_id, false).unwrap();
    assert!(matches!(
        after2.status,
        RunStatus::Completed {
            outcome: Outcome::Abstained,
            ..
        }
    ));
    // Missing values never get invented: needs_input outcome lists them.
    let r3 = rig
        .executor(None)
        .start(rig.request(g, json!({"values": {"name": "C"}})))
        .unwrap();
    match r3.status {
        RunStatus::Completed { outcome, outputs } => {
            assert_eq!(outcome, Outcome::NeedsInput);
            assert_eq!(outputs["fill.unmapped"], json!(["ref"]));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn budgets_bounded_cycles_and_map_are_enforced() {
    let rig = Rig::new();
    // Steps budget: a 3-node chain under max_steps=2 fails before node 3.
    let tight = graph(
        "tight",
        "a",
        (2, 5),
        json!([
            {"id": "a", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": "doc"}, "out": "/a", "next": "b"},
            {"id": "b", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": "doc"}, "out": "/b", "next": "c"},
            {"id": "c", "kind": "end", "outcome": "completed"}
        ]),
    );
    let r = rig
        .executor(None)
        .start(rig.request(tight, json!({})))
        .unwrap();
    assert_eq!(r.state, "FAILED");
    assert!(
        matches!(&r.status, RunStatus::Failed { error } if error.contains("budget exceeded: steps")),
        "{:?}",
        r.status
    );
    assert_eq!(
        rig.log.replay(&r.run_id).unwrap().final_state,
        Some(RunState::Failed)
    );

    // Bounded cycle: the loop node runs 1 + max_iterations times, then
    // the exhausted continuation is taken.
    let cyc = graph(
        "cyc",
        "loop",
        (20, 5),
        json!([
            {"id": "loop", "kind": "model.text", "instructions": "Refine.", "context": [{"label": "Draft", "from": "/draft"}], "out": "/draft",
             "next": {"to": "loop", "max_iterations": 2, "exhausted": "done"}},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/draft"]}
        ]),
    );
    let cassette = Cassette::default()
        .with_entry("cyc/loop#1", "draft 1")
        .with_entry("cyc/loop#2", "draft 2")
        .with_entry("cyc/loop#3", "draft 3");
    let provider = RecordReplayProvider::replay(cassette);
    let r = rig
        .executor(Some(&provider))
        .start(rig.request(cyc, json!({})))
        .unwrap();
    assert_eq!(r.state, "COMPLETED");
    assert_eq!(provider.hits(), 3);
    assert_eq!(r.trail.iter().filter(|t| t.node_id == "loop").count(), 3);
    match r.status {
        RunStatus::Completed { outputs, .. } => assert_eq!(outputs["draft"], "draft 3"),
        other => panic!("{other:?}"),
    }

    // Map: body runs per item (each a step) and results are collected.
    let map = graph(
        "mapg",
        "m",
        (20, 5),
        json!([
            {"id": "m", "kind": "map", "over": "/input/items", "item": "/cur", "body": "body", "collect": "/results", "max_items": 2, "next": "done"},
            {"id": "body", "kind": "model.text", "instructions": "Echo.", "context": [{"label": "Item", "from": "/cur"}], "out": "/one"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/results"]}
        ]),
    );
    let cassette = Cassette::default()
        .with_entry("mapg/body#1", "echo a")
        .with_entry("mapg/body#2", "echo b");
    let provider = RecordReplayProvider::replay(cassette);
    let r = rig
        .executor(Some(&provider))
        .start(rig.request(map, json!({"items": ["a", "b", "c"]})))
        .unwrap();
    assert_eq!(r.state, "COMPLETED");
    match r.status {
        RunStatus::Completed { outputs, .. } => {
            assert_eq!(outputs["results"], json!(["echo a", "echo b"]))
        }
        other => panic!("{other:?}"),
    }
    // map + 2 bodies + end = 4 steps.
    assert_eq!(r.steps, 4);
}

#[test]
fn structured_output_is_validated_below_the_model_with_bounded_retries() {
    let rig = Rig::new();
    let g = graph(
        "strict",
        "n",
        (10, 5),
        json!([
            {"id": "n", "kind": "model.structured", "instructions": "Give a count.", "context": [],
             "output_schema": {"type": "object", "properties": {"count": {"type": "integer", "minimum": 0}}, "required": ["count"], "additionalProperties": false},
             "out": "/r", "max_retries": 1, "next": "done"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/r"]}
        ]),
    );
    // First answer violates the schema; the retry (keyed with /retry1)
    // satisfies it.
    let cassette = Cassette::default()
        .with_entry("strict/n#1", r#"Sure! {"count": -2}"#)
        .with_entry("strict/n#1/retry1", r#"{"count": 3}"#);
    let provider = RecordReplayProvider::replay(cassette);
    let r = rig
        .executor(Some(&provider))
        .start(rig.request(g.clone(), json!({})))
        .unwrap();
    assert_eq!(r.state, "COMPLETED");
    assert_eq!(provider.hits(), 2);
    match r.status {
        RunStatus::Completed { outputs, .. } => assert_eq!(outputs["r"]["count"], 3),
        other => panic!("{other:?}"),
    }
    // Persistently bad output fails the run with a typed message; the
    // event log records the failure and the chain still replays.
    let cassette = Cassette::default()
        .with_entry("strict/n#1", "not json")
        .with_entry("strict/n#1/retry1", "{\"count\": \"x\"}");
    let provider = RecordReplayProvider::replay(cassette);
    let r = rig
        .executor(Some(&provider))
        .start(rig.request(g, json!({})))
        .unwrap();
    assert_eq!(r.state, "FAILED");
    assert!(
        matches!(&r.status, RunStatus::Failed { error } if error.contains("did not satisfy the schema")),
        "{:?}",
        r.status
    );
    assert_eq!(
        rig.log.replay(&r.run_id).unwrap().final_state,
        Some(RunState::Failed)
    );
}

#[test]
fn planning_refuses_unregistered_tools_bad_inputs_and_missing_model() {
    let rig = Rig::new();
    let bad_tool = graph(
        "bt",
        "x",
        (5, 5),
        json!([{"id": "x", "kind": "tool.call", "tool": "os.shell", "args": {}, "out": "/x"}]),
    );
    let r = rig
        .executor(None)
        .start(rig.request(bad_tool, json!({})))
        .unwrap();
    assert!(
        matches!(&r.status, RunStatus::Failed { error } if error.contains("os.shell")),
        "{:?}",
        r.status
    );
    assert_eq!(
        rig.log.replay(&r.run_id).unwrap().final_state,
        Some(RunState::Failed)
    );

    let mut needs_model = graph(
        "nm",
        "n",
        (5, 5),
        json!([{"id": "n", "kind": "model.text", "instructions": "hi", "out": "/t"}]),
    );
    needs_model.inputs =
        json!({"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]});
    let r = rig
        .executor(None)
        .start(rig.request(needs_model.clone(), json!({})))
        .unwrap();
    assert!(
        matches!(&r.status, RunStatus::Failed { error } if error.contains("input schema")),
        "{:?}",
        r.status
    );
    let r = rig
        .executor(None)
        .start(rig.request(needs_model, json!({"q": "x"})))
        .unwrap();
    assert!(
        matches!(&r.status, RunStatus::Failed { error } if error.contains("no provider")),
        "{:?}",
        r.status
    );
}

#[test]
fn cancellation_is_acknowledged_and_durable() {
    let rig = Rig::new();
    let g = graph(
        "c",
        "a",
        (5, 5),
        json!([{"id": "a", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": "doc"}, "out": "/a", "next": "z"}, {"id": "z", "kind": "end", "outcome": "completed"}]),
    );
    rig.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    let r = rig.executor(None).start(rig.request(g, json!({}))).unwrap();
    assert_eq!(r.state, "CANCELLED");
    assert!(matches!(r.status, RunStatus::Cancelled));
    let types: Vec<String> = rig
        .log
        .load_stream(&r.run_id)
        .unwrap()
        .iter()
        .map(|e| e.event_type.as_str().to_string())
        .collect();
    assert!(types.contains(&EventType::RunCancelRequested.as_str().to_string()));
    let replay = rig.log.replay(&r.run_id).unwrap();
    assert_eq!(replay.final_state, Some(RunState::Cancelled));
}

#[test]
fn blob_state_store_keeps_snapshots_encrypted_at_rest_and_resumable() {
    use harbor_core::executor::{BlobStateStore, RunStateStore};
    let dir = tempfile::tempdir().unwrap();
    let opts = harbor_core::OpenOptions {
        data_root: dir.path().to_path_buf(),
        device_id: "dev-test".into(),
    };
    let ws = harbor_core::Workspace::open(
        &opts,
        "ws-blob",
        harbor_security::policy::PrivacyMode::LocalOnly,
    )
    .unwrap();
    let store = BlobStateStore::new(ws.blobs_arc(), "ws-blob", dir.path());
    let registry = ToolRegistry::builtin();
    let artifacts = MemoryArtifacts::new().with("tpl", "letter.docx", placeholder_docx());
    let cancel = AtomicBool::new(false);
    let g = graph(
        "fill",
        "fill",
        (6, 3),
        json!([
            {"id": "fill", "kind": "tool.call", "tool": "artifact.fill_placeholders", "args": {"artifact_id": "tpl", "values": {"$state": "/input/values"}}, "out": "/fill", "next": "approve"},
            {"id": "approve", "kind": "approval", "effect_class": "artifact.commit", "batch": "/fill/batch", "next_approved": "done"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/fill/preview"]}
        ]),
    );
    let host = Host {
        log: ws.agent_log.clone(),
        lease_db: lease_db_path(dir.path()),
        store: &store,
        registry: &registry,
        provider: None,
        artifacts: &artifacts,
        knowledge: None,
        workspace_root: None,
        cancel: &cancel,
        executor_id: "blob-test".into(),
        commit_journal: None,
        step: None,
    };
    let exec = Executor::new(host);
    let secret = "Zubaida-Al-Rashid-9981";
    let report = exec
        .start(RunRequest {
            run_id: None,
            workspace_id: "ws-blob".into(),
            graph: g,
            skill_id: None,
            skill_instructions: None,
            inputs: json!({"values": {"name": secret, "ref": "R"}}),
            host_inputs: json!({}),
            model: None,
        })
        .unwrap();
    assert_eq!(report.state, "WAITING_APPROVAL");
    // The snapshot round-trips through the encrypted store…
    let snap = store.load(&report.run_id).unwrap().unwrap();
    assert_eq!(snap.state["input"]["values"]["name"], secret);
    // …and the private input never sits in plaintext under the data root.
    let mut hits = 0;
    for entry in walkdir(dir.path()) {
        if let Ok(bytes) = std::fs::read(&entry) {
            if bytes.windows(secret.len()).any(|w| w == secret.as_bytes()) {
                hits += 1;
                eprintln!("plaintext leak: {}", entry.display());
            }
        }
    }
    assert_eq!(hits, 0, "run snapshot leaked plaintext");
    // A fresh executor (new process in effect) decides from the store.
    let exec2 = Executor::new(Host {
        log: ws.agent_log.clone(),
        lease_db: lease_db_path(dir.path()),
        store: &store,
        registry: &registry,
        provider: None,
        artifacts: &artifacts,
        knowledge: None,
        workspace_root: None,
        cancel: &cancel,
        executor_id: "blob-test-2".into(),
        commit_journal: None,
        step: None,
    });
    let after = exec2.decide(&report.run_id, true).unwrap();
    assert_eq!(after.state, "COMPLETED");
}

fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
    }
    out
}
