//! Kill/restart/replay for the graph executor (decision 0006 open item,
//! M1 checklist "kill/restart/replay at every run transition").
//!
//! A process death is simulated by a host resource that panics at a chosen
//! call (a "fuse"); the panic unwinds through the executor without releasing
//! its lease or saving a snapshot — exactly what a crash leaves behind. A
//! second executor then reopens the same log, lease database and state
//! store, resumes, and must reproduce the uninterrupted run: same node
//! trail, same io hashes, same final blackboard, same outputs, verifiable
//! chain. The fuse is placed before the first node, mid-tool-node and
//! mid-model-node so every executor transition is covered.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use harbor_agent::{EventLog, EventPayload, EventType, RunState};
use harbor_core::executor::{
    lease_db_path, ExecError, Executor, FileStateStore, Host, NodeTrace, RunRequest, RunStateStore,
    RunStatus,
};
use harbor_core::graph::Graph;
use harbor_core::tools::{ArtifactBytes, ArtifactSource, MemoryArtifacts, ToolRegistry};
use harbor_inference::provider::{
    Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError,
};
use harbor_inference::{Cassette, RecordReplayProvider};

fn fixture(name: &str) -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read(root.join("fixtures/office").join(name)).unwrap()
}

/// Counts host calls (artifact reads + model generations) and panics on
/// the `arm`-th one. `0` never fires.
#[derive(Default)]
struct Fuse {
    calls: AtomicUsize,
    arm: AtomicUsize,
}

impl Fuse {
    fn arm(&self, at: usize) {
        self.calls.store(0, Ordering::SeqCst);
        self.arm.store(at, Ordering::SeqCst);
    }

    fn tick(&self, what: &str) {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let at = self.arm.load(Ordering::SeqCst);
        if at != 0 && n == at {
            panic!("simulated crash during {what} (host call {n})");
        }
    }
}

struct CrashingArtifacts {
    inner: MemoryArtifacts,
    fuse: Arc<Fuse>,
}

impl ArtifactSource for CrashingArtifacts {
    fn get(&self, artifact_id: &str) -> Option<ArtifactBytes> {
        self.fuse.tick("artifact.read");
        self.inner.get(artifact_id)
    }
    fn ids(&self) -> Vec<String> {
        self.inner.ids()
    }
}

struct CrashingProvider {
    inner: RecordReplayProvider,
    fuse: Arc<Fuse>,
}

impl ModelProvider for CrashingProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn capabilities(&self) -> &'static [Capabilities] {
        self.inner.capabilities()
    }
    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        self.inner.supports(model, need)
    }
    fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
        self.inner.load(model)
    }
    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
        self.inner.unload(model)
    }
    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.fuse.tick("model.generate");
        self.inner.generate(req)
    }
    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        self.inner.execution_location()
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    log: Arc<EventLog>,
    lease_db: std::path::PathBuf,
    store: FileStateStore,
    registry: ToolRegistry,
    artifacts: CrashingArtifacts,
    provider: CrashingProvider,
    cancel: AtomicBool,
    fuse: Arc<Fuse>,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("db")).unwrap();
        let lease_db = lease_db_path(dir.path());
        let log = Arc::new(EventLog::open(&lease_db).unwrap());
        let fuse = Arc::new(Fuse::default());
        let cassette = Cassette::default().with_entry(
            "kill/sum#1",
            r#"{"summary": "Two documents were read.", "documents": 2}"#,
        );
        Rig {
            store: FileStateStore::new(dir.path().join("runs")),
            registry: ToolRegistry::builtin(),
            artifacts: CrashingArtifacts {
                inner: MemoryArtifacts::new()
                    .with("doc", "structured.docx", fixture("structured.docx"))
                    .with("wb", "board_demo.xlsx", fixture("board_demo.xlsx")),
                fuse: fuse.clone(),
            },
            provider: CrashingProvider {
                inner: RecordReplayProvider::replay(cassette),
                fuse: fuse.clone(),
            },
            cancel: AtomicBool::new(false),
            fuse,
            lease_db,
            log,
            _dir: dir,
        }
    }

    /// One executor "process". Restarts reuse the durable substrate but
    /// get a fresh executor value.
    fn executor(&self, executor_id: &str) -> Executor<'_> {
        Executor::new(Host {
            log: self.log.clone(),
            lease_db: self.lease_db.clone(),
            store: &self.store,
            registry: &self.registry,
            provider: Some(&self.provider),
            artifacts: &self.artifacts,
            knowledge: None,
            workspace_root: None,
            cancel: &self.cancel,
            executor_id: executor_id.into(),
            commit_journal: None,
            step: None,
        })
    }

    fn request(&self, run_id: &str) -> RunRequest {
        RunRequest {
            run_id: Some(run_id.into()),
            workspace_id: "ws-kill".into(),
            graph: graph(),
            skill_id: Some("kill-test".into()),
            skill_instructions: Some("You are Harbor. Never invent facts.".into()),
            inputs: json!({"doc": "doc", "wb": "wb"}),
            host_inputs: json!({}),
            model: Some(ModelRef::InstalledPackage {
                package_id: "cassette".into(),
            }),
        }
    }

    fn started_steps(&self, run_id: &str) -> Vec<String> {
        self.log
            .load_stream(run_id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.payload {
                EventPayload::StepStarted { step_id, .. } => Some(step_id),
                _ => None,
            })
            .collect()
    }

    fn completed_steps(&self, run_id: &str) -> Vec<String> {
        self.log
            .load_stream(run_id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.payload {
                EventPayload::StepCompleted { step_id, .. } => Some(step_id),
                _ => None,
            })
            .collect()
    }

    fn notes(&self, run_id: &str) -> Vec<String> {
        self.log
            .load_stream(run_id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.payload {
                EventPayload::Note { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }
}

/// Two tool nodes, one model node, a branch and an end: host calls are
/// `read_doc` (1), `read_wb` (2), `sum` (3).
fn graph() -> Graph {
    Graph::from_value(&json!({
        "schema": "harbor.graph/v1",
        "id": "kill",
        "version": 1,
        "inputs": {"type": "object"},
        "entry": "read_doc",
        "budgets": {"max_steps": 12, "max_tool_calls": 6},
        "nodes": [
            {"id": "read_doc", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": {"$state": "/input/doc"}}, "out": "/doc", "next": "read_wb"},
            {"id": "read_wb", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": {"$state": "/input/wb"}}, "out": "/wb", "next": "sum"},
            {"id": "sum", "kind": "model.structured", "instructions": "Summarize what was read.",
             "context": [{"label": "Paragraphs", "from": "/doc/paragraphs"}],
             "output_schema": {"type": "object", "properties": {"summary": {"type": "string", "minLength": 1}, "documents": {"type": "integer"}}, "required": ["summary", "documents"], "additionalProperties": false},
             "out": "/summary", "next": "check"},
            {"id": "check", "kind": "branch", "cases": [{"when": {"from": "/summary/documents", "op": "gt", "value": 1}, "next": "done"}], "default": "short"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/summary"]},
            {"id": "short", "kind": "end", "outcome": "abstained"}
        ]
    }))
    .unwrap()
}

/// The parts of a trail that must be identical between an uninterrupted
/// run and a crashed-and-resumed one (timings are not).
fn trail_identity(trail: &[NodeTrace]) -> Vec<(String, String, u32, String, Option<String>)> {
    trail
        .iter()
        .map(|t| {
            (
                t.node_id.clone(),
                t.kind.clone(),
                t.iteration,
                t.input_hash.clone(),
                t.output_hash.clone(),
            )
        })
        .collect()
}

#[test]
fn kill_and_restart_reproduces_the_uninterrupted_run_at_every_transition() {
    let rig = Rig::new();

    // Reference: the run nobody killed.
    rig.fuse.arm(0);
    let reference = rig
        .executor("exec-ref")
        .start(rig.request("run-ref"))
        .unwrap();
    assert_eq!(reference.state, "COMPLETED");
    let reference_outputs = match &reference.status {
        RunStatus::Completed { outputs, .. } => outputs.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        reference
            .trail
            .iter()
            .map(|t| t.node_id.as_str())
            .collect::<Vec<_>>(),
        ["read_doc", "read_wb", "sum", "check", "done"]
    );

    // Crash at host call 1 (before the first node completes — the run
    // has only just become RUNNING), 2 (mid tool node, previous node's
    // output already on the blackboard) and 3 (mid model node).
    for (fuse_at, crashed_step, expected_started) in [
        (1usize, "read_doc:1", vec!["read_doc:1"]),
        (2, "read_wb:1", vec!["read_doc:1", "read_wb:1"]),
        (3, "sum:1", vec!["read_doc:1", "read_wb:1", "sum:1"]),
    ] {
        let run_id = format!("run-kill-{fuse_at}");
        rig.fuse.arm(fuse_at);
        let executor = rig.executor("exec-crashy");
        let died =
            std::panic::catch_unwind(AssertUnwindSafe(|| executor.start(rig.request(&run_id))));
        drop(executor);
        assert!(
            died.is_err(),
            "fuse {fuse_at} should have killed the executor"
        );

        // What the crash left behind: RUNNING, a started-but-not-completed
        // step, a snapshot that still points at the crashed node, and a
        // lease that was never released.
        let (state, _, _) = rig.log.run_state(&run_id).unwrap();
        assert_eq!(state, RunState::Running, "fuse {fuse_at}");
        let started = rig.started_steps(&run_id);
        assert_eq!(started, expected_started, "fuse {fuse_at}");
        let completed = rig.completed_steps(&run_id);
        assert_eq!(
            completed.len(),
            expected_started.len() - 1,
            "fuse {fuse_at}"
        );
        assert!(!completed.iter().any(|s| s == crashed_step));
        let snap = rig.store.load(&run_id).unwrap().unwrap();
        let crashed_node = crashed_step.split(':').next().unwrap();
        assert_eq!(snap.position.next_node.as_deref(), Some(crashed_node));
        assert_eq!(snap.trail.len(), expected_started.len() - 1);
        assert!(
            snap.state.get("summary").is_none(),
            "no partial model output may be persisted"
        );

        // Generation fencing: a *different* executor cannot take over while
        // the dead one's lease is still valid.
        rig.fuse.arm(0);
        match rig.executor("exec-stranger").resume(&run_id) {
            Err(ExecError::Lease(harbor_agent::LeaseError::Held { owner, .. })) => {
                assert_eq!(owner, "exec-crashy")
            }
            other => panic!("stranger must be fenced out, got {other:?}"),
        }
        // Wrong-state refusal on the reference run is unchanged.
        assert!(matches!(
            rig.executor("exec-crashy").resume("run-ref"),
            Err(ExecError::WrongState { .. })
        ));

        // Restart: the same executor identity (the FFI uses a stable id)
        // reopens the substrate and resumes. The lease is renewed, the
        // crashed node re-executes from the last saved blackboard, and
        // the run finishes exactly as the reference did.
        let resumed = rig.executor("exec-crashy").resume(&run_id).unwrap();
        assert_eq!(resumed.state, "COMPLETED", "fuse {fuse_at}");
        assert_eq!(
            trail_identity(&resumed.trail),
            trail_identity(&reference.trail),
            "fuse {fuse_at}: node trail / io hashes diverged after resume"
        );
        assert_eq!(resumed.state_hash, reference.state_hash, "fuse {fuse_at}");
        assert_eq!(resumed.blackboard, reference.blackboard, "fuse {fuse_at}");
        match &resumed.status {
            RunStatus::Completed { outputs, .. } => assert_eq!(*outputs, reference_outputs),
            other => panic!("{other:?}"),
        }
        assert_eq!(resumed.tool_calls, reference.tool_calls);

        // Durable evidence of the interruption: the crashed step was
        // started twice (once orphaned, once resumed), every step was
        // completed exactly once, the resume is noted, and the chain
        // still verifies end to end with one extra step admitted.
        let started = rig.started_steps(&run_id);
        assert_eq!(
            started.iter().filter(|s| *s == crashed_step).count(),
            2,
            "fuse {fuse_at}"
        );
        let completed = rig.completed_steps(&run_id);
        assert_eq!(
            completed,
            ["read_doc:1", "read_wb:1", "sum:1", "check:1", "done:1"]
        );
        assert!(rig
            .notes(&run_id)
            .iter()
            .any(|n| n == "resumed from snapshot"));
        let replay = rig.log.replay(&run_id).unwrap();
        assert_eq!(replay.final_state, Some(RunState::Completed));
        assert_eq!(replay.verified_events as u64, resumed.events);
        assert_eq!(replay.counters.step_count_total, reference.steps + 1);
        let types: Vec<EventType> = rig
            .log
            .load_stream(&run_id)
            .unwrap()
            .iter()
            .map(|e| e.event_type.clone())
            .collect();
        assert_eq!(
            types
                .iter()
                .filter(|t| **t == EventType::RunLeaseAcquired)
                .count(),
            2,
            "fuse {fuse_at}: one lease per process generation"
        );
        // The persisted snapshot is the resumed run's final state.
        let snap = rig.store.load(&run_id).unwrap().unwrap();
        assert_eq!(snap.state_hash, reference.state_hash);
        assert_eq!(snap.position.next_node, None);
        let _: Value = snap.state;
    }
}

#[test]
fn resume_is_idempotent_once_the_run_has_finished() {
    let rig = Rig::new();
    rig.fuse.arm(0);
    let r = rig.executor("exec").start(rig.request("run-done")).unwrap();
    assert_eq!(r.state, "COMPLETED");
    match rig.executor("exec").resume("run-done") {
        Err(ExecError::WrongState {
            state, expected, ..
        }) => {
            assert_eq!(state, "COMPLETED");
            assert_eq!(expected, "RUNNING");
        }
        other => panic!("{other:?}"),
    }
    match rig.executor("exec").resume("run-never") {
        Err(ExecError::NoSnapshot(id)) => assert_eq!(id, "run-never"),
        other => panic!("{other:?}"),
    }
}
