//! Durable Ask conversation threads over the FFI boundary.
//!
//! Threads persist in `<data_root>/db/threads.db`. Titles and message
//! bodies (question/answer text plus citation metadata) are private
//! workspace content, so they are sealed with ChaCha20-Poly1305 under a
//! workspace-derived key (`harbor_store::kcipher::ask_thread_key`) with
//! AAD binding each value to its thread, sequence number and field —
//! at rest the database holds only ids, roles, kinds, statuses and
//! timestamps (policy 13).
//!
//! Only COMPLETED turns are recorded, and a turn's question and answer
//! are written in one transaction: a cancelled or failed generation, or a
//! crash mid-generation, leaves nothing behind ("Nothing was recorded for
//! this question"), so a thread never holds a dangling question.
//!
//! Citations are stored WITHOUT their chunk text (`_text`): a thread keeps
//! title/score/state/hash (what the UI shows), never a second copy of
//! source content. Retrieval evidence for a follow-up is always fetched
//! fresh against the current index, so a removed or changed source is
//! never replayed from chat history as current evidence.
//!
//! The pure conversation-memory helpers (`bound_history`,
//! `retrieval_query`) live here, independent of the GGUF runtime, so the
//! policy is unit-testable in lean builds.

use std::path::{Path, PathBuf};

use harbor_store::keys::KeyMaterial;
use rusqlite::{params, Connection, OptionalExtension};

/// Longest accepted title, in characters.
pub const MAX_TITLE_CHARS: usize = 80;
/// Longest accepted message text, in bytes (a bound, not a quota: a
/// single answer is token-limited far below this).
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Conversation-memory budget for the prompt (see [`bound_history`]).
pub const HISTORY_MAX_TURNS: usize = 4;
pub const HISTORY_QUESTION_CHARS: usize = 300;
pub const HISTORY_ANSWER_CHARS: usize = 500;
pub const HISTORY_TOTAL_CHARS: usize = 2000;
/// A follow-up this short is assumed to lean on the previous question.
pub const FOLLOW_UP_MAX_WORDS: usize = 6;

#[derive(Debug, thiserror::Error)]
pub enum ThreadError {
    #[error("db: {0}")]
    Db(String),
    #[error("crypto: sealed thread value failed to open (wrong key or corrupted row)")]
    Crypto,
    #[error("thread not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
}

impl From<rusqlite::Error> for ThreadError {
    fn from(e: rusqlite::Error) -> Self {
        ThreadError::Db(e.to_string())
    }
}

/// One conversation as listed (title already unsealed).
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    /// Question/answer turns (a turn is two messages).
    pub turns: u64,
}

/// A message as persisted (already unsealed).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredMessage {
    pub seq: u64,
    /// `user` | `assistant`
    pub role: String,
    /// `question` | `answer` | `search`
    pub kind: String,
    pub text: String,
    /// Citations, usage, run id, model — whatever the assistant turn
    /// carried (never chunk text).
    pub meta: serde_json::Value,
    pub created_at: String,
}

/// One earlier question/answer pair offered to the model as context.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryTurn {
    pub question: String,
    pub answer: String,
}

/// One conversation affected by a source purge (the preview).
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedThread {
    pub id: String,
    pub title: String,
    /// Turns in this thread whose citations name the source.
    pub turns: u64,
    /// Whether the purge would leave the thread empty (and so delete it).
    pub becomes_empty: bool,
}

/// What a purge removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgeReport {
    pub turns_removed: u64,
    pub threads_deleted: u64,
}

pub struct ThreadStore {
    db_path: PathBuf,
    key: KeyMaterial,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn aad(thread_id: &str, seq: i64, field: &str) -> Vec<u8> {
    format!("harbor.ask/1\x1f{thread_id}\x1f{seq}\x1f{field}").into_bytes()
}

/// Collapse whitespace and cap to [MAX_TITLE_CHARS] on a char boundary.
fn clean_title(raw: &str) -> String {
    let one_line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(MAX_TITLE_CHARS).collect()
}

impl ThreadStore {
    pub fn open(db_path: &Path, key: KeyMaterial) -> Result<Self, ThreadError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ThreadError::Db(e.to_string()))?;
        }
        let store = ThreadStore {
            db_path: db_path.to_path_buf(),
            key,
        };
        let conn = store.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ask_threads (
                id TEXT PRIMARY KEY,
                title_sealed BLOB NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ask_messages (
                thread_id TEXT NOT NULL REFERENCES ask_threads(id) ON DELETE CASCADE,
                seq INTEGER NOT NULL,
                role TEXT NOT NULL,
                kind TEXT NOT NULL,
                body_sealed BLOB NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (thread_id, seq)
            );
            CREATE INDEX IF NOT EXISTS ask_threads_updated ON ask_threads(updated_at);",
        )?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, ThreadError> {
        let conn = Connection::open(&self.db_path)?;
        // Background generation ops and the UI thread each open their own
        // connection; WAL + a busy timeout keeps them from tripping.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // Deleted conversations must not linger in free pages.
        conn.pragma_update(None, "secure_delete", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }

    fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, ThreadError> {
        harbor_store::seal_bound(&self.key, aad, plaintext).map_err(|_| ThreadError::Crypto)
    }

    fn open_sealed(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, ThreadError> {
        harbor_store::open_bound(&self.key, aad, sealed).map_err(|_| ThreadError::Crypto)
    }

    fn open_title(&self, id: &str, sealed: &[u8]) -> Result<String, ThreadError> {
        let pt = self.open_sealed(&aad(id, 0, "title"), sealed)?;
        String::from_utf8(pt).map_err(|_| ThreadError::Crypto)
    }

    /// Create an empty thread. An empty title is filled from the first
    /// question recorded.
    pub fn create_thread(&self, id: &str, title: &str) -> Result<ThreadSummary, ThreadError> {
        if id.is_empty() {
            return Err(ThreadError::Invalid("empty thread id".into()));
        }
        let title = clean_title(title);
        let at = now();
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO ask_threads (id, title_sealed, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![id, self.seal(&aad(id, 0, "title"), title.as_bytes())?, at],
        )?;
        Ok(ThreadSummary {
            id: id.to_string(),
            title,
            created_at: at.clone(),
            updated_at: at,
            turns: 0,
        })
    }

    pub fn exists(&self, id: &str) -> Result<bool, ThreadError> {
        let conn = self.connect()?;
        Ok(conn
            .query_row("SELECT 1 FROM ask_threads WHERE id = ?1", [id], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// Newest-first listing.
    pub fn list_threads(&self) -> Result<Vec<ThreadSummary>, ThreadError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT t.id, t.title_sealed, t.created_at, t.updated_at,
                    (SELECT COUNT(*) FROM ask_messages m
                      WHERE m.thread_id = t.id AND m.role = 'user')
             FROM ask_threads t ORDER BY t.updated_at DESC, t.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, sealed, created_at, updated_at, turns) = row?;
            out.push(ThreadSummary {
                title: self.open_title(&id, &sealed)?,
                id,
                created_at,
                updated_at,
                turns: turns as u64,
            });
        }
        Ok(out)
    }

    pub fn get_thread(&self, id: &str) -> Result<ThreadSummary, ThreadError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT t.title_sealed, t.created_at, t.updated_at,
                        (SELECT COUNT(*) FROM ask_messages m
                          WHERE m.thread_id = t.id AND m.role = 'user')
                 FROM ask_threads t WHERE t.id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| ThreadError::NotFound(id.to_string()))?;
        Ok(ThreadSummary {
            title: self.open_title(id, &row.0)?,
            id: id.to_string(),
            created_at: row.1,
            updated_at: row.2,
            turns: row.3 as u64,
        })
    }

    pub fn rename_thread(&self, id: &str, title: &str) -> Result<(), ThreadError> {
        let title = clean_title(title);
        if title.is_empty() {
            return Err(ThreadError::Invalid("empty title".into()));
        }
        let conn = self.connect()?;
        let n = conn.execute(
            "UPDATE ask_threads SET title_sealed = ?2 WHERE id = ?1",
            params![id, self.seal(&aad(id, 0, "title"), title.as_bytes())?],
        )?;
        if n == 0 {
            return Err(ThreadError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Delete one thread and all its messages. Returns whether it existed.
    pub fn delete_thread(&self, id: &str) -> Result<bool, ThreadError> {
        let conn = self.connect()?;
        Ok(conn.execute("DELETE FROM ask_threads WHERE id = ?1", [id])? > 0)
    }

    /// Delete every thread. Returns how many existed.
    pub fn delete_all(&self) -> Result<u64, ThreadError> {
        let conn = self.connect()?;
        Ok(conn.execute("DELETE FROM ask_threads", [])? as u64)
    }

    /// Record one completed turn — the question and its answer (or the
    /// retrieval-only evidence) — atomically. Returns the sequence number
    /// of the question. `meta` rides with the assistant message.
    pub fn append_turn(
        &self,
        thread_id: &str,
        question: &str,
        answer_kind: &str,
        answer: &str,
        meta: &serde_json::Value,
    ) -> Result<u64, ThreadError> {
        if question.trim().is_empty() {
            return Err(ThreadError::Invalid("empty question".into()));
        }
        if answer_kind != "answer" && answer_kind != "search" {
            return Err(ThreadError::Invalid(format!(
                "unknown turn kind {answer_kind}"
            )));
        }
        if question.len() > MAX_MESSAGE_BYTES || answer.len() > MAX_MESSAGE_BYTES {
            return Err(ThreadError::Invalid("message too large".into()));
        }
        let meta = strip_chunk_text(meta);
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let title_sealed: Vec<u8> = tx
            .query_row(
                "SELECT title_sealed FROM ask_threads WHERE id = ?1",
                [thread_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| ThreadError::NotFound(thread_id.to_string()))?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq) + 1, 0) FROM ask_messages WHERE thread_id = ?1",
            [thread_id],
            |r| r.get(0),
        )?;
        let at = now();
        let put = |seq: i64, role: &str, kind: &str, text: &str, meta: &serde_json::Value| {
            let body = serde_json::json!({ "text": text, "meta": meta });
            let sealed = self.seal(&aad(thread_id, seq, "body"), body.to_string().as_bytes())?;
            tx.execute(
                "INSERT INTO ask_messages (thread_id, seq, role, kind, body_sealed, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![thread_id, seq, role, kind, sealed, at],
            )?;
            Ok::<(), ThreadError>(())
        };
        put(next, "user", "question", question, &serde_json::json!({}))?;
        put(next + 1, "assistant", answer_kind, answer, &meta)?;
        // First question names an untitled thread.
        if self.open_title(thread_id, &title_sealed)?.is_empty() {
            tx.execute(
                "UPDATE ask_threads SET title_sealed = ?2 WHERE id = ?1",
                params![
                    thread_id,
                    self.seal(
                        &aad(thread_id, 0, "title"),
                        clean_title(question).as_bytes()
                    )?
                ],
            )?;
        }
        tx.execute(
            "UPDATE ask_threads SET updated_at = ?2 WHERE id = ?1",
            params![thread_id, at],
        )?;
        tx.commit()?;
        Ok(next as u64)
    }

    /// Every message of a thread in order.
    pub fn messages(&self, thread_id: &str) -> Result<Vec<StoredMessage>, ThreadError> {
        self.load_messages(thread_id, None)
    }

    fn load_messages(
        &self,
        thread_id: &str,
        last: Option<usize>,
    ) -> Result<Vec<StoredMessage>, ThreadError> {
        if !self.exists(thread_id)? {
            return Err(ThreadError::NotFound(thread_id.to_string()));
        }
        let conn = self.connect()?;
        // Newest-first with an optional cap, restored to order below.
        let limit = last.map(|n| n as i64).unwrap_or(-1);
        let mut stmt = conn.prepare(
            "SELECT seq, role, kind, body_sealed, created_at FROM ask_messages
             WHERE thread_id = ?1 ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread_id, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, role, kind, sealed, created_at) = row?;
            let pt = self.open_sealed(&aad(thread_id, seq, "body"), &sealed)?;
            let body: serde_json::Value =
                serde_json::from_slice(&pt).map_err(|_| ThreadError::Crypto)?;
            out.push(StoredMessage {
                seq: seq as u64,
                role,
                kind,
                text: body
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                meta: body.get("meta").cloned().unwrap_or(serde_json::json!({})),
                created_at,
            });
        }
        out.reverse();
        Ok(out)
    }

    /// Answer messages (by sequence) whose citations name [source_id],
    /// grouped per thread. A turn is a question at `seq - 1` plus its
    /// answer at `seq`. Citations record what the answer was derived
    /// from, so any turn citing the source is content derived from it.
    fn turns_citing(&self, source_id: &str) -> Result<Vec<(String, Vec<i64>)>, ThreadError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT thread_id, seq, body_sealed FROM ask_messages
             WHERE role = 'assistant' ORDER BY thread_id, seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let mut out: Vec<(String, Vec<i64>)> = Vec::new();
        for row in rows {
            let (thread_id, seq, sealed) = row?;
            let pt = self.open_sealed(&aad(&thread_id, seq, "body"), &sealed)?;
            let body: serde_json::Value =
                serde_json::from_slice(&pt).map_err(|_| ThreadError::Crypto)?;
            let cites = body
                .pointer("/meta/citations")
                .and_then(|c| c.as_array())
                .map(|a| {
                    a.iter()
                        .any(|c| c.get("source_id").and_then(|v| v.as_str()) == Some(source_id))
                })
                .unwrap_or(false);
            if !cites {
                continue;
            }
            match out.last_mut() {
                Some((t, seqs)) if *t == thread_id => seqs.push(seq),
                _ => out.push((thread_id, vec![seq])),
            }
        }
        Ok(out)
    }

    /// Preview of a purge: the conversations holding turns derived from
    /// [source_id]. Reads only; nothing changes.
    pub fn derived_from_source(&self, source_id: &str) -> Result<Vec<DerivedThread>, ThreadError> {
        let conn = self.connect()?;
        let mut out = Vec::new();
        for (thread_id, seqs) in self.turns_citing(source_id)? {
            let title_sealed: Vec<u8> = conn.query_row(
                "SELECT title_sealed FROM ask_threads WHERE id = ?1",
                [&thread_id],
                |r| r.get(0),
            )?;
            let total: i64 = conn.query_row(
                "SELECT COUNT(*) FROM ask_messages WHERE thread_id = ?1",
                [&thread_id],
                |r| r.get(0),
            )?;
            out.push(DerivedThread {
                title: self.open_title(&thread_id, &title_sealed)?,
                turns: seqs.len() as u64,
                becomes_empty: total as usize == seqs.len() * 2,
                id: thread_id,
            });
        }
        Ok(out)
    }

    /// Delete every turn (question and answer) derived from [source_id];
    /// a thread left with no turns is deleted. Atomic. This is the
    /// explicit purge-derived-content cascade over chat copies
    /// (`16_Index_and_Evaluation_Contract.md`); removing a source alone
    /// never calls it.
    pub fn purge_source(&self, source_id: &str) -> Result<PurgeReport, ThreadError> {
        let affected = self.turns_citing(source_id)?;
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut report = PurgeReport {
            turns_removed: 0,
            threads_deleted: 0,
        };
        for (thread_id, seqs) in affected {
            for seq in &seqs {
                // The answer and the question just before it.
                tx.execute(
                    "DELETE FROM ask_messages WHERE thread_id = ?1 AND seq IN (?2, ?3)",
                    params![thread_id, seq, seq - 1],
                )?;
                report.turns_removed += 1;
            }
            let left: i64 = tx.query_row(
                "SELECT COUNT(*) FROM ask_messages WHERE thread_id = ?1",
                [&thread_id],
                |r| r.get(0),
            )?;
            if left == 0 {
                tx.execute("DELETE FROM ask_threads WHERE id = ?1", [&thread_id])?;
                report.threads_deleted += 1;
            }
        }
        tx.commit()?;
        Ok(report)
    }

    /// The thread's earlier generated question/answer pairs, oldest first,
    /// bounded for the prompt. Retrieval-only turns carry no model answer
    /// and are skipped.
    pub fn history(&self, thread_id: &str) -> Result<Vec<HistoryTurn>, ThreadError> {
        // Enough tail to hold HISTORY_MAX_TURNS answered turns even when
        // search-only turns are interleaved.
        let tail = self.load_messages(thread_id, Some(HISTORY_MAX_TURNS * 8))?;
        let mut turns = Vec::new();
        let mut pending: Option<String> = None;
        for m in tail {
            match (m.role.as_str(), m.kind.as_str()) {
                ("user", _) => pending = Some(m.text),
                ("assistant", "answer") => {
                    if let Some(q) = pending.take() {
                        turns.push(HistoryTurn {
                            question: q,
                            answer: m.text,
                        });
                    }
                }
                _ => pending = None,
            }
        }
        Ok(bound_history(&turns))
    }
}

/// Drop `_text` (source chunk content) from every citation in `meta`.
fn strip_chunk_text(meta: &serde_json::Value) -> serde_json::Value {
    let mut meta = meta.clone();
    if let Some(cites) = meta.get_mut("citations").and_then(|c| c.as_array_mut()) {
        for c in cites {
            if let Some(o) = c.as_object_mut() {
                o.remove("_text");
            }
        }
    }
    meta
}

fn cap_chars(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// What the model is shown for an earlier abstention instead of the
/// literal sentinel it was told to emit (copying the sentinel forward
/// would make every later answer abstain).
const PRIOR_ABSTENTION: &str = "I could not find support for this in the knowledge sources.";

/// Bound earlier turns to the prompt budget: at most
/// [HISTORY_MAX_TURNS] newest turns, each question/answer capped, the
/// whole within [HISTORY_TOTAL_CHARS] (oldest dropped first). Input and
/// output are oldest-first.
pub fn bound_history(turns: &[HistoryTurn]) -> Vec<HistoryTurn> {
    let mut kept: Vec<HistoryTurn> = Vec::new();
    let mut used = 0usize;
    for t in turns.iter().rev().take(HISTORY_MAX_TURNS) {
        let answer = if t.answer.contains("INSUFFICIENT_EVIDENCE") {
            PRIOR_ABSTENTION.to_string()
        } else {
            cap_chars(&t.answer, HISTORY_ANSWER_CHARS)
        };
        let turn = HistoryTurn {
            question: cap_chars(&t.question, HISTORY_QUESTION_CHARS),
            answer,
        };
        let cost = turn.question.chars().count() + turn.answer.chars().count();
        // The newest turn is always kept (a follow-up needs at least it).
        if !kept.is_empty() && used + cost > HISTORY_TOTAL_CHARS {
            break;
        }
        used += cost;
        kept.push(turn);
    }
    kept.reverse();
    kept
}

/// The text embedded for retrieval. A short follow-up ("and when does it
/// end?") cannot be retrieved on its own, so it borrows the previous
/// question; a self-contained question is used verbatim. Deterministic by
/// design: no extra on-device generation pass per question.
pub fn retrieval_query(question: &str, history: &[HistoryTurn]) -> String {
    match history.last() {
        Some(prev) if question.split_whitespace().count() <= FOLLOW_UP_MAX_WORDS => {
            format!("{} {}", prev.question, question.trim())
        }
        _ => question.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ThreadStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = ThreadStore::open(
            &dir.path().join("db").join("threads.db"),
            KeyMaterial::random(),
        )
        .unwrap();
        (dir, s)
    }

    fn cites() -> serde_json::Value {
        serde_json::json!({
            "citations": [{
                "source_id": "s1", "title": "MSA", "chunk_id": "s1-0",
                "score": 0.9, "state": "Current", "content_hash": "h1",
                "_text": "SECRET CHUNK TEXT"
            }],
            "used_citations": true,
        })
    }

    #[test]
    fn turns_roundtrip_in_order_and_title_comes_from_first_question() {
        let (_d, s) = store();
        s.create_thread("t1", "").unwrap();
        assert_eq!(
            s.append_turn("t1", "What is the value?", "answer", "5000 USD", &cites())
                .unwrap(),
            0
        );
        assert_eq!(
            s.append_turn("t1", "And the end date?", "answer", "2026-12-31", &cites())
                .unwrap(),
            2
        );
        let t = s.get_thread("t1").unwrap();
        assert_eq!(t.title, "What is the value?");
        assert_eq!(t.turns, 2);
        let m = s.messages("t1").unwrap();
        assert_eq!(
            m.iter().map(|m| m.seq).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(m[0].role, "user");
        assert_eq!(m[1].text, "5000 USD");
        assert_eq!(m[1].meta["citations"][0]["title"], "MSA");
    }

    #[test]
    fn chunk_text_is_never_persisted() {
        let (d, s) = store();
        s.create_thread("t1", "T").unwrap();
        s.append_turn("t1", "q", "answer", "a", &cites()).unwrap();
        let m = s.messages("t1").unwrap();
        assert!(m[1].meta["citations"][0].get("_text").is_none());
        // Nor anywhere on disk.
        let bytes = std::fs::read(d.path().join("db").join("threads.db")).unwrap();
        assert!(!bytes.windows(6).any(|w| w == b"SECRET"));
    }

    #[test]
    fn content_is_sealed_at_rest_and_bound_to_its_row() {
        let (d, s) = store();
        s.create_thread("t1", "Quarterly numbers").unwrap();
        s.append_turn(
            "t1",
            "sentinel-question-9f3a",
            "answer",
            "sentinel-answer-71cc",
            &cites(),
        )
        .unwrap();
        let conn = Connection::open(d.path().join("db").join("threads.db")).unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
        for f in std::fs::read_dir(d.path().join("db")).unwrap().flatten() {
            let bytes = std::fs::read(f.path()).unwrap();
            for needle in [
                "sentinel-question-9f3a",
                "sentinel-answer-71cc",
                "Quarterly numbers",
            ] {
                assert!(
                    !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
                    "plaintext {needle} in {:?}",
                    f.path()
                );
            }
        }
        // A row swapped to another seq (or thread) fails authentication.
        conn.execute(
            "UPDATE ask_messages SET seq = 9 WHERE thread_id = 't1' AND seq = 1",
            [],
        )
        .unwrap();
        assert!(matches!(s.messages("t1"), Err(ThreadError::Crypto)));
    }

    #[test]
    fn a_different_key_cannot_read_the_threads() {
        let (d, s) = store();
        s.create_thread("t1", "T").unwrap();
        let other = ThreadStore::open(
            &d.path().join("db").join("threads.db"),
            KeyMaterial::random(),
        )
        .unwrap();
        assert!(matches!(other.list_threads(), Err(ThreadError::Crypto)));
        drop(s);
    }

    #[test]
    fn list_is_newest_first_and_delete_cascades() {
        let (_d, s) = store();
        s.create_thread("a", "A").unwrap();
        s.create_thread("b", "B").unwrap();
        s.append_turn("a", "q", "answer", "x", &serde_json::json!({}))
            .unwrap();
        // `a` was touched last.
        let ids: Vec<_> = s
            .list_threads()
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert!(s.delete_thread("a").unwrap());
        assert!(!s.delete_thread("a").unwrap());
        assert!(matches!(s.messages("a"), Err(ThreadError::NotFound(_))));
        let conn = s.connect().unwrap();
        let orphans: i64 = conn
            .query_row("SELECT COUNT(*) FROM ask_messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphans, 0, "messages must cascade with their thread");
        assert_eq!(s.delete_all().unwrap(), 1);
        assert!(s.list_threads().unwrap().is_empty());
    }

    #[test]
    fn rename_and_unknown_thread() {
        let (_d, s) = store();
        s.create_thread("a", "A").unwrap();
        s.rename_thread("a", "  New   name \n here ").unwrap();
        assert_eq!(s.get_thread("a").unwrap().title, "New name here");
        assert!(matches!(
            s.rename_thread("a", "   "),
            Err(ThreadError::Invalid(_))
        ));
        assert!(matches!(
            s.rename_thread("zz", "x"),
            Err(ThreadError::NotFound(_))
        ));
        assert!(matches!(
            s.append_turn("zz", "q", "answer", "a", &serde_json::json!({})),
            Err(ThreadError::NotFound(_))
        ));
    }

    #[test]
    fn history_pairs_answered_turns_and_skips_search_only() {
        let (_d, s) = store();
        s.create_thread("t", "T").unwrap();
        let none = serde_json::json!({});
        s.append_turn("t", "q1", "answer", "a1", &none).unwrap();
        s.append_turn("t", "searched", "search", "", &none).unwrap();
        s.append_turn("t", "q2", "answer", "a2", &none).unwrap();
        let h = s.history("t").unwrap();
        assert_eq!(
            h,
            vec![
                HistoryTurn {
                    question: "q1".into(),
                    answer: "a1".into()
                },
                HistoryTurn {
                    question: "q2".into(),
                    answer: "a2".into()
                },
            ]
        );
    }

    #[test]
    fn bound_history_keeps_newest_within_budget_and_hides_the_sentinel() {
        let turn = |q: &str, a: &str| HistoryTurn {
            question: q.into(),
            answer: a.into(),
        };
        let many: Vec<_> = (0..10)
            .map(|i| turn(&format!("q{i}"), &format!("a{i}")))
            .collect();
        let b = bound_history(&many);
        assert_eq!(b.len(), HISTORY_MAX_TURNS);
        assert_eq!(b.last().unwrap().question, "q9");
        assert_eq!(b[0].question, "q6");

        let big = "x".repeat(5000);
        let b = bound_history(&[turn(&big, &big), turn(&big, &big), turn("q", "a")]);
        // Each capped; the newest always survives; total stays in budget.
        assert_eq!(b.last().unwrap().question, "q");
        let total: usize = b
            .iter()
            .map(|t| t.question.chars().count() + t.answer.chars().count())
            .sum();
        assert!(total <= HISTORY_TOTAL_CHARS + 2, "{total}");

        let only_big = bound_history(&[turn(&big, &big)]);
        assert_eq!(only_big.len(), 1, "a lone turn is kept, capped");
        assert!(only_big[0].answer.chars().count() <= HISTORY_ANSWER_CHARS + 1);

        let b = bound_history(&[turn("q", "INSUFFICIENT_EVIDENCE")]);
        assert!(!b[0].answer.contains("INSUFFICIENT_EVIDENCE"));
        // Arabic text is capped on char boundaries, not bytes.
        let ar = "سؤال ".repeat(400);
        let b = bound_history(&[turn(&ar, &ar)]);
        assert!(b[0].question.chars().count() <= HISTORY_QUESTION_CHARS + 1);
    }

    #[test]
    fn retrieval_query_borrows_the_previous_question_only_for_short_follow_ups() {
        let h = vec![HistoryTurn {
            question: "What is the contract value?".into(),
            answer: "5000".into(),
        }];
        assert_eq!(
            retrieval_query("and the end date?", &h),
            "What is the contract value? and the end date?"
        );
        let long = "when exactly does the agreement between the parties end";
        assert_eq!(retrieval_query(long, &h), long);
        assert_eq!(
            retrieval_query("and the end date?", &[]),
            "and the end date?"
        );
    }

    #[test]
    fn purge_removes_only_turns_derived_from_the_source_and_empty_threads() {
        let (_d, s) = store();
        let cite = |sid: &str| serde_json::json!({"citations": [{"source_id": sid, "title": sid}]});
        s.create_thread("mixed", "").unwrap();
        s.append_turn("mixed", "about a", "answer", "a1", &cite("a"))
            .unwrap();
        s.append_turn("mixed", "about b", "answer", "b1", &cite("b"))
            .unwrap();
        s.append_turn("mixed", "search a", "search", "", &cite("a"))
            .unwrap();
        s.create_thread("only_a", "").unwrap();
        s.append_turn("only_a", "again a", "answer", "a2", &cite("a"))
            .unwrap();
        s.create_thread("none", "").unwrap();
        s.append_turn("none", "nothing", "answer", "n", &serde_json::json!({}))
            .unwrap();

        // The preview names exactly what a purge would touch, and changes nothing.
        let mut preview = s.derived_from_source("a").unwrap();
        preview.sort_by(|x, y| x.id.cmp(&y.id));
        assert_eq!(preview.len(), 2);
        assert_eq!(
            (
                preview[0].id.as_str(),
                preview[0].turns,
                preview[0].becomes_empty
            ),
            ("mixed", 2, false)
        );
        assert_eq!(
            (
                preview[1].id.as_str(),
                preview[1].turns,
                preview[1].becomes_empty
            ),
            ("only_a", 1, true)
        );
        assert_eq!(s.messages("mixed").unwrap().len(), 6);
        assert!(s.derived_from_source("zzz").unwrap().is_empty());

        let report = s.purge_source("a").unwrap();
        assert_eq!(
            report,
            PurgeReport {
                turns_removed: 3,
                threads_deleted: 1
            }
        );
        // Unrelated turns survive intact, in order, and the thread still works.
        let m = s.messages("mixed").unwrap();
        assert_eq!(
            m.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            vec!["about b", "b1"]
        );
        assert!(matches!(
            s.messages("only_a"),
            Err(ThreadError::NotFound(_))
        ));
        assert_eq!(s.messages("none").unwrap().len(), 2);
        s.append_turn("mixed", "more", "answer", "m", &serde_json::json!({}))
            .unwrap();
        assert_eq!(s.history("mixed").unwrap().len(), 2);
        // Idempotent.
        assert_eq!(
            s.purge_source("a").unwrap(),
            PurgeReport {
                turns_removed: 0,
                threads_deleted: 0
            }
        );
    }
}
