//! Capability handles: least-privilege, scoped, revocable authorities.
//! A capability never grants OS permissions beyond its scope, expires, and
//! can be revoked; approval receipts reference the permission record that
//! minted them.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::ids::HarborId;

/// What a capability authorizes, bound to a concrete scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityScope {
    /// Machine-readable capability id, e.g. `cap.file.write`.
    pub capability_id: HarborId,
    /// Concrete object scope (path, connector object, endpoint...).
    pub object: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityState {
    Active,
    Revoked,
    Expired,
}

/// A durable capability record (the "permission record").
#[derive(Debug, Clone)]
pub struct Capability {
    pub record_id: HarborId,
    pub scope: CapabilityScope,
    pub allow_once: bool,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub state: CapabilityState,
    pub uses_remaining: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("capability revoked")]
    Revoked,
    #[error("capability expired at {0}")]
    Expired(DateTime<Utc>),
    #[error("capability exhausted")]
    Exhausted,
}

/// In-memory registry of capability records with persistence hooks left to
/// the store layer; authoritative state lives in the encrypted database.
#[derive(Default)]
pub struct CapabilityRegistry {
    records: HashMap<HarborId, Capability>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(
        &mut self,
        scope: CapabilityScope,
        allow_once: bool,
        expires_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Capability {
        let record = Capability {
            record_id: HarborId::generate("perm"),
            scope,
            allow_once,
            issued_at: now,
            expires_at,
            state: CapabilityState::Active,
            uses_remaining: if allow_once { Some(1) } else { None },
        };
        self.records
            .insert(record.record_id.clone(), record.clone());
        record
    }

    pub fn revoke(&mut self, record_id: &HarborId) -> bool {
        if let Some(c) = self.records.get_mut(record_id) {
            c.state = CapabilityState::Revoked;
            true
        } else {
            false
        }
    }

    /// Check usability at `now`; returns an error when revoked/expired/
    /// exhausted. Does NOT consume a use — consumption commits together
    /// with the dispatch it authorizes (see `consume`).
    pub fn check(&self, record_id: &HarborId, now: DateTime<Utc>) -> Result<(), CapabilityError> {
        let c = self
            .records
            .get(record_id)
            .ok_or(CapabilityError::Revoked)?;
        match c.state {
            CapabilityState::Revoked => Err(CapabilityError::Revoked),
            CapabilityState::Expired => Err(CapabilityError::Expired(c.expires_at.unwrap_or(now))),
            CapabilityState::Active => {
                if let Some(exp) = c.expires_at {
                    if now >= exp {
                        return Err(CapabilityError::Expired(exp));
                    }
                }
                if c.uses_remaining == Some(0) {
                    return Err(CapabilityError::Exhausted);
                }
                Ok(())
            }
        }
    }

    /// Consume one use atomically after a successful authorization.
    pub fn consume(
        &mut self,
        record_id: &HarborId,
        now: DateTime<Utc>,
    ) -> Result<(), CapabilityError> {
        self.check(record_id, now)?;
        let c = self.records.get_mut(record_id).unwrap();
        if let Some(n) = c.uses_remaining.as_mut() {
            *n = n.saturating_sub(1);
        }
        Ok(())
    }

    pub fn get(&self, record_id: &HarborId) -> Option<&Capability> {
        self.records.get(record_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> CapabilityScope {
        CapabilityScope {
            capability_id: HarborId::new("cap.file.write").unwrap(),
            object: "/Users/test/report.docx".into(),
        }
    }

    #[test]
    fn allow_once_capability_is_single_use() {
        let mut reg = CapabilityRegistry::new();
        let now = Utc::now();
        let cap = reg.grant(scope(), true, None, now);
        reg.check(&cap.record_id, now).unwrap();
        reg.consume(&cap.record_id, now).unwrap();
        assert!(matches!(
            reg.check(&cap.record_id, now),
            Err(CapabilityError::Exhausted)
        ));
    }

    #[test]
    fn revocation_blocks_immediately() {
        let mut reg = CapabilityRegistry::new();
        let now = Utc::now();
        let cap = reg.grant(scope(), false, None, now);
        assert!(reg.revoke(&cap.record_id));
        assert!(matches!(
            reg.check(&cap.record_id, now),
            Err(CapabilityError::Revoked)
        ));
    }

    #[test]
    fn expiry_is_checked() {
        let mut reg = CapabilityRegistry::new();
        let now = Utc::now();
        let cap = reg.grant(
            scope(),
            false,
            Some(now + chrono::Duration::minutes(5)),
            now,
        );
        assert!(reg.check(&cap.record_id, now).is_ok());
        let later = now + chrono::Duration::minutes(6);
        assert!(matches!(
            reg.check(&cap.record_id, later),
            Err(CapabilityError::Expired(_))
        ));
    }
}
