//! Safe commit of an approved proposal (production plan B1): the approval
//! node parks the run with a bound batch + diff; `decide_and_commit`
//! re-derives the approved output from the base bytes, records the
//! decision and dispatch durably, publishes once through `SafeCommitter`
//! (Save New Copy by default; Overwrite revalidates the base inside the
//! protected interval), records the resolution and finishes the graph.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde_json::{json, Value};

use harbor_agent::{EventLog, EventPayload, EventType, RunState};
use harbor_core::executor::{
    commit_journal_path, lease_db_path, CommitTarget, ExecError, Executor, FileStateStore, Host,
    RunRequest, RunStateStore, RunStatus,
};
use harbor_core::graph::Graph;
use harbor_core::tools::{MemoryArtifacts, ToolRegistry};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(repo_root().join("fixtures/office").join(name)).unwrap()
}

struct Rig {
    dir: tempfile::TempDir,
    log: Arc<EventLog>,
    store: FileStateStore,
    registry: ToolRegistry,
    cancel: AtomicBool,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("db")).unwrap();
        let log = Arc::new(EventLog::open(lease_db_path(dir.path())).unwrap());
        Rig {
            store: FileStateStore::new(dir.path().join("runs")),
            registry: ToolRegistry::builtin(),
            cancel: AtomicBool::new(false),
            log,
            dir,
        }
    }

    /// A fresh executor "call" with whatever bytes the host re-supplies.
    fn executor<'a>(&'a self, artifacts: &'a MemoryArtifacts) -> Executor<'a> {
        Executor::new(Host {
            log: self.log.clone(),
            lease_db: lease_db_path(self.dir.path()),
            store: &self.store,
            registry: &self.registry,
            provider: None,
            artifacts,
            knowledge: None,
            workspace_root: None,
            cancel: &self.cancel,
            executor_id: "commit-test".into(),
            commit_journal: Some(commit_journal_path(self.dir.path())),
        })
    }

    fn events(&self, run_id: &str) -> Vec<(EventType, EventPayload)> {
        self.log
            .load_stream(run_id)
            .unwrap()
            .into_iter()
            .map(|e| (e.event_type, e.payload))
            .collect()
    }

    fn resolved_outcomes(&self, run_id: &str) -> Vec<String> {
        self.events(run_id)
            .into_iter()
            .filter_map(|(_, p)| match p {
                EventPayload::EffectResolved { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect()
    }
}

fn fill_graph() -> Graph {
    Graph::from_value(&json!({
        "schema": "harbor.graph/v1",
        "id": "fill-commit",
        "version": 1,
        "inputs": {"type": "object"},
        "entry": "fill",
        "budgets": {"max_steps": 8, "max_tool_calls": 4},
        "nodes": [
            {"id": "fill", "kind": "tool.call", "tool": "artifact.fill_placeholders", "args": {"artifact_id": "tpl", "values": {"$state": "/input/values"}}, "out": "/fill", "next": "approve"},
            {"id": "approve", "kind": "approval", "effect_class": "artifact.commit", "batch": "/fill/batch", "next_approved": "done", "next_rejected": "rejected"},
            {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/approvals/approve"]},
            {"id": "rejected", "kind": "end", "outcome": "abstained"}
        ]
    }))
    .unwrap()
}

fn start(
    rig: &Rig,
    artifacts: &MemoryArtifacts,
    values: Value,
) -> harbor_core::executor::RunReport {
    rig.executor(artifacts)
        .start(RunRequest {
            run_id: None,
            workspace_id: "ws-commit".into(),
            graph: fill_graph(),
            skill_id: Some("placeholder-fill".into()),
            skill_instructions: None,
            inputs: json!({"values": values}),
            host_inputs: json!({}),
            model: None,
        })
        .unwrap()
}

fn template() -> MemoryArtifacts {
    MemoryArtifacts::new().with(
        "tpl",
        "letter_template.docx",
        fixture("letter_template.docx"),
    )
}

fn docx_text(bytes: &[u8]) -> String {
    harbor_artifacts::DocxDocument::load(bytes)
        .unwrap()
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn approval_carries_a_before_after_diff() {
    let rig = Rig::new();
    let tpl = template();
    let report = start(
        &rig,
        &tpl,
        json!({"name": "Amina Haddad", "ref": "2026-09-19", "AMOUNT": "1", "sender": "H"}),
    );
    assert_eq!(report.state, "WAITING_APPROVAL");
    let pending = match &report.status {
        RunStatus::WaitingApproval { approval } => (**approval).clone(),
        other => panic!("{other:?}"),
    };
    assert!(
        !pending.diff.is_empty(),
        "diff computed while bytes are attached"
    );
    assert!(pending.requested_at.is_some());
    for entry in &pending.diff {
        assert_eq!(entry.kind, "text.replace");
        assert!(entry.location.starts_with("¶ "), "{entry:?}");
        let before = entry.before.as_deref().expect("base paragraph text");
        let after = entry.after.as_deref().expect("proposed text");
        assert!(
            ["{{", "[[", "$", "<<"].iter().any(|m| before.contains(m)),
            "before must be the placeholder paragraph: {before}"
        );
        assert!(!after.contains("{{") && !after.contains("[[") && !after.contains("<<"));
    }
    // The diff is persisted in the snapshot, so a later review needs no
    // document bytes.
    let snap = rig.store.load(&report.run_id).unwrap().unwrap();
    assert_eq!(snap.pending_approval.unwrap().diff, pending.diff);
}

#[test]
fn save_new_copy_commits_once_records_the_effect_and_finishes_the_run() {
    let rig = Rig::new();
    let tpl = template();
    let report = start(
        &rig,
        &tpl,
        json!({"name": "Amina Haddad", "ref": "2026-09-19", "AMOUNT": "1", "sender": "H"}),
    );
    let pending = match &report.status {
        RunStatus::WaitingApproval { approval } => (**approval).clone(),
        other => panic!("{other:?}"),
    };
    let proposed = pending.proposed_output_hash.clone().unwrap();
    let base = pending.base_content_hash.clone().unwrap();
    let out_dir = tempfile::tempdir().unwrap();
    let destination = out_dir.path().join("letter (Harbor).docx");

    // Host re-supplies the same base bytes in a fresh executor (new call).
    let (after, commit) = rig
        .executor(&tpl)
        .decide_and_commit(
            &report.run_id,
            CommitTarget::SaveNewCopy {
                destination: destination.clone(),
            },
        )
        .unwrap();
    assert_eq!(after.state, "COMPLETED");
    assert_eq!(commit.outcome, "committed");
    assert_eq!(commit.mode, "new_copy");
    assert_eq!(commit.destination, destination);
    assert_eq!(commit.proposed_output_hash, proposed);
    assert_eq!(commit.base_content_hash, base);
    assert_eq!(commit.effect_id, pending.effect_id);
    assert_eq!(commit.receipt_id, pending.receipt_id);
    assert!(commit.version_id.starts_with("v-"));

    // The file on disk IS the approved output; the original was untouched.
    let written = std::fs::read(&destination).unwrap();
    assert_eq!(harbor_canonical::sha256_hex(&written), proposed);
    assert_eq!(commit.bytes_written, written.len() as u64);
    assert!(docx_text(&written).contains("Amina Haddad"));
    assert_eq!(
        harbor_canonical::sha256_hex(&fixture("letter_template.docx")),
        base
    );

    // Durable record: approval decided, effect dispatched, effect resolved
    // (committed), run completed, chain verifies; the blackboard carries
    // the commit bound to the approval.
    let types: Vec<String> = rig
        .events(&report.run_id)
        .iter()
        .map(|(t, _)| t.as_str().to_string())
        .collect();
    let pos = |t: &str| types.iter().position(|x| x == t).unwrap();
    assert!(pos("run.approval_decided") < pos("run.effect_dispatched"));
    assert!(pos("run.effect_dispatched") < pos("run.effect_resolved"));
    assert_eq!(rig.resolved_outcomes(&report.run_id), ["committed"]);
    let attempt = rig
        .events(&report.run_id)
        .into_iter()
        .find_map(|(_, p)| match p {
            EventPayload::EffectDispatched { attempt_id, .. } => Some(attempt_id),
            _ => None,
        })
        .unwrap();
    assert_eq!(attempt, commit.attempt_id);
    match &after.status {
        RunStatus::Completed { outputs, .. } => {
            let a = &outputs["approvals.approve"];
            assert_eq!(a["approved"], true);
            assert_eq!(a["commit"]["outcome"], "committed");
            assert_eq!(a["commit"]["mode"], "new_copy");
            assert_eq!(a["commit"]["version_id"], commit.version_id);
        }
        other => panic!("{other:?}"),
    }
    let replay = rig.log.replay(&report.run_id).unwrap();
    assert_eq!(replay.final_state, Some(RunState::Completed));
    assert_eq!(replay.verified_events as u64, after.events);
    assert_eq!(
        after
            .trail
            .iter()
            .find(|t| t.node_id == "approve")
            .and_then(|t| t.decision.as_deref()),
        Some("approved:committed")
    );

    // The commit journal is finalized for the batch.
    let committer =
        harbor_artifacts::SafeCommitter::new(commit_journal_path(rig.dir.path())).unwrap();
    assert_eq!(
        committer.recover(&commit.batch_id).unwrap(),
        harbor_artifacts::commit::RecoveryAction::AlreadyCommitted {
            version_id: commit.version_id.clone()
        }
    );

    // The receipt is consumed with the run: a second commit is refused.
    match rig
        .executor(&tpl)
        .decide_and_commit(&report.run_id, CommitTarget::SaveNewCopy { destination })
    {
        Err(ExecError::WrongState { state, .. }) => assert_eq!(state, "COMPLETED"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn commit_is_refused_before_dispatch_when_preconditions_fail() {
    let rig = Rig::new();
    let tpl = template();
    let report = start(
        &rig,
        &tpl,
        json!({"name": "A", "ref": "B", "AMOUNT": "1", "sender": "H"}),
    );
    let out_dir = tempfile::tempdir().unwrap();
    let destination = out_dir.path().join("out.docx");

    // Base bytes that no longer match the approved base hash.
    let changed =
        MemoryArtifacts::new().with("tpl", "letter_template.docx", fixture("structured.docx"));
    match rig.executor(&changed).decide_and_commit(
        &report.run_id,
        CommitTarget::SaveNewCopy {
            destination: destination.clone(),
        },
    ) {
        Err(ExecError::Commit(msg)) => assert!(msg.contains("base file changed"), "{msg}"),
        other => panic!("{other:?}"),
    }
    // Base bytes missing entirely.
    let none = MemoryArtifacts::new();
    assert!(matches!(
        rig.executor(&none).decide_and_commit(
            &report.run_id,
            CommitTarget::SaveNewCopy {
                destination: destination.clone(),
            },
        ),
        Err(ExecError::Commit(_))
    ));
    // Save New Copy never overwrites an existing destination.
    std::fs::write(&destination, b"someone else's file").unwrap();
    match rig.executor(&tpl).decide_and_commit(
        &report.run_id,
        CommitTarget::SaveNewCopy {
            destination: destination.clone(),
        },
    ) {
        Err(ExecError::Commit(msg)) => assert!(msg.contains("already exists"), "{msg}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(&destination).unwrap(), b"someone else's file");

    // Nothing durable happened: still WAITING_APPROVAL, no decision or
    // dispatch recorded, and the ordinary reject path still works.
    let (state, _, _) = rig.log.run_state(&report.run_id).unwrap();
    assert_eq!(state, RunState::WaitingApproval);
    let types: Vec<String> = rig
        .events(&report.run_id)
        .iter()
        .map(|(t, _)| t.as_str().to_string())
        .collect();
    assert!(!types.contains(&"run.approval_decided".to_string()));
    assert!(!types.contains(&"run.effect_dispatched".to_string()));
    let rejected = rig.executor(&tpl).decide(&report.run_id, false).unwrap();
    assert_eq!(rejected.state, "COMPLETED");
    assert_eq!(rig.resolved_outcomes(&report.run_id), ["aborted"]);
}

#[test]
fn expired_receipt_is_refused() {
    let rig = Rig::new();
    let tpl = template();
    let report = start(
        &rig,
        &tpl,
        json!({"name": "A", "ref": "B", "AMOUNT": "1", "sender": "H"}),
    );
    // Age the approval past the receipt validity window in the snapshot.
    let mut snap = rig.store.load(&report.run_id).unwrap().unwrap();
    let p = snap.pending_approval.as_mut().unwrap();
    p.requested_at = Some(chrono::Utc::now() - chrono::Duration::minutes(16));
    rig.store.save(&snap).unwrap();
    let dest = tempfile::tempdir().unwrap();
    match rig.executor(&tpl).decide_and_commit(
        &report.run_id,
        CommitTarget::SaveNewCopy {
            destination: dest.path().join("late.docx"),
        },
    ) {
        Err(ExecError::Commit(msg)) => assert!(msg.contains("expired"), "{msg}"),
        other => panic!("{other:?}"),
    }
    assert!(!dest.path().join("late.docx").exists());
}

#[test]
fn overwrite_revalidates_the_base_inside_the_protected_interval() {
    let rig = Rig::new();
    let tpl = template();
    let work = tempfile::tempdir().unwrap();
    let original = work.path().join("letter.docx");
    std::fs::write(&original, fixture("letter_template.docx")).unwrap();

    // 1. Happy path: the file on disk still equals the approved base.
    let report = start(
        &rig,
        &tpl,
        json!({"name": "Amina", "ref": "today", "AMOUNT": "1", "sender": "H"}),
    );
    let proposed = match &report.status {
        RunStatus::WaitingApproval { approval } => approval.proposed_output_hash.clone().unwrap(),
        other => panic!("{other:?}"),
    };
    let (after, commit) = rig
        .executor(&tpl)
        .decide_and_commit(
            &report.run_id,
            CommitTarget::Overwrite {
                destination: original.clone(),
            },
        )
        .unwrap();
    assert_eq!(after.state, "COMPLETED");
    assert_eq!(commit.mode, "provider_compare_and_swap");
    assert_eq!(
        harbor_canonical::sha256_hex(&std::fs::read(&original).unwrap()),
        proposed
    );
    assert!(!work
        .path()
        .join(format!(".harbor-stage-{}.tmp", commit.batch_id))
        .exists());

    // 2. Conflict: the host's bytes are the approved base, but the file on
    //    disk was replaced by a third version after approval. The dispatch
    //    is recorded, the write is refused inside the protected interval,
    //    the run fails with the reason, and the third version survives.
    let report2 = start(
        &rig,
        &tpl,
        json!({"name": "Zayd", "ref": "tomorrow", "AMOUNT": "2", "sender": "Z"}),
    );
    std::fs::write(&original, b"edited elsewhere").unwrap();
    match rig.executor(&tpl).decide_and_commit(
        &report2.run_id,
        CommitTarget::Overwrite {
            destination: original.clone(),
        },
    ) {
        Err(ExecError::CommitFailed {
            outcome,
            error,
            report,
        }) => {
            assert_eq!(outcome, "conflict");
            assert!(error.contains("base file changed"), "{error}");
            assert_eq!(report.state, "FAILED");
            assert!(matches!(report.status, RunStatus::Failed { .. }));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(&original).unwrap(), b"edited elsewhere");
    let (state, _, _) = rig.log.run_state(&report2.run_id).unwrap();
    assert_eq!(state, RunState::Failed);
    assert_eq!(rig.resolved_outcomes(&report2.run_id), ["conflict"]);
    let replay = rig.log.replay(&report2.run_id).unwrap();
    assert_eq!(replay.final_state, Some(RunState::Failed));
    // No staging leftovers in the user's directory.
    assert_eq!(
        std::fs::read_dir(work.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with(".harbor-stage-"))
            .count(),
        0
    );
}
