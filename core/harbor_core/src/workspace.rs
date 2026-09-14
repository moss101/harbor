//! Workspace: the durable unit of Harbor. One workspace = one privacy
//! policy, one key scope, one blob namespace, one run store.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Duration, Utc};

use harbor_agent::EventLog;
use harbor_net::audit::SqliteAuditSink;
use harbor_net::broker::EgressBroker;
use harbor_store::blob::BlobStore;
use harbor_store::keys::{FileKeyStore, KeyStore, WorkspaceKey};
use harbor_store::Database;
use harbor_security::policy::PrivacyMode;

use crate::HarborError;

pub struct OpenOptions {
    /// App-private data root (workspace-independent).
    pub data_root: PathBuf,
    pub device_id: String,
}

pub struct Workspace {
    pub workspace_id: String,
    pub privacy_mode: PrivacyMode,
    pub privacy_policy_version: String,
    pub agent_log: Arc<EventLog>,
    pub broker: Arc<EgressBroker>,
    blobs: BlobStore,
    store_db_path: PathBuf,
    key_source: Arc<dyn KeyStore>,
    workspace_key: WorkspaceKey,
}

impl Workspace {
    /// Open or create a workspace with the platform dev keystore
    /// (file-backed). Production callers go through
    /// [`Workspace::open_with_keystore`] with an OS keystore adapter.
    pub fn open(opts: &OpenOptions, workspace_id: &str, mode: PrivacyMode) -> Result<Workspace, HarborError> {
        let key_source = FileKeyStore::new(opts.data_root.join("keys"))
            .map_err(HarborError::Store)?;
        Self::open_with_keystore(opts, workspace_id, mode, Arc::new(key_source))
    }

    /// Open or create a workspace. Establishes keys, binds the encrypted
    /// blob store, opens the durable run log and the egress broker with a
    /// SQLite-backed network audit. `key_source` is the device root key
    /// origin (OS keystore adapter or injected root); the workspace keeps
    /// it for the workspace lifetime.
    pub fn open_with_keystore(
        opts: &OpenOptions,
        workspace_id: &str,
        mode: PrivacyMode,
        key_source: Arc<dyn KeyStore>,
    ) -> Result<Workspace, HarborError> {
        std::fs::create_dir_all(opts.data_root.join("db"))?;
        let root = key_source.device_root_key("harbor.device")?;
        let store_db_path = opts.data_root.join("db").join("store.db");
        let mut db = Database::open(&store_db_path)?;
        db.migrate(&[])?;

        let blobs = BlobStore::new(opts.data_root.join("blobs"))?;
        // Create or load the workspace key (wrapped by the device root key
        // in the store database).
        let wrapped = db.write(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS workspace_keys (
                    workspace_id TEXT PRIMARY KEY,
                    wrapped_key BLOB NOT NULL,
                    wrap_version INTEGER NOT NULL DEFAULT 1
                );",
            )
            .map_err(harbor_store::StoreError::Db)?;
            let existing: Option<Vec<u8>> = c
                .query_row(
                    "SELECT wrapped_key FROM workspace_keys WHERE workspace_id = ?1",
                    [workspace_id],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            Ok(match existing {
                Some(bytes) => harbor_store::keys::WrappedKey::from_bytes(&bytes)
                    .map_err(|_| harbor_store::StoreError::Crypto)?,
                None => {
                    let wk = WorkspaceKey::generate();
                    let wrapped = wk.wrap_with(&root)?;
                    c.execute(
                        "INSERT INTO workspace_keys (workspace_id, wrapped_key) VALUES (?1, ?2)",
                        rusqlite::params![workspace_id, wrapped.to_bytes()],
                    )
                    .map_err(harbor_store::StoreError::Db)?;
                    wrapped
                }
            })
        })?;
        let key = WorkspaceKey::from_wrapped(&root, &wrapped)?;
        blobs.bind_workspace(workspace_id, key.clone(), wrapped);
        // Run evidence is private workspace data (policy 13): the event
        // log seals its payloads with a key derived from the workspace
        // key (domain-separated), never stored raw on disk.
        let payload_key = harbor_store::keys::KeyMaterial::derive_subkey(
            key.kek_material(),
            "harbor.agent.event-payload/v1",
        );

        let audit = SqliteAuditSink::open(opts.data_root.join("db").join("network_audit.db"))
            .map_err(HarborError::Store)?;
        let broker = Arc::new(EgressBroker::new(Box::new(audit)));

        let agent_log = Arc::new(EventLog::open_with_payload_key(
            opts.data_root.join("db").join("agent.db"),
            payload_key,
        )?);

        Ok(Workspace {
            workspace_id: workspace_id.into(),
            privacy_mode: mode,
            privacy_policy_version: "policy-2026.09".into(),
            agent_log,
            broker,
            blobs,
            store_db_path,
            key_source,
            workspace_key: key,
        })
    }

    /// Trust Pulse facts: policy AND where execution actually happened are
    /// separate facts; this returns the policy half.
    pub fn trust_pulse_policy(&self) -> String {
        format!(
            "Policy: {} — execution location is reported per run by the provider.",
            self.privacy_mode
        )
    }

    /// Blob access for private artifacts (extracts, previews, run evidence).
    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    pub fn store_db_path(&self) -> &Path {
        &self.store_db_path
    }

    /// Device root identity (used in receipt binding).
    pub fn device_key(&self) -> Result<harbor_store::keys::KeyMaterial, HarborError> {
        self.key_source
            .device_root_key("harbor.device")
            .map_err(HarborError::Store)
    }

    /// Key for sealing knowledge chunks at rest, derived from the
    /// workspace key (domain-separated). Never persisted.
    pub fn knowledge_chunk_key(
        &self,
    ) -> Result<harbor_store::keys::KeyMaterial, HarborError> {
        Ok(harbor_store::kcipher::knowledge_chunk_key(
            self.workspace_key.kek_material(),
        ))
    }

    /// Default egress session lifetime for explicit acquisition sessions.
    pub fn acquisition_session_ttl() -> Duration {
        Duration::minutes(10)
    }

    /// The timestamp source used for durable records (single clock).
    pub fn now() -> chrono::DateTime<chrono::Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_keys_blobs_and_log() {
        let dir = tempfile::tempdir().unwrap();
        let opts = OpenOptions { data_root: dir.path().to_path_buf(), device_id: "dev-1".into() };
        let ws = Workspace::open(&opts, "ws-1", PrivacyMode::LocalOnly).unwrap();
        assert_eq!(ws.privacy_mode.as_str(), "LOCAL_ONLY");
        assert!(ws.trust_pulse_policy().contains("LOCAL_ONLY"));
        // Runs can be created through the facade's durable log.
        ws.agent_log.create_run("run-1", "ws-1", Workspace::now()).unwrap();
        let (state, _, _) = ws.agent_log.run_state("run-1").unwrap();
        assert_eq!(state, harbor_agent::RunState::Created);
        // Blobs are bound and private.
        let r = ws
            .blobs()
            .put("ws-1", b"private payload", &Default::default())
            .unwrap();
        assert_eq!(ws.blobs().list("ws-1").unwrap(), vec![r.id.clone()]);
        // Second open: same key context (idempotent creation).
        let ws2 = Workspace::open(&opts, "ws-1", PrivacyMode::LocalOnly).unwrap();
        let out = ws2
            .blobs()
            .get("ws-1", &r.id, &Default::default())
            .unwrap();
        assert_eq!(out, b"private payload");
    }

    #[test]
    fn workspaces_have_separate_keys() {
        let dir = tempfile::tempdir().unwrap();
        let opts = OpenOptions { data_root: dir.path().to_path_buf(), device_id: "dev-1".into() };
        let a = Workspace::open(&opts, "ws-a", PrivacyMode::LocalOnly).unwrap();
        let _b = Workspace::open(&opts, "ws-b", PrivacyMode::Hybrid).unwrap();
        let r = a.blobs().put("ws-a", b"secret-a", &Default::default()).unwrap();
        // Cross-workspace read must be impossible (no key bound for ws-a on b).
        assert!(a.blobs().get("ws-b", &r.id, &Default::default()).is_err());
    }
}
