//! Semantic memory records (decision 0012): durable, inspectable notes
//! with provenance.
//!
//! This module is the RECORD of memory — what was remembered, by whom,
//! when. The semantic part (embedding + similarity search) lives in the
//! knowledge layer, which indexes each record as a `memory:<id>` source
//! that ordinary document retrieval can never see. Provenance is the
//! point: nanoMuse declares that its "memory does not yet know who wrote
//! it"; here every record carries its origin (user, skill run, or goal
//! execution) so an agent-written note is always distinguishable from
//! something the user said, and deletable on its own.
//!
//! Records are private workspace content (policy 13): the whole store is
//! AEAD-sealed at rest under a workspace-derived subkey, like goals.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Who wrote a memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum Provenance {
    /// The user typed it into Memory.
    User,
    /// A skill run wrote it (the run id is the audit trail).
    Run { run_id: String, skill_id: String },
    /// A scheduled goal execution wrote it.
    Goal { goal_id: String, run_id: String },
}

impl Provenance {
    fn validate(&self) -> Result<(), MemoryError> {
        let fields: &[&str] = match self {
            Provenance::User => &[],
            Provenance::Run { run_id, skill_id } => &[run_id, skill_id],
            Provenance::Goal { goal_id, run_id } => &[goal_id, run_id],
        };
        if fields.iter().any(|f| f.trim().is_empty()) {
            return Err(MemoryError::Invalid(
                "provenance ids must be non-empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub id: String,
    pub text: String,
    pub provenance: Provenance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("memory not found: {0}")]
    NotFound(String),
    #[error("invalid memory: {0}")]
    Invalid(String),
    #[error("store: {0}")]
    Store(String),
}

pub const MAX_MEMORY_CHARS: usize = 4_000;
pub const MAX_MEMORIES: usize = 5_000;

/// The knowledge-index source id for a memory record.
pub fn source_id(id: &str) -> String {
    format!("memory:{id}")
}

const SEALED_PREFIX: &str = "enc.v1:";
const MEMORY_AAD: &[u8] = b"harbor.agent.memory/v1";

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MemoryFile {
    memories: Vec<MemoryRecord>,
}

/// Durable memory store: one sealed file, atomic tmp+rename per
/// mutation (the goal-store pattern).
pub struct MemoryStore {
    path: PathBuf,
    key: Option<harbor_store::keys::KeyMaterial>,
}

impl MemoryStore {
    /// Plaintext store — tests and tools only.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, MemoryError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MemoryError::Store(format!("{}: {e}", parent.display())))?;
        }
        Ok(MemoryStore { path, key: None })
    }

    /// Sealed store: records sit on disk only as AEAD ciphertext.
    pub fn open_with_key(
        path: impl Into<PathBuf>,
        key: harbor_store::keys::KeyMaterial,
    ) -> Result<Self, MemoryError> {
        let mut store = Self::open(path)?;
        store.key = Some(key);
        Ok(store)
    }

    fn read(&self) -> Result<MemoryFile, MemoryError> {
        if !self.path.exists() {
            return Ok(MemoryFile::default());
        }
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| MemoryError::Store(format!("{}: {e}", self.path.display())))?;
        let plain: Vec<u8> = if let Some(body) = text.strip_prefix(SEALED_PREFIX) {
            let sealed = hex_decode(body)
                .ok_or_else(|| MemoryError::Store("corrupt memory store: bad hex".into()))?;
            if sealed.len() < 12 {
                return Err(MemoryError::Store(
                    "corrupt memory store: short nonce".into(),
                ));
            }
            let (nonce, ct) = sealed.split_at(12);
            let nonce: [u8; 12] = nonce.try_into().unwrap();
            let key = self.key.as_ref().ok_or_else(|| {
                MemoryError::Store("sealed memory store opened without a key".into())
            })?;
            harbor_store::keys::aead_open(key, &nonce, ct, MEMORY_AAD).map_err(|_| {
                MemoryError::Store("sealed memory store failed to open (wrong key?)".into())
            })?
        } else if self.key.is_some() {
            return Err(MemoryError::Store(
                "plaintext memory store opened with a key; refusing to mix".into(),
            ));
        } else {
            text.into_bytes()
        };
        serde_json::from_slice(&plain)
            .map_err(|e| MemoryError::Store(format!("corrupt memory store: {e}")))
    }

    fn write(&self, file: &MemoryFile) -> Result<(), MemoryError> {
        use rand::RngCore;
        let tmp = self.path.with_extension("json.tmp");
        let plain =
            serde_json::to_vec(file).map_err(|e| MemoryError::Store(format!("serialize: {e}")))?;
        let bytes = match &self.key {
            None => plain,
            Some(key) => {
                let mut nonce = [0u8; 12];
                rand::rngs::OsRng.fill_bytes(&mut nonce);
                let ct = harbor_store::keys::aead_seal(key, &nonce, &plain, MEMORY_AAD)
                    .map_err(|_| MemoryError::Store("seal failed".into()))?;
                format!("{SEALED_PREFIX}{}{}", hex_encode(&nonce), hex_encode(&ct)).into_bytes()
            }
        };
        std::fs::write(&tmp, bytes)
            .map_err(|e| MemoryError::Store(format!("{}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| MemoryError::Store(format!("{}: {e}", self.path.display())))?;
        Ok(())
    }

    pub fn add(&self, record: MemoryRecord) -> Result<MemoryRecord, MemoryError> {
        if record.id.trim().is_empty() {
            return Err(MemoryError::Invalid("id is required".into()));
        }
        let text = record.text.trim();
        if text.is_empty() {
            return Err(MemoryError::Invalid("text is required".into()));
        }
        if text.chars().count() > MAX_MEMORY_CHARS {
            return Err(MemoryError::Invalid(format!(
                "text exceeds {MAX_MEMORY_CHARS} characters"
            )));
        }
        record.provenance.validate()?;
        let mut file = self.read()?;
        if file.memories.iter().any(|m| m.id == record.id) {
            return Err(MemoryError::Invalid(format!("memory {} exists", record.id)));
        }
        if file.memories.len() >= MAX_MEMORIES {
            return Err(MemoryError::Invalid(format!(
                "memory is full ({MAX_MEMORIES} records); delete some first"
            )));
        }
        let stored = MemoryRecord {
            text: text.to_string(),
            ..record
        };
        file.memories.push(stored.clone());
        self.write(&file)?;
        Ok(stored)
    }

    /// All records, oldest first.
    pub fn list(&self) -> Result<Vec<MemoryRecord>, MemoryError> {
        Ok(self.read()?.memories)
    }

    pub fn get(&self, id: &str) -> Result<Option<MemoryRecord>, MemoryError> {
        Ok(self.read()?.memories.into_iter().find(|m| m.id == id))
    }

    pub fn delete(&self, id: &str) -> Result<(), MemoryError> {
        let mut file = self.read()?;
        let before = file.memories.len();
        file.memories.retain(|m| m.id != id);
        if file.memories.len() == before {
            return Err(MemoryError::NotFound(id.to_string()));
        }
        self.write(&file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, text: &str, provenance: Provenance) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            text: text.into(),
            provenance,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn add_list_get_delete_roundtrip_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.json");
        {
            let s = MemoryStore::open(&path).unwrap();
            s.add(rec("m1", "  prefers metric units ", Provenance::User))
                .unwrap();
            s.add(rec(
                "m2",
                "client is Acme",
                Provenance::Run {
                    run_id: "run-1".into(),
                    skill_id: "email-drafting".into(),
                },
            ))
            .unwrap();
        }
        let s = MemoryStore::open(&path).unwrap();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].text, "prefers metric units", "text is trimmed");
        assert!(matches!(
            s.get("m2").unwrap().unwrap().provenance,
            Provenance::Run { .. }
        ));
        s.delete("m1").unwrap();
        assert_eq!(s.list().unwrap().len(), 1);
        assert!(matches!(s.delete("m1"), Err(MemoryError::NotFound(_))));
    }

    #[test]
    fn rejects_blank_oversized_duplicate_and_unattributed() {
        let dir = tempfile::tempdir().unwrap();
        let s = MemoryStore::open(dir.path().join("m.json")).unwrap();
        assert!(s.add(rec("a", "   ", Provenance::User)).is_err());
        let big = "x".repeat(MAX_MEMORY_CHARS + 1);
        assert!(s.add(rec("b", &big, Provenance::User)).is_err());
        s.add(rec("c", "ok", Provenance::User)).unwrap();
        assert!(s.add(rec("c", "again", Provenance::User)).is_err());
        // A run-written memory with an empty run id has no audit trail.
        let anon = Provenance::Run {
            run_id: " ".into(),
            skill_id: "s".into(),
        };
        assert!(s.add(rec("d", "ghost", anon)).is_err());
    }

    #[test]
    fn sealed_store_leaves_no_plaintext_and_refuses_mixing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.json");
        let key = || harbor_store::keys::KeyMaterial(*b"0123456789abcdef0123456789abcdef");
        let s = MemoryStore::open_with_key(&path, key()).unwrap();
        s.add(rec("m1", "the vault code is swordfish", Provenance::User))
            .unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with(SEALED_PREFIX));
        assert!(!raw.contains("swordfish"));
        // Re-open with the key reads it back; without a key it refuses.
        let again = MemoryStore::open_with_key(&path, key()).unwrap();
        assert_eq!(again.list().unwrap().len(), 1);
        assert!(MemoryStore::open(&path).unwrap().list().is_err());
    }
}
