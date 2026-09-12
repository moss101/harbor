//! Catalog signing: monotonic epochs + key rotation (03 §7).
//!
//! The model catalog is signed data. Rules:
//! - Signatures are Ed25519; a document binds the canonical JSON of
//!   `{epoch, published_at, entries}` so any tampering breaks it.
//! - Epochs are monotonic: an accepted catalog of epoch N makes every
//!   catalog with epoch <= N invalid (rollback protection).
//! - Keys rotate via rotation records signed by an already-trusted key;
//!   a rotation adds and/or removes keys in the same monotonic epoch
//!   space. The bootstrap trust anchor is the pinned release key.
//! - An old (revoked) key can no longer make catalogs acceptable, even
//!   with a fresh epoch.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use harbor_canonical::JsonValue;

#[derive(Debug, thiserror::Error)]
pub enum CatalogSignError {
    #[error("signature verification failed")]
    BadSignature,
    #[error("epoch regression: accepted {accepted}, got {got}")]
    StaleEpoch { accepted: u64, got: u64 },
    #[error("unknown signing key {0}")]
    UnknownKey(String),
    #[error("revoked key {0} cannot sign")]
    RevokedKey(String),
    #[error("malformed catalog: {0}")]
    Malformed(String),
}

/// Ed25519 key pair with a stable key id (sha256 of the public key).
pub struct CatalogSigningKey {
    pub key_id: String,
    signing: SigningKey,
}

impl CatalogSigningKey {
    pub fn generate() -> Self {
        let mut secret = [0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut secret);
        let signing = SigningKey::from_bytes(&secret);
        Self::from_signing(signing)
    }

    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self::from_signing(SigningKey::from_bytes(bytes))
    }

    fn from_signing(signing: SigningKey) -> Self {
        let key_id = harbor_canonical::sha256_hex(&signing.verifying_key().to_bytes())
            .get(..16)
            .unwrap_or("key")
            .to_string();
        CatalogSigningKey { key_id, signing }
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// The 32-byte secret seed (for secure off-repo persistence).
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }
}

/// A signed catalog document (entries carried as canonical JSON).
#[derive(Debug, Clone, PartialEq)]
pub struct SignedCatalog {
    pub epoch: u64,
    pub published_at: String,
    pub entries: JsonValue,
    pub key_id: String,
    pub signature: String,
}

fn canonical_payload(epoch: u64, published_at: &str, entries: &JsonValue) -> Result<Vec<u8>, CatalogSignError> {
    let v = JsonValue::object([
        ("entries", entries.clone()),
        ("epoch", JsonValue::int(epoch as i64).map_err(|e| CatalogSignError::Malformed(e.to_string()))?),
        ("published_at", JsonValue::str(published_at)),
    ]);
    v.to_canonical_bytes()
        .map_err(|e| CatalogSignError::Malformed(e.to_string()))
}

fn sign_bytes(key: &CatalogSigningKey, payload: &[u8]) -> String {
    {
        let sig = key.signing.sign(payload);
        hex::encode(&sig.to_bytes())
    }
}

fn verify_bytes(public: &VerifyingKey, payload: &[u8], sig_hex: &str) -> bool {
    let Some(bytes) = hex::decode(sig_hex) else { return false };
    let Ok(arr) = <[u8; 64]>::try_from(bytes.as_slice()) else { return false };
    let sig = Signature::from_bytes(&arr);
    public.verify(payload, &sig).is_ok()
}

fn parse_public(bytes32: &str) -> Result<VerifyingKey, CatalogSignError> {
    let raw = hex::decode(bytes32).ok_or(CatalogSignError::Malformed("bad key hex".into()))?;
    let arr: [u8; 32] = raw
        .try_into()
        .map_err(|_| CatalogSignError::Malformed("bad key length".into()))?;
    VerifyingKey::from_bytes(&arr).map_err(|_| CatalogSignError::Malformed("bad key".into()))
}

/// Sign `entries` at `epoch` with `key`.
pub fn sign_catalog(
    key: &CatalogSigningKey,
    epoch: u64,
    published_at: &str,
    entries: JsonValue,
) -> Result<SignedCatalog, CatalogSignError> {
    let payload = canonical_payload(epoch, published_at, &entries)?;
    Ok(SignedCatalog {
        epoch,
        published_at: published_at.to_string(),
        entries,
        key_id: key.key_id.clone(),
        signature: sign_bytes(key, &payload),
    })
}

/// A signed key-rotation record: add/remove verification keys.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedRotation {
    pub epoch: u64,
    pub published_at: String,
    pub add_keys: Vec<(String, String)>, // (key_id, public hex)
    pub remove_key_ids: Vec<String>,
    pub key_id: String,
    pub signature: String,
}

fn rotation_payload(r: &SignedRotation) -> Result<Vec<u8>, CatalogSignError> {
    let adds: Vec<JsonValue> = r
        .add_keys
        .iter()
        .map(|(id, pk)| JsonValue::object([("key_id", JsonValue::str(id.clone())), ("public", JsonValue::str(pk.clone()))]))
        .collect();
    let removes: Vec<JsonValue> = r.remove_key_ids.iter().map(|k| JsonValue::str(k.clone())).collect();
    let v = JsonValue::object([
        ("add_keys", JsonValue::Array(adds)),
        ("epoch", JsonValue::int(r.epoch as i64).map_err(|e| CatalogSignError::Malformed(e.to_string()))?),
        ("published_at", JsonValue::str(r.published_at.clone())),
        ("remove_key_ids", JsonValue::Array(removes)),
    ]);
    v.to_canonical_bytes()
        .map_err(|e| CatalogSignError::Malformed(e.to_string()))
}

pub fn sign_rotation(
    key: &CatalogSigningKey,
    epoch: u64,
    published_at: &str,
    add_keys: Vec<(String, String)>,
    remove_key_ids: Vec<String>,
) -> Result<SignedRotation, CatalogSignError> {
    let record = SignedRotation {
        epoch,
        published_at: published_at.to_string(),
        add_keys,
        remove_key_ids,
        key_id: key.key_id.clone(),
        signature: String::new(),
    };
    let payload = rotation_payload(&record)?;
    let mut record = record;
    record.signature = sign_bytes(key, &payload);
    Ok(record)
}

/// Verification state: trusted keys + accepted epoch. Persisted by callers
/// via harbor_store (state must survive restarts to keep epochs monotonic).
pub struct CatalogVerifier {
    pub trusted: BTreeMap<String, String>, // key_id -> public hex
    pub accepted_epoch: u64,
    pub revoked: BTreeMap<String, ()>,
}

impl CatalogVerifier {
    /// Bootstrap with the release-pinned root key.
    pub fn new(root_public_hex: &str) -> Result<Self, CatalogSignError> {
        let _ = parse_public(root_public_hex)?;
        let raw = hex::decode(root_public_hex).ok_or(CatalogSignError::Malformed("hex".into()))?;
        let key_id = harbor_canonical::sha256_hex(&raw)
            .get(..16)
            .unwrap_or("root")
            .to_string();
        let mut trusted = BTreeMap::new();
        trusted.insert(key_id, root_public_hex.to_string());
        Ok(CatalogVerifier { trusted, accepted_epoch: 0, revoked: BTreeMap::new() })
    }

    /// Verify and accept a catalog document.
    pub fn verify(&mut self, catalog: &SignedCatalog) -> Result<(), CatalogSignError> {
        if catalog.epoch <= self.accepted_epoch {
            return Err(CatalogSignError::StaleEpoch { accepted: self.accepted_epoch, got: catalog.epoch });
        }
        if self.revoked.contains_key(&catalog.key_id) {
            return Err(CatalogSignError::RevokedKey(catalog.key_id.clone()));
        }
        let public_hex = self
            .trusted
            .get(&catalog.key_id)
            .ok_or_else(|| CatalogSignError::UnknownKey(catalog.key_id.clone()))?;
        let public = parse_public(public_hex)?;
        let payload = canonical_payload(catalog.epoch, &catalog.published_at, &catalog.entries)?;
        if !verify_bytes(&public, &payload, &catalog.signature) {
            return Err(CatalogSignError::BadSignature);
        }
        self.accepted_epoch = catalog.epoch;
        Ok(())
    }

    /// Apply a signed rotation; rotations obey the same epoch monotonicity.
    pub fn apply_rotation(&mut self, rotation: &SignedRotation) -> Result<(), CatalogSignError> {
        if rotation.epoch <= self.accepted_epoch {
            return Err(CatalogSignError::StaleEpoch { accepted: self.accepted_epoch, got: rotation.epoch });
        }
        if self.revoked.contains_key(&rotation.key_id) {
            return Err(CatalogSignError::RevokedKey(rotation.key_id.clone()));
        }
        let public_hex = self
            .trusted
            .get(&rotation.key_id)
            .ok_or_else(|| CatalogSignError::UnknownKey(rotation.key_id.clone()))?;
        let public = parse_public(public_hex)?;
        let payload = rotation_payload(rotation)?;
        if !verify_bytes(&public, &payload, &rotation.signature) {
            return Err(CatalogSignError::BadSignature);
        }
        // A key cannot remove itself in the same rotation that adds nothing
        // trusted: at least one trusted key must remain.
        let mut trusted = self.trusted.clone();
        for id in &rotation.remove_key_ids {
            trusted.remove(id);
            self.revoked.insert(id.clone(), ());
        }
        for (_, pk) in &rotation.add_keys {
            // Key ids are ALWAYS derived from the key bytes: a rotation
            // cannot mint an id that does not match its key material.
            let raw = hex::decode(pk).ok_or(CatalogSignError::Malformed("hex".into()))?;
            let id = harbor_canonical::sha256_hex(&raw)
                .get(..16)
                .ok_or(CatalogSignError::Malformed("id".into()))?
                .to_string();
            trusted.insert(id, pk.clone());
        }
        if trusted.is_empty() {
            return Err(CatalogSignError::Malformed("rotation would remove all trusted keys".into()));
        }
        self.trusted = trusted;
        self.accepted_epoch = rotation.epoch;
        Ok(())
    }
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn decode(s: &str) -> Option<Vec<u8>> {
        if s.len() % 2 != 0 {
            return None;
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> CatalogSigningKey {
        CatalogSigningKey::from_secret_bytes(&[7u8; 32])
    }

    fn entries() -> JsonValue {
        harbor_canonical::parse(r#"[{"id":"model-a","tier":"Fast"}]"#).unwrap()
    }

    #[test]
    fn sign_verify_and_stale_epoch_rejected() {
        let k = key();
        let public = hex::encode(&k.public_bytes());
        let mut v = CatalogVerifier::new(&public).unwrap();
        let c1 = sign_catalog(&k, 1, "2026-09-12T00:00:00Z", entries()).unwrap();
        v.verify(&c1).unwrap();
        // Same epoch or older: rejected (rollback protection).
        let c1_again = sign_catalog(&k, 1, "2026-09-12T00:00:00Z", entries()).unwrap();
        assert!(matches!(
            v.verify(&c1_again),
            Err(CatalogSignError::StaleEpoch { accepted: 1, got: 1 })
        ));
        let c2 = sign_catalog(&k, 2, "2026-09-12T01:00:00Z", entries()).unwrap();
        v.verify(&c2).unwrap();
    }

    #[test]
    fn tampered_entries_rejected() {
        let k = key();
        let mut v = CatalogVerifier::new(&hex::encode(&k.public_bytes())).unwrap();
        let mut c = sign_catalog(&k, 1, "t", entries()).unwrap();
        c.entries = harbor_canonical::parse(r#"[{"id":"model-EVIL","tier":"Fast"}]"#).unwrap();
        assert!(matches!(v.verify(&c), Err(CatalogSignError::BadSignature)));
    }

    #[test]
    fn unknown_key_rejected() {
        let k1 = key();
        let k2 = CatalogSigningKey::from_secret_bytes(&[9u8; 32]);
        let mut v = CatalogVerifier::new(&hex::encode(&k1.public_bytes())).unwrap();
        let c = sign_catalog(&k2, 1, "t", entries()).unwrap();
        assert!(matches!(v.verify(&c), Err(CatalogSignError::UnknownKey(_))));
    }

    #[test]
    fn rotation_revokes_old_key_and_survives_fresh_epochs() {
        let old = key();
        let new = CatalogSigningKey::from_secret_bytes(&[11u8; 32]);
        let new_public = hex::encode(&new.public_bytes());
        let mut v = CatalogVerifier::new(&hex::encode(&old.public_bytes())).unwrap();

        let rotation = sign_rotation(
            &old,
            5,
            "t",
            vec![("new-key".into(), new_public.clone())],
            vec![old.key_id.clone()],
        )
        .unwrap();
        v.apply_rotation(&rotation).unwrap();

        // Old key with a FRESH epoch is still unacceptable: revoked.
        let c_old = sign_catalog(&old, 6, "t", entries()).unwrap();
        assert!(matches!(v.verify(&c_old), Err(CatalogSignError::RevokedKey(_))));
        // New key is now trusted.
        let c_new = sign_catalog(&new, 6, "t", entries()).unwrap();
        v.verify(&c_new).unwrap();
    }

    #[test]
    fn rotation_signed_by_untrusted_key_rejected() {
        let root = key();
        let outsider = CatalogSigningKey::from_secret_bytes(&[13u8; 32]);
        let mut v = CatalogVerifier::new(&hex::encode(&root.public_bytes())).unwrap();
        let r = sign_rotation(&outsider, 9, "t", vec![], vec![]).unwrap();
        assert!(matches!(v.apply_rotation(&r), Err(CatalogSignError::UnknownKey(_))));
    }

    #[test]
    fn cannot_remove_all_trusted_keys() {
        let root = key();
        let mut v = CatalogVerifier::new(&hex::encode(&root.public_bytes())).unwrap();
        let r = sign_rotation(&root, 3, "t", vec![], vec![root.key_id.clone()]).unwrap();
        assert!(matches!(
            v.apply_rotation(&r),
            Err(CatalogSignError::Malformed(_))
        ));
    }
}
