//! Approval receipts (`harbor.approval/v1`).
//!
//! A receipt is the durable, device-bound authorization for exactly one
//! protected effect (or one artifact batch). Semantics ported from
//! `contracts.py::validate_approval_binding`:
//! - lifetime is positive and at most 15 minutes;
//! - consumption must fall inside validity;
//! - denied / expired / revoked / consumed / post-termination receipts
//!   cannot authorize a new dispatch;
//! - receipts bind run, effect, generation, canonical args hash, target,
//!   effect class and policy version; artifact writes additionally bind
//!   batch id, approved base hash and proposed output hash.

use chrono::{DateTime, Duration, Utc};

use crate::effect::EffectClass;
use crate::ids::{validate_hash, HarborId};
use crate::policy::ExecutionLocation; // re-export use keeps policy visible

pub const MAX_RECEIPT_LIFETIME: Duration = Duration::minutes(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Artifact,
    File,
    ConnectorObject,
    RemoteDestination,
    WorkspaceSetting,
}

impl TargetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetKind::Artifact => "artifact",
            TargetKind::File => "file",
            TargetKind::ConnectorObject => "connector_object",
            TargetKind::RemoteDestination => "remote_destination",
            TargetKind::WorkspaceSetting => "workspace_setting",
        }
    }
}

/// The exact canonical target a receipt authorizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub kind: TargetKind,
    pub identity: String,
    pub capability_id: HarborId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationSource {
    AllowOnce,
    PersistentPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Denied,
}

/// A durable approval receipt. Issued only from authoritative (encrypted,
/// device-bound) storage; an untrusted JSON payload alone never establishes
/// authenticity.
#[derive(Debug, Clone)]
pub struct ApprovalReceipt {
    pub receipt_id: HarborId,
    pub run_id: HarborId,
    pub effect_id: HarborId,
    pub device_id: String,
    pub executor_generation: u64,
    pub effect_class: EffectClass,
    /// Lowercase SHA-256 of the canonical arguments.
    pub canonical_args_hash: String,
    pub target: Target,
    pub policy_version: String,
    pub authorization_source: AuthorizationSource,
    pub permission_record_id: HarborId,
    pub decision: Decision,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    /// Optional artifact-batch binding (required for file_write effects).
    pub batch_binding: Option<BatchBinding>,
}

/// Artifact-batch-specific receipt binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchBinding {
    pub batch_id: HarborId,
    pub base_content_hash: String,
    pub proposed_output_hash: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ReceiptError {
    #[error("receipt lifetime must be positive and at most 15 minutes")]
    Lifetime,
    #[error("consumption outside receipt validity")]
    ConsumedOutsideValidity,
    #[error("receipt is denied")]
    Denied,
    #[error("receipt already consumed at {0}")]
    Consumed(DateTime<Utc>),
    #[error("receipt expired at {0}")]
    Expired(DateTime<Utc>),
    #[error("receipt not yet valid (issued at {0})")]
    NotYetValid(DateTime<Utc>),
    #[error("binding mismatch: {0}")]
    Binding(&'static str),
    #[error("run terminated before receipt validity")]
    Terminated,
    #[error("invalid hash: {0}")]
    Hash(String),
    #[error("artifact approval requires a batch binding")]
    MissingBatch,
}

impl ApprovalReceipt {
    /// Validate structural receipt invariants (lifetime and consumption).
    pub fn validate_invariants(&self) -> Result<(), ReceiptError> {
        if !(self.issued_at < self.expires_at
            && self.expires_at - self.issued_at <= MAX_RECEIPT_LIFETIME)
        {
            return Err(ReceiptError::Lifetime);
        }
        if let Some(consumed) = self.consumed_at {
            if !(self.issued_at <= consumed && consumed < self.expires_at) {
                return Err(ReceiptError::ConsumedOutsideValidity);
            }
        }
        validate_hash(&self.canonical_args_hash)
            .map_err(|_| ReceiptError::Hash("canonical_args_hash".into()))?;
        if let Some(b) = &self.batch_binding {
            validate_hash(&b.base_content_hash)
                .map_err(|_| ReceiptError::Hash("base_content_hash".into()))?;
            validate_hash(&b.proposed_output_hash)
                .map_err(|_| ReceiptError::Hash("proposed_output_hash".into()))?;
        }
        Ok(())
    }

    /// May this receipt authorize a dispatch right now, given device,
    /// executor generation and run termination?
    pub fn check_authority(
        &self,
        device_id: &str,
        generation: u64,
        now: DateTime<Utc>,
        run_terminated: bool,
    ) -> Result<(), ReceiptError> {
        self.validate_invariants()?;
        if self.decision != Decision::Approved {
            return Err(ReceiptError::Denied);
        }
        if let Some(consumed_at) = self.consumed_at {
            return Err(ReceiptError::Consumed(consumed_at));
        }
        if run_terminated {
            return Err(ReceiptError::Terminated);
        }
        if self.device_id != device_id {
            return Err(ReceiptError::Binding("device"));
        }
        if self.executor_generation != generation {
            return Err(ReceiptError::Binding("executor_generation"));
        }
        if now < self.issued_at {
            return Err(ReceiptError::NotYetValid(self.issued_at));
        }
        if now >= self.expires_at {
            return Err(ReceiptError::Expired(self.expires_at));
        }
        Ok(())
    }

    /// Check the full effect-binding: the receipt must match every durable
    /// field of the effect intent it authorizes.
    #[allow(clippy::too_many_arguments)] // wide durable bindings are the contract here
    pub fn check_effect_binding(
        &self,
        run_id: &HarborId,
        effect_id: &HarborId,
        effect_class: EffectClass,
        canonical_args_hash: &str,
        target: &Target,
        policy_version: &str,
        batch: Option<(&HarborId, &str, &str)>,
    ) -> Result<(), ReceiptError> {
        if &self.run_id != run_id {
            return Err(ReceiptError::Binding("run_id"));
        }
        if &self.effect_id != effect_id {
            return Err(ReceiptError::Binding("effect_id"));
        }
        if self.effect_class != effect_class {
            return Err(ReceiptError::Binding("effect_class"));
        }
        if self.canonical_args_hash != canonical_args_hash {
            return Err(ReceiptError::Binding("canonical_args_hash"));
        }
        if &self.target != target {
            return Err(ReceiptError::Binding("target"));
        }
        if self.policy_version != policy_version {
            return Err(ReceiptError::Binding("policy_version"));
        }
        if effect_class == EffectClass::FileWrite {
            let b = batch.ok_or(ReceiptError::MissingBatch)?;
            let binding = self
                .batch_binding
                .as_ref()
                .ok_or(ReceiptError::MissingBatch)?;
            if &binding.batch_id != b.0 {
                return Err(ReceiptError::Binding("batch_id"));
            }
            if binding.base_content_hash != b.1 {
                return Err(ReceiptError::Binding("base_content_hash"));
            }
            if binding.proposed_output_hash != b.2 {
                return Err(ReceiptError::Binding("proposed_output_hash"));
            }
        }
        Ok(())
    }

    /// Consume the receipt (single use). Consumption and dispatch
    /// preparation must commit together; callers persist the update.
    pub fn consume(&mut self, now: DateTime<Utc>) -> Result<(), ReceiptError> {
        self.check_authority(&self.device_id, self.executor_generation, now, false)?;
        self.consumed_at = Some(now);
        Ok(())
    }

    /// Execution location indicator for the Trust Pulse.
    pub fn location_hint(&self) -> ExecutionLocation {
        ExecutionLocation::OnDevice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn lifetime_bounds() {
        let mut r = receipt();
        r.expires_at = r.issued_at + Duration::minutes(16);
        assert!(matches!(
            r.validate_invariants(),
            Err(ReceiptError::Lifetime)
        ));
        r.expires_at = r.issued_at + Duration::seconds(0);
        assert!(matches!(
            r.validate_invariants(),
            Err(ReceiptError::Lifetime)
        ));
    }

    #[test]
    fn authority_checks_generation_device_and_expiry() {
        let r = receipt();
        assert!(r.check_authority("device-1", 3, Utc::now(), false).is_ok());
        assert!(matches!(
            r.check_authority("device-1", 4, Utc::now(), false),
            Err(ReceiptError::Binding("executor_generation"))
        ));
        assert!(matches!(
            r.check_authority("device-2", 3, Utc::now(), false),
            Err(ReceiptError::Binding("device"))
        ));
        assert!(matches!(
            r.check_authority("device-1", 3, Utc::now(), true),
            Err(ReceiptError::Terminated)
        ));
        let later = r.issued_at + Duration::minutes(11);
        assert!(matches!(
            r.check_authority("device-1", 3, later, false),
            Err(ReceiptError::Expired(_))
        ));
    }

    #[test]
    fn single_consumption() {
        let mut r = receipt();
        let now = Utc::now();
        r.consume(now).unwrap();
        assert!(r.consumed_at.is_some());
        let second = r.clone();
        assert!(matches!(
            second.check_authority("device-1", 3, now, false),
            Err(ReceiptError::Consumed(_))
        ));
    }

    #[test]
    fn denied_receipt_cannot_authorize() {
        let mut r = receipt();
        r.decision = Decision::Denied;
        assert!(matches!(
            r.check_authority("device-1", 3, Utc::now(), false),
            Err(ReceiptError::Denied)
        ));
    }

    #[test]
    fn file_write_requires_matching_batch() {
        let mut r = receipt();
        r.effect_class = EffectClass::FileWrite;
        r.batch_binding = Some(BatchBinding {
            batch_id: HarborId::new("batch-1").unwrap(),
            base_content_hash: "11".repeat(32),
            proposed_output_hash: "22".repeat(32),
        });
        // No batch at all -> error.
        assert!(matches!(
            r.check_effect_binding(
                &r.run_id,
                &r.effect_id,
                EffectClass::FileWrite,
                &r.canonical_args_hash,
                &r.target,
                &r.policy_version,
                None
            ),
            Err(ReceiptError::MissingBatch)
        ));
        // Matching batch -> ok.
        assert!(r
            .check_effect_binding(
                &r.run_id,
                &r.effect_id,
                EffectClass::FileWrite,
                &r.canonical_args_hash,
                &r.target,
                &r.policy_version,
                Some((
                    &HarborId::new("batch-1").unwrap(),
                    &"11".repeat(32),
                    &"22".repeat(32)
                ))
            )
            .is_ok());
        // Changed base hash -> mismatch.
        assert!(matches!(
            r.check_effect_binding(
                &r.run_id,
                &r.effect_id,
                EffectClass::FileWrite,
                &r.canonical_args_hash,
                &r.target,
                &r.policy_version,
                Some((
                    &HarborId::new("batch-1").unwrap(),
                    &"33".repeat(32),
                    &"22".repeat(32)
                ))
            ),
            Err(ReceiptError::Binding("base_content_hash"))
        ));
    }
}
