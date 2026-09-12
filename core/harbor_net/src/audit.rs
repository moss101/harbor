//! Append-only, hash-chained network audit log. Distinguishes blocked
//! attempts, dispatched traffic and completed traffic. Privacy-safe:
//! entries record origins and outcomes, never payloads.

use rusqlite::Connection;

use harbor_canonical::sha256_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkEventKind {
    /// An egress request was attempted and denied by policy.
    Blocked,
    /// Authorized request handed to the transport.
    Dispatched,
    /// Transport returned a response (success or transport error).
    Completed,
    /// Redirect hop was denied by re-authorization.
    RedirectBlocked,
}

impl NetworkEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NetworkEventKind::Blocked => "blocked",
            NetworkEventKind::Dispatched => "dispatched",
            NetworkEventKind::Completed => "completed",
            NetworkEventKind::RedirectBlocked => "redirect_blocked",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkAuditEntry {
    pub seq: u64,
    pub kind: NetworkEventKind,
    pub egress_class: String,
    pub origin: String,
    pub method: String,
    pub path: String,
    pub status: Option<u16>,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub detail: String,
    pub at_rfc3339: String,
    pub prev_entry_hash: Option<String>,
    pub entry_hash: String,
}

/// Sink for audit entries: in-memory ring plus optional SQLite persistence.
pub trait AuditSink: Send + Sync {
    fn append(&self, entry: NetworkAuditEntry);
    fn entries(&self) -> Vec<NetworkAuditEntry>;
}

/// SQLite-backed append-only audit sink with per-entry hash chaining.
pub struct SqliteAuditSink {
    conn: std::sync::Mutex<Connection>,
}

impl SqliteAuditSink {
    pub fn open_in_memory() -> Result<Self, harbor_store::StoreError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::init(conn)
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, harbor_store::StoreError> {
        let conn = rusqlite::Connection::open(path)?;
        Self::init(conn)
    }

    fn init(mut conn: Connection) -> Result<Self, harbor_store::StoreError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS network_audit (
                seq INTEGER PRIMARY KEY,
                kind TEXT NOT NULL,
                egress_class TEXT NOT NULL,
                origin TEXT NOT NULL,
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                status INTEGER,
                session_id TEXT,
                run_id TEXT,
                detail TEXT NOT NULL,
                at TEXT NOT NULL,
                prev_entry_hash TEXT,
                entry_hash TEXT NOT NULL
            );",
        )?;
        Ok(SqliteAuditSink { conn: std::sync::Mutex::new(conn) })
    }
}

impl AuditSink for SqliteAuditSink {
    fn append(&self, mut entry: NetworkAuditEntry) {
        let conn = self.conn.lock().unwrap();
        // Chain on the last persisted entry.
        let (seq, prev): (i64, Option<String>) = conn
            .query_row(
                "SELECT seq, entry_hash FROM network_audit ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0).unwrap(), r.get(1).unwrap())),
            )
            .unwrap_or((-1, None));
        entry.seq = (seq + 1) as u64;
        entry.prev_entry_hash = prev.filter(|_| seq >= 0);
        entry.entry_hash = hash_entry(&entry);
        let _ = conn.execute(
            "INSERT INTO network_audit (seq, kind, egress_class, origin, method, path, status, session_id, run_id, detail, at, prev_entry_hash, entry_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            rusqlite::params![
                entry.seq as i64,
                entry.kind.as_str(),
                entry.egress_class,
                entry.origin,
                entry.method,
                entry.path,
                entry.status,
                entry.session_id,
                entry.run_id,
                entry.detail,
                entry.at_rfc3339,
                entry.prev_entry_hash,
                entry.entry_hash
            ],
        );
    }

    fn entries(&self) -> Vec<NetworkAuditEntry> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT seq, kind, egress_class, origin, method, path, status, session_id, run_id, detail, at, prev_entry_hash, entry_hash FROM network_audit ORDER BY seq ASC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |r| {
            Ok(NetworkAuditEntry {
                seq: r.get::<_, i64>(0).unwrap_or(0) as u64,
                kind: match r.get::<_, String>(1).unwrap_or_default().as_str() {
                    "dispatched" => NetworkEventKind::Dispatched,
                    "completed" => NetworkEventKind::Completed,
                    "redirect_blocked" => NetworkEventKind::RedirectBlocked,
                    _ => NetworkEventKind::Blocked,
                },
                egress_class: r.get(2).unwrap_or_default(),
                origin: r.get(3).unwrap_or_default(),
                method: r.get(4).unwrap_or_default(),
                path: r.get(5).unwrap_or_default(),
                status: r.get(6).unwrap_or(None),
                session_id: r.get(7).unwrap_or(None),
                run_id: r.get(8).unwrap_or(None),
                detail: r.get(9).unwrap_or_default(),
                at_rfc3339: r.get(10).unwrap_or_default(),
                prev_entry_hash: r.get(11).unwrap_or(None),
                entry_hash: r.get(12).unwrap_or_default(),
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}

fn opt_string(s: &Option<String>) -> String {
    s.clone().unwrap_or_default()
}

fn hash_entry(e: &NetworkAuditEntry) -> String {
    let canonical = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        e.seq,
        e.kind.as_str(),
        e.egress_class,
        e.origin,
        e.method,
        e.path,
        e.status.map(|s| s.to_string()).unwrap_or_default(),
        opt_string(&e.session_id),
        opt_string(&e.run_id),
        e.detail,
        e.at_rfc3339,
        e.prev_entry_hash.clone().unwrap_or_else(|| "genesis".into())
    );
    sha256_hex(canonical.as_bytes())
}

/// Verify the audit chain end-to-end.
pub fn verify_chain(entries: &[NetworkAuditEntry]) -> bool {
    let mut prev_hash: Option<String> = None;
    for (i, e) in entries.iter().enumerate() {
        if e.seq != i as u64 {
            return false;
        }
        if e.prev_entry_hash != prev_hash {
            return false;
        }
        if hash_entry(e) != e.entry_hash {
            return false;
        }
        prev_hash = Some(e.entry_hash.clone());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: NetworkEventKind) -> NetworkAuditEntry {
        NetworkAuditEntry {
            seq: 0,
            kind,
            egress_class: "weight_transfer".to_string(),
            origin: "huggingface.co".into(),
            method: "GET".into(),
            path: "/repo/model.gguf".into(),
            status: None,
            session_id: Some("sess-1".into()),
            run_id: None,
            detail: String::new(),
            at_rfc3339: "2026-09-12T00:00:00Z".into(),
            prev_entry_hash: None,
            entry_hash: String::new(),
        }
    }

    #[test]
    fn chain_is_append_only_and_verifiable() {
        let sink = SqliteAuditSink::open_in_memory().unwrap();
        sink.append(entry(NetworkEventKind::Blocked));
        sink.append(entry(NetworkEventKind::Dispatched));
        sink.append(entry(NetworkEventKind::Completed));
        let all = sink.entries();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].kind, NetworkEventKind::Blocked);
        assert!(verify_chain(&all));
        // Tampering breaks verification.
        let mut tampered = all.clone();
        tampered[1].path = "/repo/other.gguf".into();
        assert!(!verify_chain(&tampered));
    }
}
