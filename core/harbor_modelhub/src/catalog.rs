//! Harbor Recommended / Library catalog with human-friendly tiers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendedTier {
    Fast,
    Balanced,
    Quality,
    Coding,
    Vision,
    Arabic,
}

impl RecommendedTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecommendedTier::Fast => "Fast",
            RecommendedTier::Balanced => "Balanced",
            RecommendedTier::Quality => "Quality",
            RecommendedTier::Coding => "Coding",
            RecommendedTier::Vision => "Vision",
            RecommendedTier::Arabic => "Arabic",
        }
    }
}

/// One catalog entry: a package a user can install (Harbor Library or
/// Recommended). Fit Score is computed per device, not stored here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub display_name: String,
    pub tiers: Vec<RecommendedTier>,
    pub repo_id: String,
    pub revision: String,
    pub files: Vec<CatalogFile>,
    pub quantization: String,
    pub context_tokens: u64,
    pub multimodal: bool,
    pub license: String,
    /// Bilingual EN/AR capability flag for the Arabic tier.
    pub arabic_optimized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogFile {
    pub role: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

impl CatalogEntry {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size_bytes).sum()
    }

    pub fn to_manifest(&self) -> crate::install::PackageManifest {
        crate::install::PackageManifest {
            schema: "harbor.model/v3".into(),
            id: self.id.clone(),
            reference_type: "installed_package".into(),
            files: self
                .files
                .iter()
                .map(|f| crate::install::PackageFile {
                    role: f.role.clone(),
                    path: f.path.clone(),
                    sha256: f.sha256.clone(),
                    size_bytes: f.size_bytes,
                })
                .collect(),
            runtime: crate::install::RuntimeBinding {
                kind: "gguf/llama.cpp".into(),
                min_revision: "b4000".into(),
                targets: vec!["arm64".into(), "arm64-v8a".into(), "x64".into()],
            },
        }
    }
}
