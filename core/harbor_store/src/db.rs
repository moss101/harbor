//! SQLite database wrapper with an ordered migration framework.
//!
//! Every subsystem (agent runtime, artifacts, knowledge, modelhub) registers
//! its own ordered migrations; `Database::migrate` applies pending ones
//! transactionally and records the applied set. Migrations are append-only:
//! an already-applied migration must never change content.

use rusqlite::Connection;
use std::path::Path;

use crate::error::{Result, StoreError};

#[derive(Debug, Clone)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (creating if needed) with durability-oriented pragmas.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Database { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Database { conn })
    }

    pub fn connection(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Apply all pending migrations in ascending version order, each in its
    /// own transaction, recorded in `harbor_schema_migrations`.
    pub fn migrate(&mut self, migrations: &[Migration]) -> Result<Vec<u32>> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS harbor_schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );",
        )?;
        let mut applied = Vec::new();
        for m in migrations {
            let have: Option<u32> = self
                .conn
                .query_row(
                    "SELECT version FROM harbor_schema_migrations WHERE version = ?1",
                    [m.version],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            if have.is_some() {
                continue;
            }
            let tx = self.conn.transaction()?;
            let outcome = tx
                .execute_batch(m.sql)
                .map(|_| {
                    tx.execute(
                        "INSERT INTO harbor_schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                        rusqlite::params![
                            m.version,
                            m.name,
                            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                        ],
                    )
                })
                .and_then(|_| tx.commit());
            outcome.map_err(|e| {
                StoreError::MigrationFailed(m.version, format!("{}: {}", m.name, e))
            })?;
            applied.push(m.version);
        }
        Ok(applied)
    }

    /// Run a read-only closure with the connection.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        f(&self.conn)
    }

    /// Run a write transaction; the closure's error rolls it back.
    pub fn write<T>(&mut self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let tx = self.conn.transaction()?;
        match f(&tx) {
            Ok(v) => tx.commit().map_err(StoreError::Db).map(|_| v),
            Err(e) => {
                // Dropping without commit rolls back; finish explicitly.
                let _ = tx.finish();
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_migrations_once_and_in_order() {
        let mut db = Database::open_in_memory().unwrap();
        let migrations = vec![
            Migration {
                version: 1,
                name: "base",
                sql: "CREATE TABLE t (id INTEGER PRIMARY KEY);",
            },
            Migration {
                version: 2,
                name: "add_col",
                sql: "ALTER TABLE t ADD COLUMN v TEXT;",
            },
        ];
        let applied = db.migrate(&migrations).unwrap();
        assert_eq!(applied, vec![1, 2]);
        let applied_again = db.migrate(&migrations).unwrap();
        assert!(applied_again.is_empty());
        db.write(|c| {
            c.execute("INSERT INTO t (v) VALUES ('x')", [])?;
            Ok(())
        })
        .unwrap();
        let n: i64 = db
            .read(|c| {
                c.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
                    .map_err(StoreError::Db)
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn failed_migration_rolls_back() {
        let mut db = Database::open_in_memory().unwrap();
        let bad = vec![Migration {
            version: 1,
            name: "bad",
            sql: "CREATE TABLE ( oops;",
        }];
        assert!(matches!(
            db.migrate(&bad),
            Err(StoreError::MigrationFailed(1, _))
        ));
        let count: i64 = db
            .read(|c| {
                c.query_row("SELECT COUNT(*) FROM harbor_schema_migrations", [], |r| {
                    r.get(0)
                })
                .map_err(StoreError::Db)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn write_error_rolls_back() {
        let mut db = Database::open_in_memory().unwrap();
        db.write(|c| {
            c.execute_batch("CREATE TABLE t (x INTEGER);")
                .map_err(StoreError::Db)?;
            Ok(())
        })
        .unwrap();
        let err = db.write(|c| {
            c.execute("INSERT INTO t VALUES (1)", [])
                .map_err(StoreError::Db)?;
            Err::<(), _>(StoreError::Other("boom".into()))
        });
        assert!(err.is_err());
        let n: i64 = db
            .read(|c| {
                c.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
                    .map_err(StoreError::Db)
            })
            .unwrap();
        assert_eq!(n, 0, "rolled back insert must not persist");
    }
}
