//! Security scenarios SEC-046 and SEC-031 (09_Security_Test_Matrix.csv)
//! as executable controls:
//! - `security.sec_046` — run executor split brain: two executors contend
//!   to resume; the generation-fenced lease permits exactly one, and a
//!   stale owner cannot commit events after takeover.
//! - `security.sec_031` — external effect crash window: a retry of the
//!   same effect event (same event id, same chain position) is rejected
//!   by the log's uniqueness and hash chain, and replay counts the
//!   effect exactly once — no duplicate effect after a crash-retry.

use chrono::{Duration, Utc};
use harbor_agent::event::{Actor, EventPayload, EventType, ReplaySemantics, RunEvent};
use harbor_agent::lease::LeaseManager;
use harbor_agent::log::EventLog;

/// SEC-046 part 1: while one executor's lease is live, a second executor
/// contending for the same run is refused — exactly one may resume.
#[test]
fn sec_046_second_executor_blocked_while_lease_live() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("lease.db");
    let log = EventLog::open(&db).unwrap();
    log.create_run("run-a", "ws", Utc::now()).unwrap();
    drop(log);
    let mut a = LeaseManager::open_shared(&db).unwrap();
    let mut b = LeaseManager::open_shared(&db).unwrap();
    let lease_a = a
        .acquire("run-a", "executor-a", Duration::minutes(5), Utc::now())
        .unwrap();
    assert!(a.validate(&lease_a, Utc::now()).is_ok());
    let err = b
        .acquire("run-a", "executor-b", Duration::minutes(5), Utc::now())
        .unwrap_err();
    assert!(
        matches!(err, harbor_agent::lease::LeaseError::Held { held, ref owner }
            if held == lease_a.generation && owner == "executor-a"),
        "the contending executor must see who holds the fence: {err}"
    );
}

/// SEC-046 part 2: after takeover, the STALE owner cannot commit — its
/// events carry the superseded lease generation and the log's fence
/// rejects them.
#[test]
fn sec_046_stale_owner_cannot_commit_after_takeover() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let log = EventLog::open(&db).unwrap();
    log.create_run("run-b", "ws", Utc::now()).unwrap();
    drop(log);

    let mut a = LeaseManager::open_shared(&db).unwrap();
    let lease_a = a
        .acquire("run-b", "executor-a", Duration::seconds(1), Utc::now())
        .unwrap();
    // Lease expires; executor B takes over (new generation).
    let later = Utc::now() + Duration::seconds(5);
    let mut b = LeaseManager::open_shared(&db).unwrap();
    let lease_b = b
        .acquire("run-b", "executor-b", Duration::minutes(5), later)
        .unwrap();
    assert!(lease_b.generation > lease_a.generation);

    // Executor A (split brain, stale) tries to commit an event under its
    // superseded generation against the CURRENT generation: the fence
    // refuses.
    let log = EventLog::open(&db).unwrap();
    let stream = log.load_stream("run-b").unwrap();
    let head = stream.last().unwrap();
    let stale_event = RunEvent {
        run_id: "run-b".into(),
        event_id: "evt-stale-1".into(),
        seq: 1,
        event_type: EventType::RunStepStarted,
        replay_semantics: ReplaySemantics::StateAffecting,
        actor: Actor::Executor,
        lease_generation: lease_a.generation,
        counters: Default::default(),
        payload: EventPayload::StepStarted {
            step_id: "s1".into(),
            description: "stale owner write".into(),
            node_id: None,
            input_hash: None,
        },
        created_at: Utc::now(),
        prev_event_hash: Some(head.hash().unwrap()),
    };
    let err = log
        .append(stale_event, lease_b.generation, None)
        .unwrap_err();
    assert!(
        matches!(err, harbor_agent::log::LogError::LeaseFence { gen, cur, .. }
            if gen == lease_a.generation && cur == lease_b.generation),
        "the stale owner's commit must be fenced: {err}"
    );
}

/// SEC-031: a crashed executor retries the SAME effect event (same event
/// id, same chain position) — the log rejects the duplicate by chain and
/// uniqueness, and replay counts the effect exactly once. The idempotency
/// key is the event id; there is no code path that could apply the effect
/// twice from this log.
#[test]
fn sec_031_duplicate_effect_event_rejected_and_replayed_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let log = EventLog::open(&db).unwrap();
    log.create_run("run-c", "ws", Utc::now()).unwrap();

    let mut gen = 1u64;
    // The original effect-ful step lands once.
    let append_effect = |log: &EventLog, gen: u64, seq: u64, id: &str| {
        let stream = log.load_stream("run-c").unwrap();
        let head = stream.last().unwrap();
        let evt = RunEvent {
            run_id: "run-c".into(),
            event_id: id.into(),
            seq,
            event_type: EventType::RunStepCompleted,
            replay_semantics: ReplaySemantics::StateAffecting,
            actor: Actor::Executor,
            lease_generation: gen,
            counters: Default::default(),
            payload: EventPayload::StepCompleted {
                step_id: format!("step-{seq}"),
                summary: "external effect dispatched".into(),
                node_id: None,
                output_hash: None,
                tool: Some("filesystem.replace".into()),
            },
            created_at: Utc::now(),
            prev_event_hash: Some(head.hash().unwrap()),
        };
        log.append(evt, gen, None)
    };
    append_effect(&log, gen, 1, "evt-effect-1").unwrap();

    // Crash window: the executor retries the SAME event (same id). The
    // hash chain (the retry's prev hash no longer matches the head it
    // was built against after the original landed) and the event-id
    // uniqueness constraint both refuse — the effect cannot land twice.
    let retry = append_effect(&log, gen, 1, "evt-effect-1");
    assert!(retry.is_err(), "a duplicate effect event must be rejected");

    // Replay: the effect is counted exactly once.
    let report = log.replay("run-c").unwrap();
    let stream = log.load_stream("run-c").unwrap();
    let effects = stream
        .iter()
        .filter(|e| {
            matches!(
                &e.payload,
                EventPayload::StepCompleted { tool: Some(t), .. } if t == "filesystem.replace"
            )
        })
        .count();
    assert_eq!(effects, 1, "exactly one effect after crash-retry");
    assert_eq!(
        report.verified_events,
        stream.len(),
        "every event replays and verifies"
    );
    let _ = &mut gen;
}
