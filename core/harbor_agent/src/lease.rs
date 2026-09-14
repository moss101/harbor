//! Generation-fenced executor leases.
//!
//! Exactly one executor owns the authoritative run at any time. Acquiring
//! the lease increments the generation; the durable transaction validates
//! owner + generation + validity before any authoritative event or
//! dispatch authorization. Stale owners cannot authorize dispatch or
//! finalize commits.

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

pub const MIGRATION_LEASES: harbor_store::Migration = harbor_store::Migration {
    version: 2,
    name: "executor_leases",
    sql: "CREATE TABLE IF NOT EXISTS run_leases (
            run_id TEXT NOT NULL REFERENCES runs(run_id),
            generation INTEGER NOT NULL,
            owner TEXT NOT NULL,
            acquired_at TEXT NOT NULL,
            valid_until TEXT NOT NULL,
            released_at TEXT,
            PRIMARY KEY (run_id, generation)
          );",
};

#[derive(Debug, Clone)]
pub struct ExecutorLease {
    pub run_id: String,
    pub generation: u64,
    pub owner: String,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    #[error("another executor holds the lease (generation {held} by {owner})")]
    Held { held: u64, owner: String },
    #[error("lease expired for generation {0}")]
    Expired(u64),
    #[error("no lease held for run")]
    NotHeld,
    #[error("db error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("run not found")]
    RunNotFound,
    #[error("timestamp parse error: {0}")]
    Timestamp(String),
}

pub struct LeaseManager {
    conn: Connection,
}

impl LeaseManager {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, LeaseError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.execute_batch(MIGRATION_LEASES.sql)?;
        Ok(LeaseManager { conn })
    }

    /// Open on the same database as an existing connection (shares file
    /// locks; used by tests and in-process facades).
    pub fn open_shared(path: impl AsRef<std::path::Path>) -> Result<Self, LeaseError> {
        Self::open(path)
    }

    /// Acquire or renew the lease, bumping the generation. Fails while a
    /// live lease held by another owner exists. Returns the new lease.
    pub fn acquire(
        &mut self,
        run_id: &str,
        owner: &str,
        ttl: Duration,
        now: DateTime<Utc>,
    ) -> Result<ExecutorLease, LeaseError> {
        let tx = self.conn.transaction()?;
        let exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM runs WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(LeaseError::RunNotFound);
        }
        let current: Option<(i64, String, String)> = tx
            .query_row(
                "SELECT generation, owner, valid_until FROM run_leases
                 WHERE run_id = ?1 AND released_at IS NULL
                 ORDER BY generation DESC LIMIT 1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        if let Some((gen, owner_held, valid_until_s)) = current.clone() {
            let valid_until = chrono::DateTime::parse_from_rfc3339(&valid_until_s)
                .map_err(|e| LeaseError::Timestamp(e.to_string()))?
                .with_timezone(&chrono::Utc);
            if valid_until > now && owner_held != owner {
                return Err(LeaseError::Held {
                    held: gen as u64,
                    owner: owner_held,
                });
            }
            // Expired or same-owner: release the old generation.
            tx.execute(
                "UPDATE run_leases SET released_at = ?3 WHERE run_id = ?1 AND generation = ?2",
                rusqlite::params![
                    run_id,
                    gen,
                    now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                ],
            )?;
        }
        // Next generation is the max across ALL lease rows (released
        // included): generations are monotonic for the life of the run.
        let max_gen: i64 = tx.query_row(
            "SELECT COALESCE(MAX(generation), 0) FROM run_leases WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )?;
        let new_generation = max_gen as u64 + 1;
        let valid_until = now + ttl;
        tx.execute(
            "INSERT INTO run_leases (run_id, generation, owner, acquired_at, valid_until) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                run_id,
                new_generation as i64,
                owner,
                now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                valid_until.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            ],
        )?;
        tx.execute(
            "UPDATE runs SET executor_generation = ?2, updated_at = ?3 WHERE run_id = ?1",
            rusqlite::params![
                run_id,
                new_generation as i64,
                now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            ],
        )?;
        tx.commit()?;
        Ok(ExecutorLease {
            run_id: run_id.into(),
            generation: new_generation,
            owner: owner.into(),
            valid_until,
        })
    }

    /// Validate a lease against durable state before authoritative use.
    pub fn validate(&self, lease: &ExecutorLease, now: DateTime<Utc>) -> Result<(), LeaseError> {
        let row: Option<(i64, String, String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT generation, owner, valid_until, released_at FROM run_leases
                 WHERE run_id = ?1 AND generation = ?2",
                rusqlite::params![lease.run_id, lease.generation as i64],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some((gen, owner, valid_until_s, released_at)) = row else {
            return Err(LeaseError::NotHeld);
        };
        if released_at.is_some() {
            return Err(LeaseError::NotHeld);
        }
        if owner != lease.owner {
            return Err(LeaseError::Held {
                held: gen as u64,
                owner,
            });
        }
        let valid_until = chrono::DateTime::parse_from_rfc3339(&valid_until_s)
            .map_err(|e| LeaseError::Timestamp(e.to_string()))?
            .with_timezone(&chrono::Utc);
        if now >= valid_until {
            return Err(LeaseError::Expired(lease.generation));
        }
        Ok(())
    }

    /// Current durable generation for a run (0 = none ever held).
    pub fn current_generation(&self, run_id: &str) -> Result<u64, LeaseError> {
        let gen: i64 = self
            .conn
            .query_row(
                "SELECT executor_generation FROM runs WHERE run_id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(|_| LeaseError::RunNotFound)?;
        Ok(gen as u64)
    }

    pub fn release(&mut self, lease: &ExecutorLease, now: DateTime<Utc>) -> Result<(), LeaseError> {
        self.conn.execute(
            "UPDATE run_leases SET released_at = ?3 WHERE run_id = ?1 AND generation = ?2 AND released_at IS NULL",
            rusqlite::params![
                lease.run_id,
                lease.generation as i64,
                now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (LeaseManager, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.db");
        let log = crate::log::EventLog::open(&path).unwrap();
        log.create_run("run-lease-1", "ws", Utc::now()).unwrap();
        let mgr = LeaseManager::open(&path).unwrap();
        std::mem::forget(dir); // keep alive for the test process scope
        (mgr, "run-lease-1".to_string())
    }

    #[test]
    fn acquire_increments_generation() {
        let (mut mgr, run) = setup();
        let now = Utc::now();
        let l1 = mgr
            .acquire(&run, "exec-a", Duration::minutes(5), now)
            .unwrap();
        assert_eq!(l1.generation, 1);
        mgr.validate(&l1, now).unwrap();
        mgr.release(&l1, now).unwrap();
        let l2 = mgr
            .acquire(&run, "exec-a", Duration::minutes(5), now)
            .unwrap();
        assert_eq!(l2.generation, 2);
    }

    #[test]
    fn second_owner_blocked_while_lease_live() {
        let (mut mgr, run) = setup();
        let now = Utc::now();
        let _l1 = mgr
            .acquire(&run, "exec-a", Duration::minutes(5), now)
            .unwrap();
        assert!(matches!(
            mgr.acquire(&run, "exec-b", Duration::minutes(5), now),
            Err(LeaseError::Held { owner, .. }) if owner == "exec-a"
        ));
    }

    #[test]
    fn expired_lease_can_be_taken_over() {
        let (mut mgr, run) = setup();
        let now = Utc::now();
        let _l1 = mgr
            .acquire(&run, "exec-a", Duration::seconds(1), now)
            .unwrap();
        let later = now + Duration::seconds(2);
        // Same owner renewing an expired lease is fine.
        let l2 = mgr
            .acquire(&run, "exec-a", Duration::minutes(5), later)
            .unwrap();
        assert_eq!(l2.generation, 2);
        // Validate old lease fails.
        assert!(matches!(
            mgr.validate(&_l1, later),
            Err(LeaseError::NotHeld)
        ));
    }

    #[test]
    fn stale_generation_cannot_validate() {
        let (mut mgr, run) = setup();
        let now = Utc::now();
        let l1 = mgr
            .acquire(&run, "exec-a", Duration::minutes(5), now)
            .unwrap();
        mgr.release(&l1, now).unwrap();
        let l2 = mgr
            .acquire(&run, "exec-a", Duration::minutes(5), now)
            .unwrap();
        // Old generation cannot validate after release.
        assert!(matches!(mgr.validate(&l1, now), Err(LeaseError::NotHeld)));
        assert_eq!(mgr.current_generation(&run).unwrap(), l2.generation);
    }
}
