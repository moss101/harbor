//! Key lifecycle: a device root key from the OS secure store wraps
//! per-workspace keys; per-workspace keys wrap per-blob data keys.
//!
//! The [`KeyStore`] trait is the only key-source boundary. Native adapters
//! (iOS Keychain, Android Keystore, Windows DPAPI / Credential Manager)
//! implement it on their platforms; `FileKeyStore` is the app-private,
//! file-backed implementation used by desktop development builds and tests.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use std::path::PathBuf;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Result, StoreError};

/// 256-bit key material; zeroized on drop.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct KeyMaterial(pub [u8; 32]);

impl KeyMaterial {
    pub fn random() -> Self {
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        KeyMaterial(k)
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        let arr: [u8; 32] = b.try_into().map_err(|_| StoreError::Crypto)?;
        Ok(KeyMaterial(arr))
    }
}

impl KeyMaterial {
    /// Domain-separated subkey derivation: SHA-256(domain || base). Used
    /// to derive purpose-scoped keys (e.g. the run-event payload key)
    /// from a workspace key without storing a second wrapped key.
    pub fn derive_subkey(base: &KeyMaterial, domain: &str) -> KeyMaterial {
        let mut input = Vec::with_capacity(domain.len() + 32);
        input.extend_from_slice(domain.as_bytes());
        input.extend_from_slice(&base.0);
        let digest = harbor_canonical::sha256_hex(&input);
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).unwrap_or(0);
        }
        KeyMaterial(out)
    }
}

impl std::fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KeyMaterial(<redacted>)")
    }
}

/// A key wrapped (authenticated-encrypted) by another key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedKey {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

impl WrappedKey {
    pub fn wrap(kek: &KeyMaterial, plaintext: &[u8]) -> Result<Self> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&kek.0));
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|_| StoreError::Crypto)?;
        Ok(WrappedKey {
            nonce: nonce_bytes,
            ciphertext: ct,
        })
    }

    pub fn unwrap(&self, kek: &KeyMaterial) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&kek.0));
        cipher
            .decrypt(Nonce::from_slice(&self.nonce), self.ciphertext.as_slice())
            .map_err(|_| StoreError::Crypto)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.ciphertext.len());
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        if b.len() < 12 + 16 {
            return Err(StoreError::Crypto);
        }
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&b[..12]);
        Ok(WrappedKey {
            nonce,
            ciphertext: b[12..].to_vec(),
        })
    }
}

/// Source of the device root key. Implementations must store the key in
/// OS-protected storage with device-bound availability.
pub trait KeyStore: Send + Sync {
    /// Return the device root key, creating it on first use.
    fn device_root_key(&self, service: &str) -> Result<KeyMaterial>;
}

/// File-backed keystore used for desktop development builds and tests.
/// Files hold 32 raw bytes with 0600 permissions inside an app-private
/// directory. Production mobile/desktop builds replace this with the
/// platform adapter; the trait boundary is identical.
#[derive(Debug, Clone)]
pub struct FileKeyStore {
    dir: PathBuf,
}

impl FileKeyStore {
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&dir)?;
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(&dir, perms)?;
        }
        Ok(FileKeyStore { dir })
    }

    fn key_path(&self, service: &str) -> PathBuf {
        // Service names are Harbor-controlled identifiers; reject separators
        // so the mapping stays injective.
        debug_assert!(!service.contains('/') && !service.contains('\\'));
        self.dir
            .join(format!("{}.key", service.replace(['/', '\\', ':'], "_")))
    }

    /// Whether a key file exists for [service] (rotation checks use this
    /// instead of `device_root_key`, which would generate on absence).
    pub fn exists(&self, service: &str) -> bool {
        self.key_path(service).exists()
    }

    /// Delete the stored key (crypto-erasure after a root rotation). The
    /// returned material is zeroized on drop.
    pub fn remove(&self, service: &str) -> Result<()> {
        let path = self.key_path(service);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}

impl KeyStore for FileKeyStore {
    fn device_root_key(&self, service: &str) -> Result<KeyMaterial> {
        let path = self.key_path(service);
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            return KeyMaterial::from_bytes(&bytes);
        }
        let key = KeyMaterial::random();
        std::fs::write(&path, &key.0)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }
        Ok(key)
    }
}

/// AEAD helper shared by the blob store: authenticated encryption of a
/// buffer under a key, with additional data binding the blob identity.
pub fn aead_seal(
    key: &KeyMaterial,
    nonce: &[u8; 12],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key.0));
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| StoreError::Crypto)
}

pub fn aead_open(
    key: &KeyMaterial,
    nonce: &[u8; 12],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key.0));
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| StoreError::Crypto)
}

/// Derive the per-workspace blob encryption context: unwrap the workspace
/// key once and hand out a guard that re-wraps per-blob keys.
#[derive(Clone)]
pub struct WorkspaceKey {
    kek: KeyMaterial,
}

impl std::fmt::Debug for WorkspaceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorkspaceKey(<redacted>)")
    }
}

impl PartialEq for WorkspaceKey {
    fn eq(&self, other: &Self) -> bool {
        // Constant-time-ish comparison; test helper semantics only.
        self.kek
            .0
            .iter()
            .zip(other.kek.0.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

impl WorkspaceKey {
    pub fn generate() -> Self {
        WorkspaceKey {
            kek: KeyMaterial::random(),
        }
    }

    /// Raw key material for domain-separated subkey derivation.
    pub fn kek_material(&self) -> &KeyMaterial {
        &self.kek
    }

    pub fn wrap_with(&self, root: &KeyMaterial) -> Result<WrappedKey> {
        WrappedKey::wrap(root, &self.kek.0)
    }

    pub fn from_wrapped(root: &KeyMaterial, wrapped: &WrappedKey) -> Result<Self> {
        let bytes = wrapped.unwrap(root)?;
        let kek = KeyMaterial::from_bytes(&bytes)?;
        Ok(WorkspaceKey { kek })
    }

    /// Wrap a fresh per-blob data key; returns (dek, wrapped_dek).
    pub fn new_blob_key(&self) -> Result<(KeyMaterial, WrappedKey)> {
        let dek = KeyMaterial::random();
        let wrapped = WrappedKey::wrap(&self.kek, &dek.0)?;
        Ok((dek, wrapped))
    }

    pub fn unwrap_blob_key(&self, wrapped: &WrappedKey) -> Result<KeyMaterial> {
        let bytes = wrapped.unwrap(&self.kek)?;
        KeyMaterial::from_bytes(&bytes)
    }

    /// Re-wrap this workspace key under a new device root key (root rotation).
    pub fn rewrap_root(
        &self,
        old_root: &KeyMaterial,
        new_root: &KeyMaterial,
        _wrapped: &WrappedKey,
    ) -> Result<WrappedKey> {
        let _ = old_root;
        self.wrap_with(new_root)
    }
}

/// Verify a wrapped key round-trips under the given roots (test helper and
/// root-rotation validation).
pub fn verify_rewrap(
    kek: &KeyMaterial,
    wrapped: &WrappedKey,
    root_old: &KeyMaterial,
    root_new: &KeyMaterial,
) -> Result<bool> {
    let original = wrapped.unwrap(root_old)?;
    let rewrapped = WrappedKey::wrap(root_new, &original)?;
    Ok(rewrapped.unwrap(root_new)? == kek.0.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_unwrap_roundtrip() {
        let root = KeyMaterial::random();
        let wk = WorkspaceKey::generate();
        let wrapped = wk.wrap_with(&root).unwrap();
        let wk2 = WorkspaceKey::from_wrapped(&root, &wrapped).unwrap();
        assert_eq!(wk, wk2);
    }

    #[test]
    fn wrong_root_fails() {
        let root = KeyMaterial::random();
        let other = KeyMaterial::random();
        let wk = WorkspaceKey::generate();
        let wrapped = wk.wrap_with(&root).unwrap();
        assert!(WorkspaceKey::from_wrapped(&other, &wrapped).is_err());
    }

    #[test]
    fn root_rotation_rewraps_workspace_key() {
        let root = KeyMaterial::random();
        let wk = WorkspaceKey::generate();
        let wrapped = wk.wrap_with(&root).unwrap();
        // Blob keys work before rotation.
        let (dek, wrapped_dek) = wk.new_blob_key().unwrap();
        assert_eq!(wk.unwrap_blob_key(&wrapped_dek).unwrap(), dek);
        // Rotate: rewrap the workspace key under a new device root key.
        let new_root = KeyMaterial::random();
        let rewrapped = wk.rewrap_root(&root, &new_root, &wrapped).unwrap();
        assert_eq!(
            WorkspaceKey::from_wrapped(&new_root, &rewrapped).unwrap(),
            wk
        );
        // Old root no longer unwraps the new wrapped key.
        assert!(WorkspaceKey::from_wrapped(&root, &rewrapped).is_err());
    }

    #[test]
    fn blob_key_roundtrip_under_workspace_key() {
        let wk = WorkspaceKey::generate();
        let (dek, wrapped_dek) = wk.new_blob_key().unwrap();
        let recovered = wk.unwrap_blob_key(&wrapped_dek).unwrap();
        assert_eq!(recovered, dek);
    }

    #[test]
    fn file_keystore_roundtrip_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let ks = FileKeyStore::new(dir.path()).unwrap();
        let k1 = ks.device_root_key("harbor.test").unwrap();
        let ks2 = FileKeyStore::new(dir.path()).unwrap();
        let k2 = ks2.device_root_key("harbor.test").unwrap();
        assert_eq!(k1, k2, "same service must return the persisted key");
        let other = ks.device_root_key("harbor.other").unwrap();
        assert_ne!(k1, other);
    }
}
