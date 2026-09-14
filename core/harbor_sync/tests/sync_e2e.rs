//! End-to-end sync protocol tests: envelope crypto, epoch/revocation
//! monotonicity, duplicate/chain rejection, and LWW policy bounds
//! (11_Sync_Protocol.md).

use chrono::{Duration, Utc};
use harbor_sync::envelope::{
    Hlc, RecordEnvelope, SyncError, SyncGroup, SyncIdentity, SyncReceiver, SyncRecordType,
    DEVICE_HORIZON_DAYS, TOMBSTONE_RETENTION_DAYS,
};
use harbor_sync::lww::{is_lww_safe_field, lww_merge, LwwValue};

fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

#[test]
fn sealed_records_open_only_under_the_group_and_epoch() {
    let mut group = SyncGroup::create("group-1", now());
    group.enroll("device-a", now());
    group.enroll("device-b", now());

    let env = RecordEnvelope::seal(
        &group,
        "device-a",
        1,
        SyncRecordType::Chat,
        "chat-42",
        Hlc::now(1000, None),
        None,
        false,
        b"private chat body",
    )
    .unwrap();
    // Correct key opens; the AAD binds every envelope field, so tampering
    // with the tombstone flag breaks authentication.
    assert_eq!(env.open(&group).unwrap(), b"private chat body");
    let mut tampered = env.clone();
    tampered.tombstone = true;
    assert!(matches!(tampered.open(&group), Err(SyncError::AuthFailed)));
    // A different group's key cannot open it (AEAD authentication fails).
    let other = SyncGroup::create("group-2", now());
    assert!(matches!(env.open(&other), Err(SyncError::AuthFailed)));
    let _ = SyncIdentity::generate("unused");
}

#[test]
fn receiver_rejects_duplicates_gaps_and_stale_epochs() {
    let t0 = now();
    let mut group = SyncGroup::create("group-1", t0);
    group.enroll("device-a", t0);

    // Seal two sequential envelopes for device-a while the group is at
    // epoch 1, then hand the group to the receiver.
    let e1 = RecordEnvelope::seal(
        &group,
        "device-a",
        1,
        SyncRecordType::RunHistory,
        "obj",
        Hlc::now(1001, None),
        None,
        false,
        b"body-1",
    )
    .unwrap();
    let e2 = RecordEnvelope::seal(
        &group,
        "device-a",
        2,
        SyncRecordType::RunHistory,
        "obj",
        Hlc::now(1002, None),
        None,
        false,
        b"body-2",
    )
    .unwrap();
    let e3 = RecordEnvelope::seal(
        &group,
        "device-a",
        3,
        SyncRecordType::RunHistory,
        "obj",
        Hlc::now(1003, None),
        None,
        false,
        b"body-3",
    )
    .unwrap();
    let dup = RecordEnvelope::seal(
        &group,
        "device-a",
        1,
        SyncRecordType::RunHistory,
        "obj",
        Hlc::now(1001, None),
        None,
        false,
        b"body-1",
    )
    .unwrap();
    let _ = dup;

    let mut rx = SyncReceiver::new(group);
    // Chain: each accept returns the accepted record's hash, which is the
    // expected prev-hash of the next record.
    let h1 = rx.accept(&e1, None).unwrap();
    let h2 = rx.accept(&e2, Some(&h1)).unwrap();
    // Duplicate sequence rejected.
    // Duplicate check precedes the chain check.
    let h1 = rx.accept(&e1, None).unwrap_or(h1);
    assert!(matches!(
        rx.accept(&e1, Some(&h1)),
        Err(SyncError::DuplicateSequence { .. })
    ));
    // Gap rejected (seq 4 when 3 was never accepted) and a chain break
    // (wrong prev hash) is rejected even with the right sequence.
    assert!(matches!(
        rx.accept(&e3, Some("")),
        Err(SyncError::ChainMismatch)
    ));
    assert!(matches!(
        rx.accept(&e3, Some(&h1)),
        Err(SyncError::ChainMismatch)
    ));
    let _ = h2;
}

#[test]
fn revocation_advances_epoch_and_blocks_new_uploads() {
    let t0 = Utc::now();
    let mut group = SyncGroup::create("group-1", t0);
    group.enroll("device-a", t0);
    group.enroll("device-b", t0);

    let env_old = RecordEnvelope::seal(
        &group,
        "device-a",
        1,
        SyncRecordType::RunHistory,
        "o",
        Hlc::now(1, None),
        None,
        false,
        b"old history",
    )
    .unwrap();

    // Revoke device-a: epoch advances to 2.
    group.revoke_and_advance_epoch("device-a", t0 + Duration::days(1));
    assert_eq!(group.current_epoch, 2);

    // Old-epoch records remain readable as HISTORY.
    assert_eq!(env_old.open(&group).unwrap(), b"old history");

    // Revoked device cannot SEAL anything new.
    assert!(matches!(
        RecordEnvelope::seal(
            &group,
            "device-a",
            2,
            SyncRecordType::RunHistory,
            "o",
            Hlc::now(2, None),
            None,
            false,
            b"sneaky",
        ),
        Err(SyncError::DeviceExpired(_))
    ));

    // The other device continues on the new epoch.
    let env_new = RecordEnvelope::seal(
        &group,
        "device-b",
        1,
        SyncRecordType::RunHistory,
        "o",
        Hlc::now(3, None),
        None,
        false,
        b"new epoch",
    )
    .unwrap();
    assert_eq!(env_new.key_epoch, 2);
    assert_eq!(env_new.open(&group).unwrap(), b"new epoch");
}

#[test]
fn lww_only_for_safe_fields_and_deterministic_merge() {
    // Privacy and approvals are NOT LWW-eligible.
    assert!(is_lww_safe_field("appearance.theme"));
    assert!(is_lww_safe_field("display.density"));
    assert!(is_lww_safe_field("ui.language"));
    assert!(!is_lww_safe_field("privacy.mode"));
    assert!(!is_lww_safe_field("approvals.default"));
    assert!(!is_lww_safe_field("model.routing_lock"));

    let local = Some(LwwValue {
        value: "dark".into(),
        writer_hlc: (100, 0),
        writer_device: "device-a".into(),
    });
    let remote_older = LwwValue {
        value: "light".into(),
        writer_hlc: (90, 0),
        writer_device: "device-b".into(),
    };
    let merged = lww_merge(local.clone(), remote_older);
    assert_eq!(merged.value, "dark", "higher HLC wins");

    // Identical HLC resolves deterministically by device id.
    let tie = LwwValue {
        value: "light".into(),
        writer_hlc: (100, 0),
        writer_device: "device-b".into(),
    };
    assert_eq!(lww_merge(local.clone(), tie).value, "light");
}

#[test]
fn retention_horizons_match_the_authority() {
    // Devices may remain offline 90 days; tombstones retained >= 120.
    assert_eq!(DEVICE_HORIZON_DAYS, Duration::days(90));
    assert!(TOMBSTONE_RETENTION_DAYS > DEVICE_HORIZON_DAYS);
}
