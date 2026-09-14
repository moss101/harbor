//! Snapshot bundle transport: the sealed bulk-record download a
//! re-enrolling device performs before uploading (11_Sync_Protocol.md).

use chrono::{Duration, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use harbor_sync::bundle::{open_bundle, seal_bundle};
use harbor_sync::envelope::{
    Hlc, RecordEnvelope, SyncError, SyncGroup, SyncReceiver, SyncRecordType,
};

#[test]
fn reenrolling_device_downloads_live_tails_and_uploads_on_new_epoch() {
    let t0 = Utc::now();
    let mut group = SyncGroup::create("group-1", t0);
    group.enroll("device-a", t0);
    group.enroll("device-b", t0);

    // device-b produces live history on epoch 1.
    let tails: Vec<RecordEnvelope> = (1..=3)
        .map(|seq| {
            RecordEnvelope::seal(
                &group,
                "device-b",
                seq,
                SyncRecordType::RunHistory,
                &format!("obj-{seq}"),
                Hlc::now(1000 + seq, None),
                None,
                false,
                format!("history body {seq}").as_bytes(),
            )
            .unwrap()
        })
        .collect();

    // device-a goes offline 91 days: expired, epoch advances to 2. The
    // compliant device-b refreshed its enrollment at day 85 (horizon
    // extends to day 175), so it is unaffected.
    let t91 = t0 + Duration::days(91);
    group.expire_devices(t91);
    assert_eq!(group.current_epoch, 2);
    group.enroll("device-b", t0 + Duration::days(85));

    // device-a re-enrolls (authenticated approval delivers the current
    // epoch key); the issuing device seals the LIVE TAILS into a bundle.
    group.enroll("device-a", t91);
    let issuer_seed = [42u8; 32];
    let bundle = seal_bundle(&group, &issuer_seed, "device-b", t91, t91, &tails.to_vec()).unwrap();

    // NOTE: the tails were sealed under epoch 1 and are FILTERED OUT of a
    // live bundle (only current-epoch records ride it). The returning
    // device must instead download the current snapshot's records sealed
    // on epoch 2 — asserted here by the bundle being empty for epoch-1
    // tails.
    let opened = open_bundle(
        &group,
        &bundle,
        &VerifyingKey::from(&SigningKey::from_bytes(&issuer_seed)),
    )
    .unwrap();
    assert!(
        opened.is_empty(),
        "epoch-1 tails must not ride an epoch-2 bundle"
    );

    // The COMPLIANT device-b seals fresh epoch-2 records that DO ride.
    let fresh = RecordEnvelope::seal(
        &group,
        "device-b",
        1,
        SyncRecordType::RunHistory,
        "obj-new",
        Hlc::now(2000, None),
        None,
        false,
        b"epoch-2 record",
    )
    .unwrap();
    let bundle2 = seal_bundle(
        &group,
        &issuer_seed,
        "device-b",
        t91,
        t91,
        std::slice::from_ref(&fresh),
    )
    .unwrap();
    let opened = open_bundle(
        &group,
        &bundle2,
        &VerifyingKey::from(&SigningKey::from_bytes(&issuer_seed)),
    )
    .unwrap();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].open(&group).unwrap(), b"epoch-2 record");

    // The re-enrolled device-a seals on the CURRENT epoch like any member.
    let env = RecordEnvelope::seal(
        &group,
        "device-a",
        1,
        SyncRecordType::RunHistory,
        "obj",
        Hlc::now(3000, None),
        None,
        false,
        b"back online",
    )
    .unwrap();
    assert_eq!(env.key_epoch, 2);

    // A receiver accepts the transported record chain.
    let mut rx = SyncReceiver::new(group);
    let h1 = rx.accept(&fresh, None).unwrap();
    assert_eq!(h1.len(), 64);
    assert!(matches!(
        rx.accept(&fresh, Some(&h1)),
        Err(SyncError::DuplicateSequence { .. })
    ));
}

#[test]
fn stale_bundle_epoch_is_rejected_before_decryption() {
    let t0 = Utc::now();
    let mut group = SyncGroup::create("group-2", t0);
    group.enroll("device-b", t0);

    let issuer_seed = [7u8; 32];
    let bundle = seal_bundle(
        &group,
        &issuer_seed,
        "device-b",
        t0,
        t0,
        &[], // empty tails at epoch 1
    )
    .unwrap();

    // The group advances to epoch 2; the epoch-1 bundle is now stale.
    group.revoke_and_advance_epoch("device-b", t0 + Duration::days(1));
    assert!(matches!(
        open_bundle(
            &group,
            &bundle,
            &VerifyingKey::from(&SigningKey::from_bytes(&issuer_seed))
        ),
        Err(harbor_sync::bundle::BundleError::StaleBundle {
            bundle: 1,
            group: 2
        })
    ));
}

#[test]
fn bundle_carries_latest_state_per_object_with_tombstones() {
    let t0 = Utc::now();
    let mut group = SyncGroup::create("group-1", t0);
    group.enroll("device-b", t0);

    // Two versions of the same object plus a tombstone for another.
    let v1 = RecordEnvelope::seal(
        &group,
        "device-b",
        1,
        SyncRecordType::RunHistory,
        "doc-1",
        Hlc::now(100, None),
        None,
        false,
        b"version one",
    )
    .unwrap();
    let v2 = RecordEnvelope::seal(
        &group,
        "device-b",
        2,
        SyncRecordType::RunHistory,
        "doc-1",
        Hlc::now(200, None),
        None,
        false,
        b"version two",
    )
    .unwrap();
    let del = RecordEnvelope::seal(
        &group,
        "device-b",
        3,
        SyncRecordType::Tombstone,
        "doc-2",
        Hlc::now(300, None),
        None,
        true,
        b"",
    )
    .unwrap();

    let issuer_seed = [9u8; 32];
    let bundle = seal_bundle(&group, &issuer_seed, "device-b", t0, t0, &[v1, v2, del]).unwrap();

    let signer_public = VerifyingKey::from(&SigningKey::from_bytes(&issuer_seed));
    let opened = open_bundle(&group, &bundle, &signer_public).unwrap();
    // doc-1 collapsed to its latest version; the tombstone propagated.
    let doc1: Vec<&RecordEnvelope> = opened.iter().filter(|e| e.object_id == "doc-1").collect();
    assert_eq!(doc1.len(), 1, "latest per object only");
    assert_eq!(
        doc1[0].hlc,
        Hlc {
            physical_ms: 200,
            counter: 0
        }
    );
    assert_eq!(doc1[0].open(&group).unwrap(), b"version two");
    // Tombstone present and authenticated.
    let tomb = opened
        .iter()
        .find(|e| e.tombstone)
        .expect("tombstone rides");
    assert_eq!(tomb.object_id, "doc-2");
    assert!(tomb.open(&group).is_ok());
}
