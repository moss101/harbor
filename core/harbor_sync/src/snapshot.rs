//! Snapshots, device expiry and deletion watermarks
//! (11_Sync_Protocol.md "Offline and restore").
//!
//! - Devices may remain offline for 90 days. Beyond the horizon a device
//!   is EXPIRED: it must re-enroll and download a current snapshot before
//!   uploading changes.
//! - Expiring a device advances the group epoch (same rules as revocation).
//! - A device cannot extend its own horizon using an untrusted local clock:
//!   expiry is evaluated against the GROUP's clock, not the device's.
//! - Snapshots are signed by a device identity signing key; the signature
//!   binds group, epoch, watermark and issue time. Restoring a snapshot
//!   older than the group's current epoch is rejected.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use harbor_store::keys::KeyMaterial;

use crate::envelope::SyncGroup;

/// A signed, restorable group snapshot (`harbor.sync_snapshot/v1`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSnapshot {
    pub schema: String,
    pub group_id: String,
    /// The group's epoch AT ISSUE TIME. A restored snapshot never advances
    /// or rolls back the live epoch; it certifies state as of this epoch.
    pub epoch: u64,
    pub issued_at: DateTime<Utc>,
    /// Deletion watermark: tombstones newer than this instant may still
    /// exist in record tails; everything older is compacted.
    pub deletion_watermark: DateTime<Utc>,
    pub signer_device_id: String,
    pub signature: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("signature verification failed")]
    BadSignature,
    #[error("snapshot epoch {snapshot} is older than the group epoch {group}")]
    StaleSnapshot { snapshot: u64, group: u64 },
    #[error("snapshot is for group {snapshot_group}, not {expected}")]
    WrongGroup {
        snapshot_group: String,
        expected: String,
    },
    #[error("device {0} is expired (horizon {1}); re-enrollment required")]
    DeviceExpired(String, DateTime<Utc>),
}

impl SyncGroup {
    /// Group-authoritative expiry: members past their horizon are expired
    /// and the group epoch ADVANCES (same rules as revocation).
    /// Returns the ids of newly expired devices.
    pub fn expire_devices(&mut self, group_now: DateTime<Utc>) -> Vec<String> {
        let mut expired = Vec::new();
        for (id, m) in self.members.iter_mut() {
            if !m.revoked && group_now > m.horizon_until {
                m.revoked = true;
                expired.push(id.clone());
            }
        }
        if !expired.is_empty() {
            let new_epoch = self.current_epoch + 1;
            self.epoch_keys.insert(new_epoch, KeyMaterial::random());
            self.current_epoch = new_epoch;
        }
        expired
    }

    /// Authoritative expiry check evaluated with the GROUP clock — a device
    /// cannot extend its own horizon with an untrusted local clock.
    pub fn member_expired(&self, device_id: &str, group_now: DateTime<Utc>) -> bool {
        match self.members.get(device_id) {
            None => true,
            Some(m) => m.revoked || group_now > m.horizon_until,
        }
    }
}

fn snapshot_bytes(snap: &SyncSnapshot) -> Vec<u8> {
    let canonical = format!(
        "{}|{}|{}|{}|{}|{}",
        snap.schema,
        snap.group_id,
        snap.epoch,
        snap.issued_at.to_rfc3339(),
        snap.deletion_watermark.to_rfc3339(),
        snap.signer_device_id
    );
    Sha256::digest(canonical.as_bytes()).to_vec()
}

/// Sign a snapshot with a device identity signing key (seed). The signature
/// binds group, epoch, watermark and issue time.
pub fn sign_snapshot(
    signing_seed: &[u8; 32],
    signer_device_id: &str,
    group: &SyncGroup,
    watermark: DateTime<Utc>,
    issued_at: DateTime<Utc>,
) -> SyncSnapshot {
    use ed25519_dalek::SigningKey;
    let signing = SigningKey::from_bytes(signing_seed);
    let mut snap = SyncSnapshot {
        schema: "harbor.sync_snapshot/v1".into(),
        group_id: group.group_id.clone(),
        epoch: group.current_epoch,
        issued_at,
        deletion_watermark: watermark,
        signer_device_id: signer_device_id.into(),
        signature: String::new(),
    };
    let sig = signing.sign(&snapshot_bytes(&snap));
    snap.signature = hex::encode(sig.to_bytes());
    snap
}

/// Verify a snapshot signature against the signer's public key.
pub fn verify_snapshot(
    snap: &SyncSnapshot,
    signer_public: &VerifyingKey,
) -> Result<(), SnapshotError> {
    let bytes = snapshot_bytes(snap);
    let sig_bytes = hex::decode(&snap.signature).map_err(|_| SnapshotError::BadSignature)?;
    let arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SnapshotError::BadSignature)?;
    let sig = Signature::from_bytes(&arr);
    signer_public
        .verify(&bytes, &sig)
        .map_err(|_| SnapshotError::BadSignature)
}

/// Restoring a snapshot into a live group: the snapshot's epoch must not
/// be older than the group's current epoch (old backups never roll the
/// epoch backward), and the group ids must match.
pub fn validate_restore(
    snap: &SyncSnapshot,
    live_group_id: &str,
    live_epoch: u64,
) -> Result<(), SnapshotError> {
    if snap.group_id != live_group_id {
        return Err(SnapshotError::WrongGroup {
            snapshot_group: snap.group_id.clone(),
            expected: live_group_id.into(),
        });
    }
    if snap.epoch < live_epoch {
        return Err(SnapshotError::StaleSnapshot {
            snapshot: snap.epoch,
            group: live_epoch,
        });
    }
    Ok(())
}
