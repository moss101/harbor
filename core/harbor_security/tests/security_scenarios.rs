//! Security scenario SEC-008 (09_Security_Test_Matrix.csv) as an
//! executable control: protected effect replay.
//! `security.sec_008` — the approval receipt is bound to the effect's
//! canonical arguments, target and policy before dispatch, and is
//! single-use: a replayed effect cannot present a consumed receipt, and
//! tampered arguments break the binding.

use chrono::{Duration, Utc};
use harbor_security::receipt::{
    ApprovalReceipt, AuthorizationSource, Decision, Target, TargetKind,
};
use harbor_security::{EffectClass, HarborId};

fn target() -> Target {
    Target {
        kind: TargetKind::File,
        identity: "/Users/test/report.docx".into(),
        capability_id: HarborId::new("cap.file.write").unwrap(),
    }
}

fn receipt() -> ApprovalReceipt {
    ApprovalReceipt {
        receipt_id: HarborId::generate("rcpt"),
        run_id: HarborId::generate("run"),
        effect_id: HarborId::generate("fx"),
        device_id: "device-1".into(),
        executor_generation: 3,
        effect_class: EffectClass::ConnectorWrite,
        canonical_args_hash: "ab".repeat(32),
        target: target(),
        policy_version: "policy-2026.09".into(),
        authorization_source: AuthorizationSource::AllowOnce,
        permission_record_id: HarborId::new("perm-1").unwrap(),
        decision: Decision::Approved,
        issued_at: Utc::now(),
        expires_at: Utc::now() + Duration::minutes(10),
        consumed_at: None,
        batch_binding: None,
    }
}

/// SEC-008 part 1: a receipt authorizes ONE dispatch. After the effect
/// runs, the receipt is consumed and a replay is refused.
#[test]
fn sec_008_replayed_effect_cannot_reuse_a_consumed_receipt() {
    let mut r = receipt();
    let now = Utc::now();
    r.check_authority("device-1", 3, now, false).unwrap();
    r.consume(now).unwrap();
    let err = r.check_authority("device-1", 3, now, false).unwrap_err();
    assert!(
        matches!(err, harbor_security::receipt::ReceiptError::Consumed(_)),
        "a replayed effect must find its receipt spent: {err}"
    );
}

/// SEC-008 part 2: the binding covers the effect's durable identity —
/// tampered canonical arguments, a swapped effect id, or a different
/// target each break it.
#[test]
fn sec_008_tampered_arguments_break_the_receipt_binding() {
    let r = receipt();
    let ok = |args: &str| {
        r.check_effect_binding(
            &r.run_id,
            &r.effect_id,
            EffectClass::ConnectorWrite,
            args,
            &target(),
            "policy-2026.09",
            None,
        )
    };
    ok(&"ab".repeat(32)).unwrap();
    // Tampered arguments (injection after approval).
    let err = ok(&"cd".repeat(32)).unwrap_err();
    assert!(matches!(
        err,
        harbor_security::receipt::ReceiptError::Binding("canonical_args_hash")
    ));
    // A different effect cannot wear this receipt.
    let other = HarborId::generate("fx");
    let err = r
        .check_effect_binding(
            &r.run_id,
            &other,
            EffectClass::ConnectorWrite,
            &"ab".repeat(32),
            &target(),
            "policy-2026.09",
            None,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        harbor_security::receipt::ReceiptError::Binding("effect_id")
    ));
    // A different target (the classic replay-at-another-file).
    let mut other_target = target();
    other_target.identity = "/Users/test/other.docx".into();
    let err = r
        .check_effect_binding(
            &r.run_id,
            &r.effect_id,
            EffectClass::ConnectorWrite,
            &"ab".repeat(32),
            &other_target,
            "policy-2026.09",
            None,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        harbor_security::receipt::ReceiptError::Binding("target")
    ));
}

/// SEC-008 part 3: expiry and termination revoke authority outright.
#[test]
fn sec_008_expired_or_terminated_receipts_refuse_dispatch() {
    let r = receipt();
    let later = Utc::now() + Duration::minutes(30);
    assert!(matches!(
        r.check_authority("device-1", 3, later, false),
        Err(harbor_security::receipt::ReceiptError::Expired(_))
    ));
    assert!(matches!(
        r.check_authority("device-1", 3, Utc::now(), true),
        Err(harbor_security::receipt::ReceiptError::Terminated)
    ));
}
