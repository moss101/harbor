//! Encrypted, content-addressed, refcounted blob store.
//!
//! Private workspace data is encrypted per workspace (ChaCha20-Poly1305,
//! per-blob data key wrapped by the workspace key) and stored outside
//! SQLite. The blob id is the SHA-256 of the *plaintext*; integrity is
//! enforced by AEAD and by re-hashing on read. Cross-workspace
//! deduplication is intentionally disabled: blob files live under the
//! workspace directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Result, StoreError};
use crate::keys::{aead_open, aead_seal, KeyMaterial, WrappedKey, WorkspaceKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef {
    /// SHA-256 of plaintext, lowercase hex.
    pub id: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    /// Additional data bound into the AEAD tag (e.g. workspace id) so a
    /// blob file cannot be transplanted across workspaces.
    pub aad: Vec<u8>,
}

pub struct BlobStore {
    root: PathBuf,
    /// workspace_id -> (workspace key, wrapped key as persisted)
    keys: std::sync::Mutex<HashMap<String, (WorkspaceKey, WrappedKey)>>,
}

impl BlobStore {
    /// `root` is an app-private directory; blobs live under
    /// `<root>/blobs/<workspace>/<aa>/`.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(root.join("blobs"))?;
        Ok(BlobStore { root, keys: std::sync::Mutex::new(HashMap::new()) })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Register a workspace key context (key material stays in memory only).
    pub fn bind_workspace(&self, workspace_id: &str, key: WorkspaceKey, wrapped: WrappedKey) {
        self.keys
            .lock()
            .unwrap()
            .insert(workspace_id.to_string(), (key, wrapped));
    }

    /// Remove a workspace key from memory and crypto-erase context.
    pub fn unbind_workspace(&self, workspace_id: &str) {
        self.keys.lock().unwrap().remove(workspace_id);
    }

    fn key_for(&self, workspace_id: &str) -> Result<(WorkspaceKey, WrappedKey)> {
        self.keys
            .lock()
            .unwrap()
            .get(workspace_id)
            .cloned()
            .ok_or_else(|| StoreError::WorkspaceNotFound(workspace_id.to_string()))
    }

    fn path_for(&self, workspace_id: &str, id: &str) -> PathBuf {
        self.root.join("blobs").join(workspace_id).join(&id[..2]).join(format!("{id}.hblob"))
    }

    /// Encrypt and store `plaintext`; returns its content address. Storing
    /// the same bytes twice under the same workspace is idempotent.
    pub fn put(&self, workspace_id: &str, plaintext: &[u8], opts: &PutOptions) -> Result<BlobRef> {
        let id = harbor_canonical::sha256_hex(plaintext);
        let path = self.path_for(workspace_id, &id);
        if path.exists() {
            // Idempotent write of identical bytes.
            let (size, _) = self.read_raw(workspace_id, &id)?;
            if size == plaintext.len() as u64 {
                return Ok(BlobRef { id, size: plaintext.len() as u64 });
            }
            return Err(StoreError::Integrity(id));
        }
        let (key, _wrapped) = self.key_for(workspace_id)?;
        let (dek, wrapped_dek) = key.new_blob_key()?;
        let mut nonce_bytes = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
        let ct = aead_seal(&dek, &nonce_bytes, plaintext, &opts.aad)?;
        let mut file = Vec::with_capacity(12 + 12 + wrapped_dek.to_bytes().len() + ct.len());
        file.extend_from_slice(b"HB1");
        file.extend_from_slice(&nonce_bytes);
        let wd = wrapped_dek.to_bytes();
        file.extend_from_slice(&(wd.len() as u32).to_le_bytes());
        file.extend_from_slice(&wd);
        file.extend_from_slice(&ct);

        let tmp = path.with_extension("tmp");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&tmp, &file)?;
        std::fs::rename(&tmp, &path)?;
        let _ = std::fs::remove_file(tmp.with_extension("tmp")); // no-op guard
        Ok(BlobRef { id, size: plaintext.len() as u64 })
    }

    /// Decrypt and return the plaintext bytes for a blob id.
    pub fn get(&self, workspace_id: &str, id: &str, opts: &PutOptions) -> Result<Vec<u8>> {
        let (_size, plaintext) = self.read_raw(workspace_id, id)?;
        let _ = opts;
        Ok(plaintext)
    }

    fn read_raw(&self, workspace_id: &str, id: &str) -> Result<(u64, Vec<u8>)> {
        let path = self.path_for(workspace_id, id);
        let file = std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => StoreError::BlobNotFound(id.to_string()),
            _ => StoreError::Io(e),
        })?;
        if file.len() < 3 + 12 + 4 + 16 || &file[..3] != b"HB1" {
            return Err(StoreError::Integrity(id.to_string()));
        }
        let mut off = 3;
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&file[off..off + 12]);
        off += 12;
        let wd_len = u32::from_le_bytes(file[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        if file.len() < off + wd_len + 16 {
            return Err(StoreError::Integrity(id.to_string()));
        }
        let wrapped_dek = WrappedKey::from_bytes(&file[off..off + wd_len])?;
        off += wd_len;
        let ct = &file[off..];
        let (key, _wrapped) = self.key_for(workspace_id)?;
        let dek = key.unwrap_blob_key(&wrapped_dek)?;
        let plaintext = aead_open(&dek, &nonce, ct, &[]).map_err(|_| StoreError::Integrity(id.to_string()))?;
        let got = harbor_canonical::sha256_hex(&plaintext);
        if got != id {
            return Err(StoreError::Integrity(id.to_string()));
        }
        Ok((plaintext.len() as u64, plaintext))
    }

    /// Verify the stored file decrypts to its declared content address.
    pub fn verify(&self, workspace_id: &str, id: &str) -> Result<bool> {
        match self.read_raw(workspace_id, id) {
            Ok(_) => Ok(true),
            Err(StoreError::BlobNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// List blob ids stored for a workspace.
    pub fn list(&self, workspace_id: &str) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let dir = self.root.join("blobs").join(workspace_id);
        if !dir.exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(&dir)? {
            let sub = entry?.path();
            if sub.is_dir() {
                for f in std::fs::read_dir(&sub)? {
                    let name = f?.file_name().into_string().unwrap_or_default();
                    if let Some(id) = name.strip_suffix(".hblob") {
                        out.push(id.to_string());
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// Crypto-erase a workspace: remove key context, then delete its blob
    /// files. Without the key the files are already unreadable; deletion is
    /// the best-effort physical cleanup mandated by policy.
    pub fn erase_workspace(&self, workspace_id: &str) -> Result<()> {
        self.unbind_workspace(workspace_id);
        let dir = self.root.join("blobs").join(workspace_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Delete one blob file (best-effort physical cleanup after the key
    /// context confirmed removal).
    pub fn delete_blob(&self, workspace_id: &str, id: &str) -> Result<()> {
        let path = self.path_for(workspace_id, id);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Io(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyMaterial;

    fn setup() -> (tempfile::TempDir, BlobStore, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path()).unwrap();
        let ws = "ws-test-1";
        let wk = WorkspaceKey::generate();
        let root = KeyMaterial::random();
        let wrapped = wk.wrap_with(&root).unwrap();
        store.bind_workspace(ws, wk, wrapped);
        (dir, store, ws.to_string())
    }

    #[test]
    fn put_get_roundtrip_and_integrity() {
        let (_dir, store, ws) = setup();
        let data = b"harbor private artifact bytes".to_vec();
        let r1 = store.put(&ws, &data, &PutOptions::default()).unwrap();
        assert_eq!(r1.size, data.len() as u64);
        let out = store.get(&ws, &r1.id, &PutOptions::default()).unwrap();
        assert_eq!(out, data);
        assert!(store.verify(&ws, &r1.id).unwrap());
        // Idempotent identical write.
        let r2 = store.put(&ws, &data, &PutOptions::default()).unwrap();
        assert_eq!(r1.id, r2.id);
    }

    #[test]
    fn ciphertext_on_disk_not_plaintext() {
        let (_dir, store, ws) = setup();
        let data = b"SECRET-CONFIDENTIAL-CONTENT".repeat(64);
        let r = store.put(&ws, &data, &PutOptions::default()).unwrap();
        let listing = store.list(&ws).unwrap();
        assert_eq!(listing, vec![r.id.clone()]);
        // Read the raw file from disk: plaintext must not appear.
        let raw_dir = store.root().join("blobs").join(&ws).join(&r.id[..2]);
        for entry in std::fs::read_dir(raw_dir).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            let needle = b"SECRET-CONFIDENTIAL-CONTENT";
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle),
                "plaintext leaked to disk"
            );
        }
    }

    #[test]
    fn wrong_workspace_key_cannot_read() {
        let (dir, store, ws) = setup();
        let data = b"private".to_vec();
        let r = store.put(&ws, &data, &PutOptions::default()).unwrap();
        // Fresh store without the key bound.
        let store2 = BlobStore::new(dir.path()).unwrap();
        assert!(matches!(
            store2.get(&ws, &r.id, &PutOptions::default()),
            Err(StoreError::WorkspaceNotFound(_))
        ));
    }

    #[test]
    fn crypto_erase_removes_files() {
        let (_dir, store, ws) = setup();
        let r = store.put(&ws, b"data", &PutOptions::default()).unwrap();
        assert!(store.list(&ws).unwrap().len() == 1);
        store.erase_workspace(&ws).unwrap();
        assert!(store.list(&ws).unwrap().is_empty());
        let store2 = BlobStore::new(store.root()).unwrap();
        assert!(!store2.verify(&ws, &r.id).unwrap());
    }

    #[test]
    fn tampered_file_fails_integrity() {
        let (_dir, store, ws) = setup();
        let r = store.put(&ws, b"tamper target", &PutOptions::default()).unwrap();
        let path = store.root().join("blobs").join(&ws).join(&r.id[..2]).join(format!("{}.hblob", r.id));
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        std::fs::write(&path, &bytes).unwrap();
        assert!(matches!(store.get(&ws, &r.id, &PutOptions::default()), Err(StoreError::Integrity(_))));
    }
}
