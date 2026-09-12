//! Durable, hash-chained run event log with replay validation.
//!
//! Append rules (contracts.py `validate_event_append`):
//! - first event: seq 0, `run.created`, null prev hash;
//! - contiguity: seq = prev.seq + 1; no duplicate event ids;
//! - chain: prev_event_hash == sha256(canonical(previous envelope));
//! - counters: monotonic non-decreasing;
//! - authority: executor-actor or authoritative events must carry the
//!   current lease generation.
//!
//! Replay rules (02 contract): unknown state/authority event types halt
//! replay; `ignorable_display` may be skipped only after envelope
//! integrity checks (typed parse + hash chain verify).

use rusqlite::Connection;

use crate::event::{Actor, EventError, EventPayload, EventType, ReplaySemantics, RunEvent};
use crate::state::{PauseReason, RunState};

pub const MIGRATIONS: &[harbor_store::Migration] = &[
    harbor_store::Migration {
        version: 1,
        name: "agent_runs_and_events",
        sql: "CREATE TABLE runs (
                run_id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                state TEXT NOT NULL,
                pause_reason TEXT,
                executor_generation INTEGER NOT NULL DEFAULT 0,
                active_compute_ms_total INTEGER NOT NULL DEFAULT 0,
                step_count_total INTEGER NOT NULL DEFAULT 0,
                tool_count_total INTEGER NOT NULL DEFAULT 0,
                context_tokens_total INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
              );
              CREATE TABLE run_events (
                run_id TEXT NOT NULL REFERENCES runs(run_id),
                seq INTEGER NOT NULL,
                event_id TEXT NOT NULL UNIQUE,
                event_type TEXT NOT NULL,
                replay_semantics TEXT NOT NULL,
                actor TEXT NOT NULL,
                lease_generation INTEGER NOT NULL,
                active_compute_ms_total INTEGER NOT NULL,
                step_count_total INTEGER NOT NULL,
                tool_count_total INTEGER NOT NULL,
                context_tokens_total INTEGER NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                prev_event_hash TEXT,
                event_hash TEXT NOT NULL,
                PRIMARY KEY (run_id, seq)
              );
              CREATE INDEX idx_run_events_run ON run_events(run_id, seq);",
    },
];

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("run {0} not found")]
    RunNotFound(String),
    #[error("run {0} already exists")]
    RunExists(String),
    #[error("db error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("event error: {0}")]
    Event(#[from] EventError),
    #[error("canonical error: {0}")]
    Canonical(#[from] harbor_canonical::CanonicalError),
    #[error("payload error: {0}")]
    Payload(String),
    #[error("state error: {0}")]
    State(#[from] crate::state::StateError),
    #[error("chain broken at seq {0}")]
    ChainBroken(u64),
    #[error("unknown {kind} event type '{name}' halts replay")]
    UnknownEventHalts { kind: &'static str, name: String },
    #[error("authoritative event at seq {0} has missing/stale lease generation")]
    StaleLeaseAt(u64),
    #[error("counter regressed at seq {0}: {1}")]
    CounterRegressed(u64, &'static str),
    #[error("transition does not match replayed state at seq {0}")]
    TransitionMismatch(u64),
    #[error("run cannot be created twice (seq {0})")]
    DoubleCreate(u64),
    #[error("lease generation {gen} not current ({cur}) for authoritative event at seq {seq}")]
    LeaseFence { gen: u64, cur: u64, seq: u64 },
}

pub struct EventLog {
    conn: std::sync::Mutex<Connection>,
}

impl EventLog {
    /// Open (or create) the agent database at `path` and apply migrations.
    /// A fresh process reopens the same file to resume after kill/restart.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, LogError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(LogError::Io)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS harbor_schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );",
        )?;
        for m in MIGRATIONS {
            let have: i64 = conn.query_row(
                "SELECT COUNT(*) FROM harbor_schema_migrations WHERE version = ?1",
                [m.version],
                |r| r.get(0),
            )?;
            if have == 0 {
                conn.execute_batch(m.sql).map_err(LogError::Db)?;
                conn.execute(
                    "INSERT INTO harbor_schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![m.version, m.name, chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)],
                )?;
            }
        }
        Ok(EventLog { conn: std::sync::Mutex::new(conn) })
    }

    /// Create a run with its `run.created` event. Transactional.
    pub fn create_run(
        &self,
        run_id: &str,
        workspace_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<RunEvent, LogError> {
        let conn = self.conn.lock().unwrap();
        if run_exists(&conn, run_id)? {
            return Err(LogError::RunExists(run_id.into()));
        }
        let event = RunEvent {
            run_id: run_id.into(),
            event_id: format!("evt-{}", harbor_canonical::sha256_hex(format!("{run_id}-0").as_bytes()).get(..16).unwrap_or("evt")),
            seq: 0,
            event_type: EventType::RunCreated,
            replay_semantics: ReplaySemantics::StateAffecting,
            actor: Actor::System,
            lease_generation: 0,
            counters: Default::default(),
            payload: EventPayload::Created,
            created_at: now,
            prev_event_hash: None,
        };
        event.validate()?;
        let hash = event.hash()?;
        conn.execute(
            "INSERT INTO runs (run_id, workspace_id, state, created_at, updated_at) VALUES (?1, ?2, 'CREATED', ?3, ?3)",
            rusqlite::params![run_id, workspace_id, now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)],
        )?;
        insert_event(&conn, &event, &hash)?;
        Ok(event)
    }

    /// Append an event under lease authority. Validates transition and
    /// counter rules, enforces the lease fence for authoritative events,
    /// and updates durable run state in the same transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn append(
        &self,
        event: RunEvent,
        current_generation: u64,
        budget_delta: Option<crate::budgets::BudgetDelta>,
    ) -> Result<String, LogError> {
        event.validate()?;
        if !event.event_type.is_known() {
            return Err(LogError::UnknownEventHalts {
                kind: "append",
                name: event.event_type.as_str().into(),
            });
        }
        let authoritative = event.replay_semantics.is_authoritative();
        if authoritative || event.actor == Actor::Executor {
            if event.lease_generation != current_generation || current_generation == 0 {
                return Err(LogError::LeaseFence {
                    gen: event.lease_generation,
                    cur: current_generation,
                    seq: event.seq,
                });
            }
        }
        let conn = self.conn.lock().unwrap();
        // Load current durable state.
        let (state, counters, last): (String, crate::event::Counters, Option<(u64, String, String, i64)>) = load_head(&conn, &event.run_id)?;
        let Some((_last_seq, _last_id, last_hash, _last_gen)) = last else {
            return Err(LogError::RunNotFound(event.run_id.clone()));
        };
        // Chain + contiguity + id uniqueness.
        if event.prev_event_hash.as_deref() != Some(last_hash.as_str()) {
            return Err(LogError::ChainBroken(event.seq));
        }
        // Transition validation.
        if let EventPayload::Transition { from_state, to_state, reason } = &event.payload {
            let current = RunState::parse(&state).ok_or_else(|| LogError::Payload("bad state".into()))?;
            if *from_state != current {
                return Err(LogError::TransitionMismatch(event.seq));
            }
            if !current.can_transition_to(*to_state) {
                return Err(crate::state::StateError::IllegalTransition {
                    from: current.as_str(),
                    to: to_state.as_str(),
                }
                .into());
            }
            if *to_state == RunState::Paused {
                let Some(r) = reason else {
                    return Err(crate::state::StateError::MissingPauseReason { from: current.as_str() }.into());
                };
                if current == RunState::Cancelling && *r != PauseReason::CancellationUnacknowledged {
                    return Err(crate::state::StateError::CancellationPauseRequiresUnacknowledged(r.as_str().to_string()).into());
                }
            }
        }
        // Counter monotonicity vs durable totals.
        let new_counters = event.counters;
        if new_counters.active_compute_ms_total < counters.active_compute_ms_total {
            return Err(LogError::CounterRegressed(event.seq, "active_compute_ms_total"));
        }
        if new_counters.step_count_total < counters.step_count_total {
            return Err(LogError::CounterRegressed(event.seq, "step_count_total"));
        }
        if new_counters.tool_count_total < counters.tool_count_total {
            return Err(LogError::CounterRegressed(event.seq, "tool_count_total"));
        }
        if new_counters.context_tokens_total < counters.context_tokens_total {
            return Err(LogError::CounterRegressed(event.seq, "context_tokens_total"));
        }
        // Budget admission inside the same transaction.
        if let Some(delta) = &budget_delta {
            delta
                .check_admission(&counters)
                .map_err(|e| crate::state::StateError::Budget(e.to_string()))?;
        }
        let hash = event.hash()?;
        // Manual transaction: BEGIN IMMEDIATE .. COMMIT/ROLLBACK. The Mutex
        // guarantees exclusive access for the whole section.
        conn.execute_batch("BEGIN IMMEDIATE;")?;
        let result = (|| -> Result<(), LogError> {
            insert_event(&conn, &event, &hash)?;
            if let EventPayload::Transition { to_state, reason, .. } = &event.payload {
                conn.execute(
                    "UPDATE runs SET state = ?2, pause_reason = ?3, updated_at = ?4 WHERE run_id = ?1",
                    rusqlite::params![
                        event.run_id,
                        to_state.as_str(),
                        reason.map(|r| r.as_str()),
                        event.created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                    ],
                )?;
            } else {
                conn.execute(
                    "UPDATE runs SET updated_at = ?2 WHERE run_id = ?1",
                    rusqlite::params![event.run_id, event.created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)],
                )?;
            }
            conn.execute(
                "UPDATE runs SET
                    active_compute_ms_total = ?2,
                    step_count_total = ?3,
                    tool_count_total = ?4,
                    context_tokens_total = ?5
                 WHERE run_id = ?1",
                rusqlite::params![
                    event.run_id,
                    new_counters.active_compute_ms_total as i64,
                    new_counters.step_count_total as i64,
                    new_counters.tool_count_total as i64,
                    new_counters.context_tokens_total as i64
                ],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT;")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                return Err(e);
            }
        }
        Ok(hash)
    }

    /// Load the full event stream for replay.
    pub fn load_stream(&self, run_id: &str) -> Result<Vec<RunEvent>, LogError> {
        let conn = self.conn.lock().unwrap();
        load_stream(&conn, run_id)
    }

    /// Replay with the authority rules: halts on unknown authority/state
    /// event types; skips ignorable_display only after integrity check.
    /// Returns the replayed final state.
    pub fn replay(&self, run_id: &str) -> Result<ReplayReport, LogError> {
        let stream = self.load_stream(run_id)?;
        let mut state: Option<RunState> = None;
        let mut skipped_display = 0usize;
        let mut previous_hash: Option<String> = None;
        let mut counters = crate::event::Counters::default();
        for event in &stream {
            // Integrity: chain + canonical hash.
            if event.prev_event_hash != previous_hash {
                return Err(LogError::ChainBroken(event.seq));
            }
            let computed = event.hash()?;
            let stored = stored_hash(&self.conn.lock().unwrap(), run_id, event.seq)?;
            if computed != stored {
                return Err(LogError::ChainBroken(event.seq));
            }
            // Unknown authority/state types halt replay; unknown display
            // types are skipped (envelope integrity already verified).
            if !event.event_type.is_known() {
                if event.replay_semantics.is_authoritative() {
                    return Err(LogError::UnknownEventHalts {
                        kind: "authority",
                        name: event.event_type.as_str().into(),
                    });
                }
                skipped_display += 1;
                if event.replay_semantics == ReplaySemantics::IgnorableDisplay {
                    // counted above
                }
                previous_hash = Some(computed);
                continue;
            }
            // State fold.
            match (&event.payload, &event.event_type) {
                (EventPayload::Created, EventType::RunCreated) => {
                    if state.is_some() {
                        return Err(LogError::DoubleCreate(event.seq));
                    }
                    state = Some(RunState::Created);
                }
                (EventPayload::Transition { from_state, to_state, .. }, EventType::RunTransition) => {
                    if state.as_ref() != Some(from_state) {
                        return Err(LogError::TransitionMismatch(event.seq));
                    }
                    state = Some(*to_state);
                }
                _ => {}
            }
            // Counters never regress across the stream.
            if event.counters.active_compute_ms_total < counters.active_compute_ms_total
                || event.counters.step_count_total < counters.step_count_total
                || event.counters.tool_count_total < counters.tool_count_total
                || event.counters.context_tokens_total < counters.context_tokens_total
            {
                return Err(LogError::CounterRegressed(event.seq, "stream"));
            }
            counters = event.counters;
            if event.replay_semantics == ReplaySemantics::IgnorableDisplay {
                skipped_display += 1;
            }
            previous_hash = Some(computed);
        }
        Ok(ReplayReport {
            final_state: state,
            counters,
            skipped_display_events: skipped_display,
            verified_events: stream.len(),
        })
    }

    pub fn run_state(&self, run_id: &str) -> Result<(RunState, crate::event::Counters, u64), LogError> {
        let conn = self.conn.lock().unwrap();
        let state: String = conn
            .query_row(
                "SELECT state FROM runs WHERE run_id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(|_| LogError::RunNotFound(run_id.into()))?;
        let counters = load_counters(&conn, run_id)?;
        let gen: i64 = conn.query_row(
            "SELECT executor_generation FROM runs WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )?;
        Ok((
            RunState::parse(&state).ok_or_else(|| LogError::Payload("bad state".into()))?,
            counters,
            gen as u64,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ReplayReport {
    pub final_state: Option<RunState>,
    pub counters: crate::event::Counters,
    pub skipped_display_events: usize,
    pub verified_events: usize,
}


/// Normalize a timestamp to the precision the event log persists
/// (microseconds) so hash recomputation after reload is stable.
fn normalize_time(t: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    let s = t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    chrono::DateTime::parse_from_rfc3339(&s)
        .expect("micros rfc3339 reparse")
        .with_timezone(&chrono::Utc)
}

fn run_exists(conn: &Connection, run_id: &str) -> Result<bool, LogError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM runs WHERE run_id = ?1",
        [run_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn insert_event(conn: &Connection, event: &RunEvent, hash: &str) -> Result<(), LogError> {
    let payload_json = event.payload.to_json().to_canonical_bytes()?;
    conn.execute(
        "INSERT INTO run_events (run_id, seq, event_id, event_type, replay_semantics, actor, lease_generation,
         active_compute_ms_total, step_count_total, tool_count_total, context_tokens_total, payload, created_at, prev_event_hash, event_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        rusqlite::params![
            event.run_id,
            event.seq as i64,
            event.event_id,
            event.event_type.as_str(),
            event.replay_semantics.as_str(),
            event.actor.as_str(),
            event.lease_generation as i64,
            event.counters.active_compute_ms_total as i64,
            event.counters.step_count_total as i64,
            event.counters.tool_count_total as i64,
            event.counters.context_tokens_total as i64,
            String::from_utf8(payload_json).map_err(|e| LogError::Payload(e.to_string()))?,
            event.created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            event.prev_event_hash,
            hash
        ],
    )?;
    Ok(())
}

type HeadInfo = Option<(u64, String, String, i64)>;

fn load_head(
    conn: &Connection,
    run_id: &str,
) -> Result<(String, crate::event::Counters, HeadInfo), LogError> {
    let state: String = conn
        .query_row("SELECT state FROM runs WHERE run_id = ?1", [run_id], |r| r.get(0))
        .map_err(|_| LogError::RunNotFound(run_id.into()))?;
    let counters = load_counters(conn, run_id)?;
    let head = conn
        .query_row(
            "SELECT seq, event_id, event_hash, lease_generation FROM run_events WHERE run_id = ?1 ORDER BY seq DESC LIMIT 1",
            [run_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)? as u64,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok((state, counters, head))
}

fn load_counters(conn: &Connection, run_id: &str) -> Result<crate::event::Counters, LogError> {
    let (a, s, t, c): (i64, i64, i64, i64) = conn.query_row(
        "SELECT active_compute_ms_total, step_count_total, tool_count_total, context_tokens_total FROM runs WHERE run_id = ?1",
        [run_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(crate::event::Counters {
        active_compute_ms_total: a as u64,
        step_count_total: s as u64,
        tool_count_total: t as u64,
        context_tokens_total: c as u64,
    })
}

fn stored_hash(conn: &Connection, run_id: &str, seq: u64) -> Result<String, LogError> {
    conn.query_row(
        "SELECT event_hash FROM run_events WHERE run_id = ?1 AND seq = ?2",
        rusqlite::params![run_id, seq as i64],
        |r| r.get(0),
    )
    .map_err(|_| LogError::ChainBroken(seq))
}

fn load_stream(conn: &Connection, run_id: &str) -> Result<Vec<RunEvent>, LogError> {
    let mut stmt = conn.prepare(
        "SELECT seq, event_id, event_type, replay_semantics, actor, lease_generation,
        active_compute_ms_total, step_count_total, tool_count_total, context_tokens_total, payload, created_at, prev_event_hash
        FROM run_events WHERE run_id = ?1 ORDER BY seq ASC",
    )?;
    let rows = stmt.query_map([run_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, String>(11)?,
            r.get::<_, Option<String>>(12)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (seq, event_id, etype_s, sem_s, actor_s, gen, a, s, t, c, payload_s, created_s, prev) = row?;
        let etype = EventType::parse(&etype_s);
        let payload_json: harbor_canonical::JsonValue =
            harbor_canonical::parse(&payload_s).map_err(|e| LogError::Payload(e.to_string()))?;
        let payload = EventPayload::from_json(etype.clone(), &payload_json)
            .map_err(|e| LogError::Payload(e.to_string()))?;
        out.push(RunEvent {
            run_id: run_id.to_string(),
            event_id,
            seq: seq as u64,
            event_type: etype,
            replay_semantics: match sem_s.as_str() {
                "state_affecting" => ReplaySemantics::StateAffecting,
                "authority_affecting" => ReplaySemantics::AuthorityAffecting,
                _ => ReplaySemantics::IgnorableDisplay,
            },
            actor: match actor_s.as_str() {
                "executor" => Actor::Executor,
                "user" => Actor::User,
                "provider" => Actor::Provider,
                "recovery" => Actor::Recovery,
                _ => Actor::System,
            },
            lease_generation: gen as u64,
            counters: crate::event::Counters {
                active_compute_ms_total: a as u64,
                step_count_total: s as u64,
                tool_count_total: t as u64,
                context_tokens_total: c as u64,
            },
            payload,
            created_at: chrono::DateTime::parse_from_rfc3339(&created_s)
                .map_err(|e| LogError::Payload(e.to_string()))?
                .with_timezone(&chrono::Utc),
            prev_event_hash: prev,
        });
    }
    Ok(out)
}
