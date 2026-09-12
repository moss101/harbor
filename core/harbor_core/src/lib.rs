//! Harbor Core facade: one handle over the Harbor subsystems with the
//! dependency rules of `08_Repo_Structure.md` enforced by construction.
//!
//! The Flutter layer never owns policy: privacy mode, egress authorization,
//! approvals and artifact commits are reachable only through this facade
//! (and its FFI projection). One runtime, one permission model, one
//! artifact model, one evidence model.

pub mod workspace;

pub use workspace::{OpenOptions, Workspace};

#[derive(Debug, thiserror::Error)]
pub enum HarborError {
    #[error("store: {0}")]
    Store(#[from] harbor_store::StoreError),
    #[error("security: {0}")]
    Security(String),
    #[error("agent: {0}")]
    Agent(#[from] harbor_agent::LogError),
    #[error("lease: {0}")]
    Lease(#[from] harbor_agent::LeaseError),
    #[error("artifacts: {0}")]
    Artifacts(#[from] harbor_artifacts::SafeCommitError),
    #[error("{0}")]
    Other(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
