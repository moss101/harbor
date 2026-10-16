//! Durable multi-skill workflow records (decision 0018).
//!
//! A workflow is an ordered list of ordinary skill runs. This store holds only
//! WHICH runs belong to which workflow and why it stopped; the work, the
//! approvals and the outputs stay in the runs themselves (their snapshots), so
//! a restart loses nothing and the workflow adds no second execution or policy
//! path. Sealed at rest like memory and goals.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const SEALED_PREFIX: &str = "enc.v1:";
const AAD: &[u8] = b"harbor.agent.workflows/v1";
pub const MAX_WORKFLOWS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRecord {
    pub id: String,
    pub template: String,
    pub chat_package: Option<String>,
    pub inputs: serde_json::Value,
    pub created_at: DateTime<Utc>,
    /// Run id of each step started so far (index = step).
    pub run_ids: Vec<String>,
    /// `running` | `done` | `stopped`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("workflow not found: {0}")]
    NotFound(String),
    #[error("invalid workflow: {0}")]
    Invalid(String),
    #[error("store: {0}")]
    Store(String),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    workflows: Vec<WorkflowRecord>,
}

pub struct WorkflowStore {
    path: PathBuf,
    key: Option<harbor_store::keys::KeyMaterial>,
}

impl WorkflowStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, WorkflowError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| WorkflowError::Store(format!("{}: {e}", parent.display())))?;
        }
        Ok(WorkflowStore { path, key: None })
    }

    pub fn open_with_key(
        path: impl Into<PathBuf>,
        key: harbor_store::keys::KeyMaterial,
    ) -> Result<Self, WorkflowError> {
        let mut s = Self::open(path)?;
        s.key = Some(key);
        Ok(s)
    }

    fn read(&self) -> Result<File, WorkflowError> {
        if !self.path.exists() {
            return Ok(File::default());
        }
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| WorkflowError::Store(format!("{}: {e}", self.path.display())))?;
        let plain: Vec<u8> = if let Some(body) = text.strip_prefix(SEALED_PREFIX) {
            let sealed = crate::memory::hex_decode(body)
                .ok_or_else(|| WorkflowError::Store("corrupt workflow store: bad hex".into()))?;
            if sealed.len() < 12 {
                return Err(WorkflowError::Store(
                    "corrupt workflow store: short nonce".into(),
                ));
            }
            let (nonce, ct) = sealed.split_at(12);
            let nonce: [u8; 12] = nonce.try_into().unwrap();
            let key = self
                .key
                .as_ref()
                .ok_or_else(|| WorkflowError::Store("sealed store opened without a key".into()))?;
            harbor_store::keys::aead_open(key, &nonce, ct, AAD)
                .map_err(|_| WorkflowError::Store("sealed store failed to open".into()))?
        } else if self.key.is_some() {
            return Err(WorkflowError::Store(
                "plaintext workflow store opened with a key; refusing to mix".into(),
            ));
        } else {
            text.into_bytes()
        };
        serde_json::from_slice(&plain)
            .map_err(|e| WorkflowError::Store(format!("corrupt workflow store: {e}")))
    }

    fn write(&self, file: &File) -> Result<(), WorkflowError> {
        use rand::RngCore;
        let tmp = self.path.with_extension("json.tmp");
        let plain = serde_json::to_vec(file).map_err(|e| WorkflowError::Store(e.to_string()))?;
        let bytes = match &self.key {
            None => plain,
            Some(key) => {
                let mut nonce = [0u8; 12];
                rand::rngs::OsRng.fill_bytes(&mut nonce);
                let ct = harbor_store::keys::aead_seal(key, &nonce, &plain, AAD)
                    .map_err(|_| WorkflowError::Store("seal failed".into()))?;
                format!(
                    "{SEALED_PREFIX}{}{}",
                    crate::memory::hex_encode(&nonce),
                    crate::memory::hex_encode(&ct)
                )
                .into_bytes()
            }
        };
        std::fs::write(&tmp, bytes)
            .map_err(|e| WorkflowError::Store(format!("{}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| WorkflowError::Store(format!("{}: {e}", self.path.display())))
    }

    pub fn create(&self, rec: WorkflowRecord) -> Result<WorkflowRecord, WorkflowError> {
        let mut f = self.read()?;
        if rec.id.trim().is_empty() || rec.template.trim().is_empty() {
            return Err(WorkflowError::Invalid(
                "id and template are required".into(),
            ));
        }
        if f.workflows.iter().any(|w| w.id == rec.id) {
            return Err(WorkflowError::Invalid(format!(
                "workflow {} exists",
                rec.id
            )));
        }
        if f.workflows.len() >= MAX_WORKFLOWS {
            return Err(WorkflowError::Invalid(
                "too many workflows; delete some".into(),
            ));
        }
        f.workflows.push(rec.clone());
        self.write(&f)?;
        Ok(rec)
    }

    pub fn get(&self, id: &str) -> Result<Option<WorkflowRecord>, WorkflowError> {
        Ok(self.read()?.workflows.into_iter().find(|w| w.id == id))
    }

    pub fn list(&self) -> Result<Vec<WorkflowRecord>, WorkflowError> {
        Ok(self.read()?.workflows)
    }

    /// Replace a record (same id). Step lists only ever grow: a shorter
    /// `run_ids` than the stored one is refused so a stale writer cannot
    /// forget a started run.
    pub fn update(&self, rec: WorkflowRecord) -> Result<(), WorkflowError> {
        let mut f = self.read()?;
        let slot = f
            .workflows
            .iter_mut()
            .find(|w| w.id == rec.id)
            .ok_or_else(|| WorkflowError::NotFound(rec.id.clone()))?;
        if rec.run_ids.len() < slot.run_ids.len() {
            return Err(WorkflowError::Invalid("run_ids cannot shrink".into()));
        }
        *slot = rec;
        self.write(&f)
    }

    pub fn delete(&self, id: &str) -> Result<(), WorkflowError> {
        let mut f = self.read()?;
        let n = f.workflows.len();
        f.workflows.retain(|w| w.id != id);
        if f.workflows.len() == n {
            return Err(WorkflowError::NotFound(id.into()));
        }
        self.write(&f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str) -> WorkflowRecord {
        WorkflowRecord {
            id: id.into(),
            template: "t".into(),
            chat_package: None,
            inputs: serde_json::json!({"a": 1}),
            created_at: Utc::now(),
            run_ids: vec!["run-1".into()],
            status: "running".into(),
            stop_reason: None,
        }
    }

    #[test]
    fn sealed_roundtrip_survives_reopen_and_leaves_no_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wf.json");
        let key = harbor_store::keys::KeyMaterial::random();
        {
            let s = WorkflowStore::open_with_key(&path, key.clone()).unwrap();
            s.create(rec("wf-1")).unwrap();
            let mut r = s.get("wf-1").unwrap().unwrap();
            r.run_ids.push("run-2".into());
            s.update(r).unwrap();
        }
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("enc.v1:") && !raw.contains("wf-1"));
        let s = WorkflowStore::open_with_key(&path, key).unwrap();
        assert_eq!(s.get("wf-1").unwrap().unwrap().run_ids.len(), 2);
        assert!(WorkflowStore::open(&path).unwrap().list().is_err());
    }

    #[test]
    fn run_ids_cannot_shrink_and_duplicates_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let s = WorkflowStore::open(dir.path().join("wf.json")).unwrap();
        s.create(rec("wf-1")).unwrap();
        assert!(s.create(rec("wf-1")).is_err());
        let mut r = rec("wf-1");
        r.run_ids.clear();
        assert!(s.update(r).is_err());
        assert!(s.delete("nope").is_err());
        s.delete("wf-1").unwrap();
    }
}
