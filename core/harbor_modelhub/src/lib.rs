//! Harbor model platform: catalog, staged package acquisition, package
//! validation and the Fit Score.
//!
//! Authority: `17_Model_Provider_and_Package_Contract.md`,
//! `schemas/model_package.schema.json`, `03_Architecture_Contracts.md` §7,
//! goal §5/§6.
//!
//! Model packages are data: no arbitrary repository code executes (no
//! `trust_remote_code` equivalent). Every install is staged, validated,
//! hashed and atomically committed before becoming available. Fit Score
//! evaluates device + model reality and never recommends a model merely
//! because the file can physically be downloaded.

pub mod catalog;
pub mod catalog_signing;
pub mod fit;
pub mod hf;
pub mod install;

pub use catalog::{CatalogEntry, RecommendedTier};
pub use catalog_signing::{sign_catalog, sign_rotation, CatalogSignError, CatalogSigningKey, CatalogVerifier, SignedCatalog, SignedRotation};
pub use fit::{DeviceProfile, FitBand, FitScore, ModelFootprint, Thermal};
pub use hf::{HfDiscovery, HfFile};
pub use install::{InstallStage, PackageInstaller, StagedInstall, ValidationReport};
