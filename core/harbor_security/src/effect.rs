//! Protected-effect intents (`harbor.effect/v3`): durable records of
//! externally visible actions with a closed transition graph.
//!
//! States: prepared -> dispatched -> committed, with outcome_unknown and
//! aborted branches. An effect whose outcome cannot be established stays
//! `outcome_unknown` and is never automatically retried (02 contract §5).

use chrono::{DateTime, Utc};

use crate::ids::{validate_hash, HarborId};
use crate::receipt::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectClass {
    FileWrite,
    ConnectorWrite,
    NetworkSend,
    Delete,
    Export,
    SettingsWrite,
}

impl EffectClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            EffectClass::FileWrite => "file_write",
            EffectClass::ConnectorWrite => "connector_write",
            EffectClass::NetworkSend => "network_send",
            EffectClass::Delete => "delete",
            EffectClass::Export => "export",
            EffectClass::SettingsWrite => "settings_write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectState {
    Prepared,
    Dispatched,
    Committed,
    OutcomeUnknown,
    Aborted,
}

impl EffectState {
    pub fn as_str(&self) -> &'static str {
        match self {
            EffectState::Prepared => "prepared",
            EffectState::Dispatched => "dispatched",
            EffectState::Committed => "committed",
            EffectState::OutcomeUnknown => "outcome_unknown",
            EffectState::Aborted => "aborted",
        }
    }

    /// Closed transition graph from the contract.
    pub fn can_transition_to(self, next: EffectState) -> bool {
        use EffectState::*;
        matches!(
            (self, next),
            (Prepared, Dispatched)
                | (Prepared, Aborted)
                | (Dispatched, Committed)
                | (Dispatched, OutcomeUnknown)
                | (Dispatched, Aborted)
                | (OutcomeUnknown, Committed)
                | (OutcomeUnknown, Aborted)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyMode {
    /// Provider idempotency with a persisted key; usable within its
    /// retention guarantee.
    ProviderKey,
    /// Versioned adapter + reliable read reconciliation.
    Reconcile,
    /// Neither available: dispatch is prohibited when uncertainty arises.
    None,
}

/// A durable effect intent. Immutable fields never change after creation;
/// updates may only alter state, attempt/result references and timestamps.
#[derive(Debug, Clone)]
pub struct EffectIntent {
    pub effect_id: HarborId,
    pub run_id: HarborId,
    pub tool: HarborId,
    pub effect_class: EffectClass,
    pub canonical_args: harbor_canonical::JsonValue,
    pub canonical_args_hash: String,
    pub target: Target,
    pub policy_version: String,
    pub approval_receipt_id: Option<HarborId>,
    pub executor_generation: u64,
    pub state: EffectState,
    pub idempotency_mode: IdempotencyMode,
    pub attempt_id: Option<String>,
    pub provider_result_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error("illegal effect transition {from} -> {to}")]
    IllegalTransition {
        from: &'static str,
        to: &'static str,
    },
    #[error("immutable effect field changed: {0}")]
    ImmutableChanged(&'static str),
    #[error("update timestamp regresses")]
    TimestampRegresses,
    #[error("dispatched effect requires an attempt id")]
    MissingAttemptId,
    #[error("provider_key idempotency requires a persisted key reference")]
    MissingProviderKey,
    #[error("reconcile idempotency requires a versioned adapter identity")]
    MissingReconcileIdentity,
    #[error("effect update regression: {0}")]
    Other(&'static str),
}

impl EffectIntent {
    /// Create a prepared intent, computing the canonical args hash.
    #[allow(clippy::too_many_arguments)] // wide durable bindings are the contract here
    pub fn prepare(
        run_id: HarborId,
        tool: HarborId,
        effect_class: EffectClass,
        canonical_args: harbor_canonical::JsonValue,
        target: Target,
        policy_version: impl Into<String>,
        executor_generation: u64,
        now: DateTime<Utc>,
    ) -> Result<Self, EffectError> {
        let hash = canonical_args
            .canonical_sha256()
            .map_err(|_| EffectError::Other("canonical args hashing failed"))?;
        Ok(EffectIntent {
            effect_id: HarborId::generate("fx"),
            run_id,
            tool,
            effect_class,
            canonical_args_hash: hash,
            canonical_args,
            target,
            policy_version: policy_version.into(),
            approval_receipt_id: None,
            executor_generation,
            state: EffectState::Prepared,
            idempotency_mode: IdempotencyMode::None,
            attempt_id: None,
            provider_result_ref: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Apply a state/reference update enforcing immutability, the closed
    /// transition graph, and timestamp monotonicity (contracts.py
    /// `validate_effect_update`).
    pub fn apply_update(
        &self,
        next_state: EffectState,
        attempt_id: Option<String>,
        provider_result_ref: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<EffectIntent, EffectError> {
        if !self.state.can_transition_to(next_state) {
            return Err(EffectError::IllegalTransition {
                from: self.state.as_str(),
                to: next_state.as_str(),
            });
        }
        if now < self.updated_at {
            return Err(EffectError::TimestampRegresses);
        }
        if next_state == EffectState::Dispatched && attempt_id.is_none() {
            return Err(EffectError::MissingAttemptId);
        }
        Ok(EffectIntent {
            effect_id: self.effect_id.clone(),
            run_id: self.run_id.clone(),
            tool: self.tool.clone(),
            effect_class: self.effect_class,
            canonical_args: self.canonical_args.clone(),
            canonical_args_hash: self.canonical_args_hash.clone(),
            target: self.target.clone(),
            policy_version: self.policy_version.clone(),
            approval_receipt_id: self.approval_receipt_id.clone(),
            executor_generation: self.executor_generation,
            state: next_state,
            idempotency_mode: self.idempotency_mode,
            attempt_id,
            provider_result_ref,
            created_at: self.created_at,
            updated_at: now,
        })
    }

    /// Verify the recorded hash still matches the canonical arguments.
    pub fn verify_args_integrity(&self) -> Result<(), EffectError> {
        let got = self
            .canonical_args
            .canonical_sha256()
            .map_err(|_| EffectError::Other("canonical re-encode failed"))?;
        if got == self.canonical_args_hash && validate_hash(&self.canonical_args_hash).is_ok() {
            Ok(())
        } else {
            Err(EffectError::ImmutableChanged("canonical_args"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::TargetKind;

    fn target() -> Target {
        Target {
            kind: TargetKind::ConnectorObject,
            identity: "mail/message/outbox-1".into(),
            capability_id: HarborId::new("cap.mail.send").unwrap(),
        }
    }

    fn intent() -> EffectIntent {
        EffectIntent::prepare(
            HarborId::generate("run"),
            HarborId::new("tool.mail.send").unwrap(),
            EffectClass::ConnectorWrite,
            harbor_canonical::parse(r#"{"to":"a@example.com","subject":"hi"}"#).unwrap(),
            target(),
            "policy-2026.09",
            1,
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn happy_path_transitions() {
        let e = intent();
        e.verify_args_integrity().unwrap();
        let now = Utc::now();
        let d = e
            .apply_update(EffectState::Dispatched, Some("attempt-1".into()), None, now)
            .unwrap();
        assert_eq!(d.state, EffectState::Dispatched);
        let c = d
            .apply_update(
                EffectState::Committed,
                Some("attempt-1".into()),
                Some("ref".into()),
                now,
            )
            .unwrap();
        assert_eq!(c.state, EffectState::Committed);
        // Terminal.
        assert!(!c.state.can_transition_to(EffectState::Aborted));
    }

    #[test]
    fn unknown_outcome_is_never_retried_as_dispatch() {
        let e = intent();
        let now = Utc::now();
        let d = e
            .apply_update(EffectState::Dispatched, Some("attempt-1".into()), None, now)
            .unwrap();
        let u = d
            .apply_update(
                EffectState::OutcomeUnknown,
                Some("attempt-1".into()),
                None,
                now,
            )
            .unwrap();
        // outcome_unknown may only commit or abort; re-dispatch is illegal.
        assert!(!u.state.can_transition_to(EffectState::Dispatched));
        assert!(u.state.can_transition_to(EffectState::Committed));
        assert!(u.state.can_transition_to(EffectState::Aborted));
        assert!(u
            .apply_update(EffectState::Dispatched, Some("a2".into()), None, now)
            .is_err());
    }

    #[test]
    fn dispatch_requires_attempt_id() {
        let e = intent();
        assert!(matches!(
            e.apply_update(EffectState::Dispatched, None, None, Utc::now()),
            Err(EffectError::MissingAttemptId)
        ));
    }

    #[test]
    fn immutable_fields_cannot_drift() {
        let e = intent();
        let now = Utc::now();
        let d = e
            .apply_update(EffectState::Dispatched, Some("a".into()), None, now)
            .unwrap();
        // The update produces a fresh struct; simulate tampering and check
        // integrity verification catches it.
        let mut tampered = d.clone();
        tampered.canonical_args = harbor_canonical::parse(r#"{"to":"evil@x.com"}"#).unwrap();
        assert!(tampered.verify_args_integrity().is_err());
        // Attempting to change the target via apply_update is impossible by
        // construction; verify the update copies bindings unchanged.
        assert_eq!(d.target, e.target);
    }

    #[test]
    fn prepared_can_only_dispatch_or_abort() {
        let e = intent();
        assert!(e.state.can_transition_to(EffectState::Dispatched));
        assert!(e.state.can_transition_to(EffectState::Aborted));
        assert!(!e.state.can_transition_to(EffectState::Committed));
        assert!(!e.state.can_transition_to(EffectState::OutcomeUnknown));
    }
}
