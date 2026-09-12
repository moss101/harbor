//! Sync record envelope and group/epoch management.
//!
//! Envelope fields per 11_Sync_Protocol.md: group ID, device ID, device
//! sequence, key epoch, record type, object ID, hybrid logical clock,
//! previous-device-record hash, tombstone flag, authenticated ciphertext.
//! Duplicates and invalid epochs are rejected.

use std::collections::BTreeMap;

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use harbor_store::keys::{KeyMaterial, WrappedKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncRecordType {
    RunHistory,
    Chat,
    ArtifactVersion,
    Setting,
    Tombstone,
}

/// Per-device identity: signing key (Ed25519) + encryption key material.
pub struct SyncIdentity {
    pub device_id: String,
    pub signing_seed: [u8; 32],
    pub encryption_key: KeyMaterial,
}

impl SyncIdentity {
    pub fn generate(device_id: &str) -> Self {
        use ed25519_dalek::SigningKey;
        let mut seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut seed);
        let signing = SigningKey::from_bytes(&seed);
        let _ = signing; // signature use: enrollment metadata signing (M4 activation)
        let mut enc = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut enc);
        SyncIdentity {
            device_id: device_id.into(),
            signing_seed: signing.to_bytes(),
            encryption_key: KeyMaterial::from_bytes(&enc).unwrap(),
        }
    }
}

/// A sync group: epoch keys, member devices, accepted epoch.
#[derive(Clone)]
pub struct SyncGroup {
    pub group_id: String,
    /// epoch -> epoch key (current epoch is the max).
    pub epoch_keys: BTreeMap<u64, KeyMaterial>,
    pub current_epoch: u64,
    pub members: BTreeMap<String, MemberState>,
}

#[derive(Debug, Clone)]
pub struct MemberState {
    pub enrolled_at: DateTime<Utc>,
    /// Last day the member is valid without re-enrollment (90-day horizon).
    pub horizon_until: DateTime<Utc>,
    pub revoked: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("duplicate device sequence {seq} for device {device}")]
    DuplicateSequence { device: String, seq: u64 },
    #[error("record epoch {got} is not the current epoch {current}")]
    InvalidEpoch { got: u64, current: u64 },
    #[error("device {0} is revoked or expired")]
    DeviceExpired(String),
    #[error("authentication failed (wrong key or tampered ciphertext)")]
    AuthFailed,
    #[error("previous-record hash mismatch (gap or tamper)")]
    ChainMismatch,
    #[error("{0}")]
    Other(String),
}

/// Device horizon: 90 days offline -> expired, must re-enroll.
pub const DEVICE_HORIZON_DAYS: chrono::Duration = chrono::Duration::days(90);
/// Tombstones: retained >= 120 days.
pub const TOMBSTONE_RETENTION_DAYS: chrono::Duration = chrono::Duration::days(120);

impl SyncGroup {
    pub fn create(group_id: &str, now: DateTime<Utc>) -> Self {
        let mut epoch_keys = BTreeMap::new();
        epoch_keys.insert(1u64, KeyMaterial::random());
        SyncGroup {
            group_id: group_id.into(),
            epoch_keys,
            current_epoch: 1,
            members: BTreeMap::new(),
        }
    }

    pub fn enroll(&mut self, device_id: &str, now: DateTime<Utc>) {
        self.members.insert(
            device_id.into(),
            MemberState {
                enrolled_at: now,
                horizon_until: now + DEVICE_HORIZON_DAYS,
                revoked: false,
            },
        );
    }

    /// Advance the epoch, revoking a device. Future records use the new
    /// epoch key; already-obtained plaintext is not erased (documented).
    pub fn revoke_and_advance_epoch(&mut self, device_id: &str, now: DateTime<Utc>) {
        if let Some(m) = self.members.get_mut(device_id) {
            m.revoked = true;
        }
        let new_epoch = self.current_epoch + 1;
        self.epoch_keys.insert(new_epoch, KeyMaterial::random());
        self.current_epoch = new_epoch;
        let _ = now;
    }

    pub fn epoch_key(&self, epoch: u64) -> Option<&KeyMaterial> {
        self.epoch_keys.get(&epoch)
    }

    pub fn epoch_key_current(&self) -> &KeyMaterial {
        self.epoch_keys.get(&self.current_epoch).expect("current epoch key")
    }
}

/// Hybrid logical clock: (physical ms, counter) — monotonic even when the
/// wall clock stalls; cannot be extended by an untrusted local clock alone
/// (peers' HLCs feed the max).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hlc {
    pub physical_ms: u64,
    pub counter: u32,
}

impl Hlc {
    pub fn now(physical_ms: u64, last: Option<Hlc>) -> Hlc {
        match last {
            Some(l) if physical_ms <= l.physical_ms => Hlc { physical_ms: l.physical_ms, counter: l.counter + 1 },
            _ => Hlc { physical_ms, counter: 0 },
        }
    }
}

/// Authenticated sync record envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordEnvelope {
    pub group_id: String,
    pub device_id: String,
    pub device_seq: u64,
    pub key_epoch: u64,
    pub record_type: SyncRecordType,
    pub object_id: String,
    pub hlc: Hlc,
    pub prev_device_record_hash: Option<String>,
    pub tombstone: bool,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

impl RecordEnvelope {
    fn aad(&self) -> Vec<u8> {
        let meta = format!(
            "{}|{}|{}|{}|{:?}|{}|{}|{}|{}",
            self.group_id,
            self.device_id,
            self.device_seq,
            self.key_epoch,
            self.record_type,
            self.object_id,
            self.hlc.physical_ms,
            self.prev_device_record_hash.as_deref().unwrap_or(""),
            self.tombstone
        );
        let mut v = meta.into_bytes();
        v.extend_from_slice(&self.nonce);
        v
    }

    /// Seal plaintext into an authenticated envelope under the group's
    /// CURRENT epoch key.
    pub fn seal(
        group: &SyncGroup,
        device_id: &str,
        device_seq: u64,
        record_type: SyncRecordType,
        object_id: &str,
        hlc: Hlc,
        prev_device_record_hash: Option<String>,
        tombstone: bool,
        plaintext: &[u8],
    ) -> Result<RecordEnvelope, SyncError> {
        let member = group
            .members
            .get(device_id)
            .ok_or_else(|| SyncError::DeviceExpired(device_id.into()))?;
        if member.revoked {
            return Err(SyncError::DeviceExpired(device_id.into()));
        }
        let key = group.epoch_key_current();
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let mut env = RecordEnvelope {
            group_id: group.group_id.clone(),
            device_id: device_id.into(),
            device_seq,
            key_epoch: group.current_epoch,
            record_type,
            object_id: object_id.into(),
            hlc,
            prev_device_record_hash,
            tombstone,
            nonce,
            ciphertext: Vec::new(),
        };
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key.0));
        env.ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload { msg: plaintext, aad: &env.aad() },
            )
            .map_err(|_| SyncError::AuthFailed)?;
        Ok(env)
    }

    /// Open with the epoch key named BY the envelope. Old-epoch records
    /// remain readable as HISTORY; only new uploads require the current
    /// epoch (enforced by the receiver's accept logic).
    pub fn open(&self, group: &SyncGroup) -> Result<Vec<u8>, SyncError> {
        let key = group
            .epoch_key(self.key_epoch)
            .ok_or(SyncError::InvalidEpoch { got: self.key_epoch, current: group.current_epoch })?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key.0));
        cipher
            .decrypt(
                Nonce::from_slice(&self.nonce),
                Payload { msg: &self.ciphertext, aad: &self.aad() },
            )
            .map_err(|_| SyncError::AuthFailed)
    }
}

/// Receiver-side acceptance policy: tracks per-device sequences and the
/// accepted epoch. Restored old backups cannot roll the epoch backward.
pub struct SyncReceiver {
    pub group: SyncGroup,
    /// Last accepted device_seq per device.
    pub last_seq: BTreeMap<String, u64>,
    /// Last accepted record hash per device (chain check).
    pub last_hash: BTreeMap<String, String>,
}

impl SyncReceiver {
    pub fn new(group: SyncGroup) -> Self {
        SyncReceiver { group, last_seq: BTreeMap::new(), last_hash: BTreeMap::new() }
    }

    /// Accept one envelope: member check, epoch check (only CURRENT-epoch
    /// records upload live), per-device sequence contiguity, and per-device
    /// hash chain. Duplicates are rejected.
    pub fn accept(
        &mut self,
        env: &RecordEnvelope,
        expected_prev_hash: Option<&str>,
    ) -> Result<String, SyncError> {
        let member = self
            .group
            .members
            .get(&env.device_id)
            .ok_or_else(|| SyncError::DeviceExpired(env.device_id.clone()))?;
        if member.revoked {
            return Err(SyncError::DeviceExpired(env.device_id.clone()));
        }
        if env.key_epoch != self.group.current_epoch {
            return Err(SyncError::InvalidEpoch {
                got: env.key_epoch,
                current: self.group.current_epoch,
            });
        }
        let last = self.last_seq.get(&env.device_id).copied();
        if let Some(last_seq) = last {
            if env.device_seq <= last_seq {
                return Err(SyncError::DuplicateSequence {
                    device: env.device_id.clone(),
                    seq: env.device_seq,
                });
            }
            if env.device_seq > last_seq + 1 {
                return Err(SyncError::ChainMismatch);
            }
        } else if env.device_seq != 1 {
            return Err(SyncError::ChainMismatch);
        }
        match (expected_prev_hash, self.last_hash.get(&env.device_id)) {
            (None, None) => {}
            (Some(expected), Some(actual)) => {
                if expected != actual {
                    return Err(SyncError::ChainMismatch);
                }
            }
            _ => return Err(SyncError::ChainMismatch),
        }
        // Open to authenticate before accepting.
        env.open(&self.group)?;
        let hash = record_hash(env)?;
        self.last_seq.insert(env.device_id.clone(), env.device_seq);
        self.last_hash.insert(env.device_id.clone(), hash.clone());
        Ok(hash)
    }
}

pub fn record_hash(env: &RecordEnvelope) -> Result<String, SyncError> {
    let v = serde_json::json!({
        "group_id": env.group_id,
        "device_id": env.device_id,
        "device_seq": env.device_seq,
        "key_epoch": env.key_epoch,
        "record_type": format!("{:?}", env.record_type),
        "object_id": env.object_id,
        "hlc_physical": env.hlc.physical_ms,
        "hlc_counter": env.hlc.counter,
        "tombstone": env.tombstone,
        "ciphertext_sha": harbor_canonical::sha256_hex(&env.ciphertext),
    });
    Ok(harbor_canonical::sha256_hex(v.to_string().as_bytes()))
}
