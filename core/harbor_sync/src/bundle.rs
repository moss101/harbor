//! Snapshot bulk-record transport: the sealed bundle a re-enrolling or
//! expired-then-returning device downloads before uploading changes
//! (11_Sync_Protocol.md "Offline and restore").
//!
//! The bundle carries the live record tails — the latest envelope per
//! object, tombstones included — sealed under the CURRENT group epoch key
//! with a unique nonce. Snapshot metadata (group, epoch, watermark,
//! signer) is signed by the issuing device's identity key. A returning
//! device that has re-enrolled — and therefore received the current epoch
//! key through the authenticated enrollment channel — can open it.
//! Bundles from older group states are rejected by epoch check.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::envelope::{RecordEnvelope, SyncError, SyncGroup};

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("bundle epoch {bundle} does not match group epoch {group}")]
    StaleBundle { bundle: u64, group: u64 },
    #[error("bundle authentication failed (wrong epoch key or tampering)")]
    AuthFailed,
    #[error("snapshot signature verification failed")]
    BadSignature,
    #[error("malformed bundle: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedSnapshotBundle {
    pub schema: String,
    pub group_id: String,
    pub epoch: u64,
    pub deletion_watermark_rfc3339: String,
    pub signer_device_id: String,
    pub signature: String,
    pub nonce: [u8; 12],
    pub records_ciphertext: Vec<u8>,
}

fn bundle_aad(group: &SyncGroup, watermark: DateTime<Utc>, signer: &str) -> String {
    format!(
        "harbor.sync_bundle/v1|{}|{}|{}|{}",
        group.group_id,
        group.current_epoch,
        watermark.to_rfc3339(),
        signer
    )
}

/// Seal the live record tails into a snapshot bundle under the group's
/// CURRENT epoch key, signed by the issuing device's identity key.
#[allow(clippy::too_many_arguments)]
pub fn seal_bundle(
    group: &SyncGroup,
    signer_seed: &[u8; 32],
    signer_device_id: &str,
    watermark: DateTime<Utc>,
    _issued_at: DateTime<Utc>,
    tails: &[RecordEnvelope],
) -> Result<SealedSnapshotBundle, SyncError> {
    // Only current-epoch records ride a live snapshot: older-epoch records
    // are history, re-derivable from restored backups, and a returning
    // device must not receive records sealed under keys it may not hold.
    //
    // Dedupe to the LATEST envelope per object (highest HLC): a snapshot
    // certifies current state, not history. Tombstones participate as the
    // latest state of their object, so deletions propagate.
    let mut latest: std::collections::BTreeMap<&str, &RecordEnvelope> = Default::default();
    for e in tails.iter().filter(|e| e.key_epoch == group.current_epoch) {
        match latest.get(e.object_id.as_str()) {
            Some(prev) if prev.hlc >= e.hlc => {}
            _ => {
                latest.insert(e.object_id.as_str(), e);
            }
        }
    }
    let current: Vec<&RecordEnvelope> = {
        let mut v: Vec<&RecordEnvelope> = latest.values().copied().collect();
        // Deterministic order: by object id.
        v.sort_by(|a, b| a.object_id.cmp(&b.object_id));
        v
    };
    let serialized = serde_json::to_vec(&current)
        .map_err(|e| SyncError::Other(format!("serialize tails: {e}")))?;

    let key = group.epoch_key_current();
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key.0));
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let aad = bundle_aad(group, watermark, signer_device_id);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &serialized,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| SyncError::Other("seal failed".into()))?;

    let signing = SigningKey::from_bytes(signer_seed);
    let mut hasher = Sha256::new();
    hasher.update(aad.as_bytes());
    hasher.update(&serialized);
    let signature = hex::encode(signing.sign(&hasher.finalize()).to_bytes());

    Ok(SealedSnapshotBundle {
        schema: "harbor.sync_bundle/v1".into(),
        group_id: group.group_id.clone(),
        epoch: group.current_epoch,
        deletion_watermark_rfc3339: watermark.to_rfc3339(),
        signer_device_id: signer_device_id.into(),
        signature,
        nonce,
        records_ciphertext: ciphertext,
    })
}

/// Verify the bundle's snapshot signature and open the record tails with
/// the CURRENT group epoch key. Epoch mismatch is rejected before any
/// decryption is attempted.
pub fn open_bundle(
    group: &SyncGroup,
    bundle: &SealedSnapshotBundle,
    signer_public: &VerifyingKey,
) -> Result<Vec<RecordEnvelope>, BundleError> {
    if bundle.epoch != group.current_epoch {
        return Err(BundleError::StaleBundle {
            bundle: bundle.epoch,
            group: group.current_epoch,
        });
    }
    // Signature verification over the AAD + decrypt-to-be (identity bound
    // by hashing the AAD before decrypt; the AEAD itself authenticates the
    // records under the epoch key).
    let key = group.epoch_key_current();
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key.0));
    let aad = format!(
        "harbor.sync_bundle/v1|{}|{}|{}|{}",
        group.group_id,
        group.current_epoch,
        bundle.deletion_watermark_rfc3339,
        bundle.signer_device_id
    );
    let serialized = cipher
        .decrypt(
            Nonce::from_slice(&bundle.nonce),
            Payload {
                msg: &bundle.records_ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| BundleError::AuthFailed)?;

    // Snapshot signature binds the metadata the returning device relies on.
    let mut hasher = Sha256::new();
    hasher.update(aad.as_bytes());
    hasher.update(&serialized);
    let sig_bytes = hex::decode(&bundle.signature).map_err(|_| BundleError::BadSignature)?;
    let arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| BundleError::BadSignature)?;
    let sig = Signature::from_bytes(&arr);
    signer_public
        .verify(&hasher.finalize(), &sig)
        .map_err(|_| BundleError::BadSignature)?;

    let envelopes: Vec<RecordEnvelope> =
        serde_json::from_slice(&serialized).map_err(|e| BundleError::Malformed(e.to_string()))?;
    Ok(envelopes)
}

/// Digest helper shared with the signature path.
pub fn digest(bundle: &SealedSnapshotBundle) -> Vec<u8> {
    Sha256::digest(&bundle.records_ciphertext).to_vec()
}
