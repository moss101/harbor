use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cryptographic operation failed")]
    Crypto,
    #[error("key not found: {0}")]
    KeyNotFound(String),
    #[error("blob not found: {0}")]
    BlobNotFound(String),
    #[error("integrity check failed for blob {0}")]
    Integrity(String),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),
    #[error("migration {0} failed: {1}")]
    MigrationFailed(u32, String),
    #[error("settings downgrade from schema {found} to {target} requires explicit fail-safe handling")]
    SettingsDowngrade { found: u32, target: u32 },
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
    #[error("canonical json error: {0}")]
    Canonical(#[from] harbor_canonical::CanonicalError),
}

pub type Result<T> = std::result::Result<T, StoreError>;
