//! Kill/restart durability, replay authority rules, tamper evidence and
//! lease fencing for the durable run runtime (M1 reference behaviors).

use chrono::Utc;
use harbor_agent::event::{Actor, Counters, EventPayload, EventType, ReplaySemantics, RunEvent};
use harbor_agent::lease::LeaseManager;
use harbor_agent::{EventLog, PauseReason, RunState};

fn base(run_id: &str, seq: u64, prev: Option<String>) -> RunEvent {
    RunEvent {
        run_id: run_id.into(),
        event_id: format!("evt-{run_id}-{seq}"),
        seq,
        event_type: EventType::RunTransition,
        replay_semantics: ReplaySemantics::StateAffecting,
        actor: Actor::Executor,
        lease_generation: 1,
        counters: Counters::default(),
        payload: EventPayload::Transition {
            from_state: RunState::Created,
            to_state: RunState::Planning,
            reason: None,
        },
        created_at: Utc::now(),
        prev_event_hash: prev,
    }
}

#[test]
fn kill_and_restart_replays_to_durable_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let last_hash;
    let run_id = "run-durable-1";
    {
        let log = EventLog::open(&path).unwrap();
        log.create_run(run_id, "ws-1", Utc::now()).unwrap();
        let mut mgr = LeaseManager::open(&path).unwrap();
        let lease = mgr
            .acquire(run_id, "exec-1", chrono::Duration::minutes(5), Utc::now())
            .unwrap();
        assert_eq!(lease.generation, 1);

        let mut e = base(run_id, 1, None);
        e.prev_event_hash = Some(log.load_stream(run_id).unwrap()[0].hash().unwrap());
        e.payload = EventPayload::Transition {
            from_state: RunState::Created,
            to_state: RunState::Planning,
            reason: None,
        };
        let h1 = log.append(e, 1, None).unwrap();

        let mut e2 = base(run_id, 2, Some(h1.clone()));
        e2.payload = EventPayload::Transition {
            from_state: RunState::Planning,
            to_state: RunState::Running,
            reason: None,
        };
        e2.counters = Counters {
            active_compute_ms_total: 1500,
            step_count_total: 1,
            tool_count_total: 0,
            context_tokens_total: 2048,
        };
        let h2 = log.append(e2, 1, None).unwrap();

        let mut e3 = base(run_id, 3, Some(h2));
        e3.payload = EventPayload::Transition {
            from_state: RunState::Running,
            to_state: RunState::Paused,
            reason: Some(PauseReason::User),
        };
        e3.counters = Counters {
            active_compute_ms_total: 2400,
            step_count_total: 2,
            tool_count_total: 1,
            context_tokens_total: 3100,
        };
        last_hash = log.append(e3, 1, None).unwrap();
    }
    // Process "killed": all handles dropped. Restart.
    let log = EventLog::open(&path).unwrap();
    let report = log.replay(run_id).unwrap();
    assert_eq!(report.final_state, Some(RunState::Paused));
    assert_eq!(report.counters.active_compute_ms_total, 2400);
    assert_eq!(report.counters.context_tokens_total, 3100);
    assert_eq!(report.verified_events, 4);
    let (state, counters, gen) = log.run_state(run_id).unwrap();
    assert_eq!(state, RunState::Paused);
    assert_eq!(counters.step_count_total, 2);
    assert_eq!(gen, 1);
    // Chain head intact.
    let stream = log.load_stream(run_id).unwrap();
    assert_eq!(stream.last().unwrap().hash().unwrap(), last_hash);
    // Resume: PAUSED -> RUNNING requires a new lease generation? No: resume
    // is an executor decision; lease is still valid, so same generation.
    let mut mgr = LeaseManager::open(&path).unwrap();
    // Renewal after restart: acquisition always increments the generation
    // (02 contract). The stale generation cannot authorize new events.
    let lease2 = mgr
        .acquire(run_id, "exec-1", chrono::Duration::minutes(5), Utc::now())
        .unwrap();
    assert_eq!(
        lease2.generation, 2,
        "lease acquisition increments generation"
    );
    // New lease can append; old generation value (1) is now fenced.
    let stream = log.load_stream(run_id).unwrap();
    let head = stream.last().unwrap().hash().unwrap();
    let mut resume = base(run_id, stream.len() as u64, Some(head));
    resume.lease_generation = lease2.generation;
    resume.counters = report.counters;
    resume.payload = EventPayload::Transition {
        from_state: RunState::Paused,
        to_state: RunState::Running,
        reason: None,
    };
    log.append(resume, lease2.generation, None).unwrap();
    assert_eq!(log.run_state(run_id).unwrap().0, RunState::Running);
}

#[test]
fn lease_acquisition_always_increments_generation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let log = EventLog::open(&path).unwrap();
    let run_id = "run-lease-gen";
    log.create_run(run_id, "ws", Utc::now()).unwrap();
    let mut mgr = LeaseManager::open(&path).unwrap();
    let l1 = mgr
        .acquire(run_id, "exec-1", chrono::Duration::minutes(1), Utc::now())
        .unwrap();
    mgr.release(&l1, Utc::now()).unwrap();
    let l2 = mgr
        .acquire(run_id, "exec-1", chrono::Duration::minutes(1), Utc::now())
        .unwrap();
    assert_eq!(
        l2.generation,
        l1.generation + 1,
        "lease acquisition increments generation"
    );
}

#[test]
fn stale_generation_cannot_authorize_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let log = EventLog::open(&path).unwrap();
    let run_id = "run-fence";
    log.create_run(run_id, "ws", Utc::now()).unwrap();
    let mut mgr = LeaseManager::open(&path).unwrap();
    let l1 = mgr
        .acquire(run_id, "exec-1", chrono::Duration::minutes(1), Utc::now())
        .unwrap();
    mgr.release(&l1, Utc::now()).unwrap();
    let _l2 = mgr
        .acquire(run_id, "exec-1", chrono::Duration::minutes(1), Utc::now())
        .unwrap();

    let stream = log.load_stream(run_id).unwrap();
    let head_hash = stream.last().unwrap().hash().unwrap();
    let mut e = base(run_id, 1, Some(head_hash));
    e.lease_generation = l1.generation; // stale
    e.payload = EventPayload::Transition {
        from_state: RunState::Created,
        to_state: RunState::Planning,
        reason: None,
    };
    assert!(matches!(
        log.append(e, _l2.generation, None),
        Err(harbor_agent::LogError::LeaseFence { .. })
    ));
}

#[test]
fn tampered_event_breaks_replay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let run_id = "run-tamper";
    {
        let log = EventLog::open(&path).unwrap();
        log.create_run(run_id, "ws", Utc::now()).unwrap();
    }
    // Simulate DB-level tampering with a stored event.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE run_events SET payload = '{\"evil\":true}' WHERE seq = 0",
            [],
        )
        .unwrap();
    }
    let log = EventLog::open(&path).unwrap();
    // Replay must reject the tampered stream: the payload no longer decodes
    // as a valid typed `run.created` payload (extra field) and/or the chain
    // hash no longer matches.
    assert!(
        log.replay(run_id).is_err(),
        "tampered payload must fail replay"
    );
}

#[test]
fn unknown_authority_event_halts_replay_but_display_skips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let run_id = "run-unknown";
    {
        let log = EventLog::open(&path).unwrap();
        log.create_run(run_id, "ws", Utc::now()).unwrap();
    }
    // Inject an unknown AUTHORITY event, correctly chained to the stream
    // head, so replay reaches the classification stage.
    {
        let log = EventLog::open(&path).unwrap();
        let head = log.load_stream(run_id).unwrap().pop().unwrap();
        let prev_hash = head.hash().unwrap();
        let unknown = RunEvent {
            run_id: run_id.into(),
            event_id: "evt-x1".into(),
            seq: 1,
            event_type: EventType::Unknown("run.superuser_grant".into()),
            replay_semantics: ReplaySemantics::AuthorityAffecting,
            actor: Actor::Executor,
            lease_generation: 0,
            counters: Counters::default(),
            payload: EventPayload::Raw(harbor_canonical::parse("{}").unwrap()),
            // Normalize to the log's persisted precision so the hash is
            // stable across reload.
            created_at: Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
            prev_event_hash: Some(prev_hash),
        };
        // Runtime append refuses unknown types; a malicious/legacy writer
        // inserts at the storage layer with a valid chain hash.
        assert!(log.append(unknown.clone(), 0, None).is_err());
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO run_events (run_id, seq, event_id, event_type, replay_semantics, actor, lease_generation,
             active_compute_ms_total, step_count_total, tool_count_total, context_tokens_total, payload, created_at, prev_event_hash, event_hash)
             VALUES (?1, 1, 'evt-x1', 'run.superuser_grant', 'authority_affecting', 'executor', 0,
                     0, 0, 0, 0, '{}', ?2, ?3, ?4)",
            rusqlite::params![
                run_id,
                unknown.created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                unknown.prev_event_hash,
                unknown.hash().unwrap()
            ],
        )
        .unwrap();
    }
    {
        let log = EventLog::open(&path).unwrap();
        assert!(matches!(
            log.replay(run_id),
            Err(harbor_agent::LogError::UnknownEventHalts {
                kind: "authority",
                ..
            })
        ));
    }
    // Now a display-only unknown event, correctly chained: replay skips it.
    let path2 = dir.path().join("agent2.db");
    {
        let log = EventLog::open(&path2).unwrap();
        log.create_run(run_id, "ws", Utc::now()).unwrap();
        let head = log.load_stream(run_id).unwrap().pop().unwrap();
        let prev_hash = head.hash().unwrap();
        let display = RunEvent {
            run_id: run_id.into(),
            event_id: "evt-x2".into(),
            seq: 1,
            event_type: EventType::Unknown("run.confetti".into()),
            replay_semantics: ReplaySemantics::IgnorableDisplay,
            actor: Actor::Executor,
            lease_generation: 0,
            counters: Counters::default(),
            payload: EventPayload::Raw(harbor_canonical::parse("{}").unwrap()),
            created_at: Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
            prev_event_hash: Some(prev_hash),
        };
        let conn = rusqlite::Connection::open(&path2).unwrap();
        conn.execute(
            "INSERT INTO run_events (run_id, seq, event_id, event_type, replay_semantics, actor, lease_generation,
             active_compute_ms_total, step_count_total, tool_count_total, context_tokens_total, payload, created_at, prev_event_hash, event_hash)
             VALUES (?1, 1, 'evt-x2', 'run.confetti', 'ignorable_display', 'executor', 0,
                     0, 0, 0, 0, '{}', ?2, ?3, ?4)",
            rusqlite::params![
                run_id,
                display.created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                display.prev_event_hash,
                display.hash().unwrap()
            ],
        )
        .unwrap();
    }
    let log = EventLog::open(&path2).unwrap();
    let report = log.replay(run_id).unwrap();
    assert_eq!(report.skipped_display_events, 1);
}

#[test]
fn illegal_transitions_are_rejected_at_append() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let log = EventLog::open(&path).unwrap();
    let run_id = "run-illegal";
    log.create_run(run_id, "ws", Utc::now()).unwrap();
    let mut mgr = LeaseManager::open(&path).unwrap();
    let l = mgr
        .acquire(run_id, "exec", chrono::Duration::minutes(5), Utc::now())
        .unwrap();

    // CREATED -> RUNNING is illegal (must pass through PLANNING).
    let stream = log.load_stream(run_id).unwrap();
    let head = stream.last().unwrap().hash().unwrap();
    let mut e = base(run_id, 1, Some(head));
    e.payload = EventPayload::Transition {
        from_state: RunState::Created,
        to_state: RunState::Running,
        reason: None,
    };
    assert!(matches!(
        log.append(e, l.generation, None),
        Err(harbor_agent::LogError::State(
            harbor_agent::StateError::IllegalTransition {
                from: "CREATED",
                to: "RUNNING"
            }
        ))
    ));

    // CANCELLING -> PAUSED with a non-unacknowledged reason is illegal.
    let mut e1 = base(run_id, 1, Some(stream.last().unwrap().hash().unwrap()));
    e1.payload = EventPayload::Transition {
        from_state: RunState::Created,
        to_state: RunState::Cancelling,
        reason: None,
    };
    let h1 = log.append(e1, l.generation, None).unwrap();
    let mut e2 = base(run_id, 2, Some(h1.clone()));
    e2.payload = EventPayload::Transition {
        from_state: RunState::Cancelling,
        to_state: RunState::Paused,
        reason: Some(PauseReason::User),
    };
    assert!(matches!(
        log.append(e2, l.generation, None),
        Err(harbor_agent::LogError::State(
            harbor_agent::StateError::CancellationPauseRequiresUnacknowledged(_)
        ))
    ));
    // With cancellation_unacknowledged it is legal.
    let mut e3 = base(run_id, 2, Some(h1));
    e3.payload = EventPayload::Transition {
        from_state: RunState::Cancelling,
        to_state: RunState::Paused,
        reason: Some(PauseReason::CancellationUnacknowledged),
    };
    log.append(e3, l.generation, None).unwrap();
}

#[test]
fn counters_never_regress() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let log = EventLog::open(&path).unwrap();
    let run_id = "run-counters";
    log.create_run(run_id, "ws", Utc::now()).unwrap();
    let mut mgr = LeaseManager::open(&path).unwrap();
    let l = mgr
        .acquire(run_id, "exec", chrono::Duration::minutes(5), Utc::now())
        .unwrap();
    let stream = log.load_stream(run_id).unwrap();
    let head = stream.last().unwrap().hash().unwrap();
    let mut e = base(run_id, 1, Some(head));
    e.payload = EventPayload::Transition {
        from_state: RunState::Created,
        to_state: RunState::Planning,
        reason: None,
    };
    e.counters = Counters {
        active_compute_ms_total: 1000,
        step_count_total: 1,
        tool_count_total: 0,
        context_tokens_total: 0,
    };
    log.append(e, l.generation, None).unwrap();
    let stream = log.load_stream(run_id).unwrap();
    let head = stream.last().unwrap().hash().unwrap();
    let mut e2 = base(run_id, 2, Some(head));
    e2.payload = EventPayload::Transition {
        from_state: RunState::Planning,
        to_state: RunState::Running,
        reason: None,
    };
    e2.counters = Counters {
        active_compute_ms_total: 900,
        step_count_total: 1,
        tool_count_total: 0,
        context_tokens_total: 0,
    };
    assert!(matches!(
        log.append(e2, l.generation, None),
        Err(harbor_agent::LogError::CounterRegressed(
            _,
            "active_compute_ms_total"
        ))
    ));
}

#[test]
fn terminal_states_accept_no_transitions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let log = EventLog::open(&path).unwrap();
    let run_id = "run-terminal";
    log.create_run(run_id, "ws", Utc::now()).unwrap();
    for (from, to, reason) in [
        (RunState::Created, RunState::Planning, None),
        (RunState::Planning, RunState::Running, None),
        (RunState::Running, RunState::Completed, None),
    ] {
        let mut mgr = LeaseManager::open(&path).unwrap();
        let l = mgr
            .acquire(run_id, "exec", chrono::Duration::minutes(5), Utc::now())
            .unwrap();
        let stream = log.load_stream(run_id).unwrap();
        let head = stream.last().unwrap().hash().unwrap();
        let mut e = base(run_id, stream.len() as u64, Some(head));
        e.lease_generation = l.generation;
        e.payload = EventPayload::Transition {
            from_state: from,
            to_state: to,
            reason,
        };
        log.append(e, l.generation, None).unwrap();
    }
    let mut mgr = LeaseManager::open(&path).unwrap();
    let l = mgr
        .acquire(run_id, "exec", chrono::Duration::minutes(5), Utc::now())
        .unwrap();
    let stream = log.load_stream(run_id).unwrap();
    let head = stream.last().unwrap().hash().unwrap();
    let mut e = base(run_id, stream.len() as u64, Some(head));
    e.lease_generation = l.generation;
    e.payload = EventPayload::Transition {
        from_state: RunState::Completed,
        to_state: RunState::Running,
        reason: None,
    };
    assert!(matches!(
        log.append(e, l.generation, None),
        Err(harbor_agent::LogError::State(
            harbor_agent::StateError::IllegalTransition { .. }
        ))
    ));
}
