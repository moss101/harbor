//! Workspace privacy policy vs. actual execution location.
//!
//! `13_Network_and_Storage_Policy.md`: PrivacyMode is policy,
//! ExecutionLocation is fact; the Trust Pulse shows both and never implies
//! that an allowed remote policy means a request went remote.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacyMode {
    LocalOnly,
    Hybrid,
    RemoteAllowed,
}

impl PrivacyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrivacyMode::LocalOnly => "LOCAL_ONLY",
            PrivacyMode::Hybrid => "HYBRID",
            PrivacyMode::RemoteAllowed => "REMOTE_ALLOWED",
        }
    }

    /// May this policy class authorize remote inference?
    pub fn allows_remote_inference(&self) -> bool {
        matches!(self, PrivacyMode::Hybrid | PrivacyMode::RemoteAllowed)
    }

    /// Does this policy prohibit user/workspace-content egress entirely?
    pub fn prohibits_content_egress(&self) -> bool {
        matches!(self, PrivacyMode::LocalOnly)
    }
}

impl fmt::Display for PrivacyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a particular request actually executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionLocation {
    OnDevice,
    Remote,
}

impl ExecutionLocation {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionLocation::OnDevice => "ON_DEVICE",
            ExecutionLocation::Remote => "REMOTE",
        }
    }
}

/// App-controlled egress classes. Every class carries its own authorization
/// rule; Local Only prohibits user/workspace-content egress but never an
/// explicit model-acquisition session the user started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EgressClass {
    AcquisitionMetadata,
    Authentication,
    WeightTransfer,
    RemoteInference,
    ConnectorRead,
    ConnectorWrite,
    Sync,
    Diagnostics,
    OsManaged,
}

impl EgressClass {
    /// Whether this class may transmit user/workspace content.
    pub fn carries_content(&self) -> bool {
        matches!(
            self,
            EgressClass::RemoteInference
                | EgressClass::ConnectorRead
                | EgressClass::ConnectorWrite
                | EgressClass::Sync
        )
    }

    /// Whether an operation of this class is a protected effect by default.
    pub fn is_protected_effect(&self) -> bool {
        matches!(self, EgressClass::ConnectorWrite)
    }

    /// Policy gate: is this class permitted under the workspace mode at all
    /// (independent of per-operation authorization)?
    pub fn permitted_under(&self, mode: PrivacyMode) -> bool {
        match (mode, self) {
            (PrivacyMode::LocalOnly, EgressClass::RemoteInference) => false,
            (PrivacyMode::LocalOnly, c) if c.carries_content() => false,
            (PrivacyMode::LocalOnly, EgressClass::Sync) => false,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_blocks_content_but_not_acquisition() {
        let m = PrivacyMode::LocalOnly;
        assert!(m.prohibits_content_egress());
        assert!(!m.allows_remote_inference());
        assert!(EgressClass::AcquisitionMetadata.permitted_under(m));
        assert!(EgressClass::WeightTransfer.permitted_under(m));
        assert!(EgressClass::Diagnostics.permitted_under(m));
        assert!(!EgressClass::RemoteInference.permitted_under(m));
        assert!(!EgressClass::ConnectorWrite.permitted_under(m));
        assert!(!EgressClass::Sync.permitted_under(m));
    }

    #[test]
    fn hybrid_allows_remote_inference_but_requires_visibility() {
        assert!(EgressClass::RemoteInference.permitted_under(PrivacyMode::Hybrid));
        assert!(EgressClass::RemoteInference.carries_content());
    }

    #[test]
    fn policy_is_not_execution() {
        // An allowed policy never implies the request went remote.
        let m = PrivacyMode::RemoteAllowed;
        assert!(m.allows_remote_inference());
        assert_eq!(ExecutionLocation::OnDevice.as_str(), "ON_DEVICE");
    }
}
