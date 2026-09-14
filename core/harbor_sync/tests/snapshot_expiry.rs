//! Snapshot, expiry and restore semantics (11_Sync_Protocol.md
//! "Offline and restore" + "Offline limits").

use chrono::{Duration, Utc};
use ed25519_dalek::VerifyingKey;
use harbor_sync::envelope::{RecordEnvelope, SyncError, SyncGroup, SyncIdentity, SyncRecordType};
use harbor_sync::snapshot::{sign_snapshot, verify_snapshot, SnapshotError};

#[test]
fn device_past_horizon_expires_epoch_advances_and_uploads_blocked() {
    let t0 = Utc::now();
    let mut group = SyncGroup::create("g", t0);
    group.enroll("gone", t0);
    // The compliant device enrolls 85 days later: still inside the horizon
    // when 91 days have passed since group creation.
    group.enroll("home", t0 + Duration::days(85));

    // 91 days pass at group time.
    let later = t0 + Duration::days(91);
    let expired = group.expire_devices(later);
    assert_eq!(expired, vec!["gone".to_string()]);
    assert_eq!(group.current_epoch, 2, "expiry advances the epoch");

    // The expired device cannot upload: receiver membership is revoked.
    let env = RecordEnvelope::seal(
        &group,
        "gone",
        1,
        SyncRecordType::RunHistory,
        "o",
        harbor_sync::envelope::Hlc::now(1, None),
        None,
        false,
        b"x",
    );
    assert!(matches!(env, Err(SyncError::DeviceExpired(_))));

    // The compliant device continues on the new epoch.
    let env2 = RecordEnvelope::seal(
        &group,
        "home",
        1,
        SyncRecordType::RunHistory,
        "o",
        harbor_sync::envelope::Hlc::now(2, None),
        None,
        false,
        b"y",
    )
    .unwrap();
    assert_eq!(env2.key_epoch, 2);
}

#[test]
fn expired_device_cannot_extend_its_own_horizon() {
    let t0 = Utc::now();
    let mut group = SyncGroup::create("g", t0);
    group.enroll("drift", t0);
    // A device with a skewed local clock claims it is still within the
    // horizon; the GROUP clock decides.
    assert!(!group.member_expired("drift", t0 + Duration::days(10)));
    assert!(group.member_expired("drift", t0 + Duration::days(91)));
}

#[test]
fn snapshots_are_signed_and_stale_restore_is_rejected() {
    use harbor_sync::snapshot::validate_restore;
    let t0 = Utc::now();
    let mut group = SyncGroup::create("g", t0);
    group.enroll("device-a", t0);

    // Issue a snapshot at epoch 1 signed by device-a's identity key.
    let identity = SyncIdentity::generate("device-a");
    let snap = sign_snapshot(&identity.signing_seed, "device-a", &group, t0, t0);
    assert_eq!(snap.epoch, 1);
    assert_eq!(snap.schema, "harbor.sync_snapshot/v1");

    // Verify against the matching public key.
    let seed = identity.signing_seed;
    let vk = VerifyingKey::from(&ed25519_dalek::SigningKey::from_bytes(&seed));
    verify_snapshot(&snap, &vk).unwrap();

    // Tampered watermark breaks the signature.
    let mut tampered = snap.clone();
    tampered.deletion_watermark = t0 + Duration::days(200);
    assert!(matches!(
        verify_snapshot(&tampered, &vk),
        Err(SnapshotError::BadSignature)
    ));

    // Restoring an OLD snapshot cannot roll the epoch backward.
    group.revoke_and_advance_epoch("device-a", t0 + Duration::days(1));
    assert_eq!(group.current_epoch, 2);
    assert!(matches!(
        validate_restore(&snap, "g", group.current_epoch),
        Err(SnapshotError::StaleSnapshot {
            snapshot: 1,
            group: 2
        })
    ));

    // Wrong group rejected.
    assert!(matches!(
        validate_restore(&snap, "other", 1),
        Err(SnapshotError::WrongGroup { .. })
    ));
}
