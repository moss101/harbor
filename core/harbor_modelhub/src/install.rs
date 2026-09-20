//! Staged, validated, atomic model package installation.
//!
//! Flow: prepare staging dir -> download/copy files -> verify each file
//! against the expected SHA-256 + size -> validate the package manifest ->
//! atomic commit into the installed store (rename into place, manifest
//! written last). Interrupted installs are garbage: they never surface as
//! installed models.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallStage {
    Preparing,
    Downloading,
    Verifying,
    Committing,
    Installed,
    Failed,
}

/// Manifest of an installed package (`harbor.model/v3` subset relevant to
/// installation; the full schema is validated in the dossier tools).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub schema: String,
    pub id: String,
    pub reference_type: String,
    pub files: Vec<PackageFile>,
    pub runtime: RuntimeBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageFile {
    pub role: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeBinding {
    /// e.g. "gguf/llama.cpp".
    pub kind: String,
    /// Minimum runtime revision this package requires.
    pub min_revision: String,
    /// Architecture/qualifier the package is compatible with.
    pub targets: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StagedInstall {
    pub package_id: String,
    pub stage: InstallStage,
    pub staging_dir: PathBuf,
    pub verified_files: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub ok: bool,
    pub checked_files: usize,
    pub problems: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("hash mismatch for {0}")]
    HashMismatch(String),
    #[error("size mismatch for {0}")]
    SizeMismatch(String),
    #[error("package path escape: {0}")]
    PathEscape(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid manifest: {0}")]
    Manifest(String),
}

pub struct PackageInstaller {
    installed_root: PathBuf,
}

impl PackageInstaller {
    pub fn new(installed_root: impl Into<PathBuf>) -> Self {
        PackageInstaller {
            installed_root: installed_root.into(),
        }
    }

    fn package_dir(&self, package_id: &str) -> PathBuf {
        self.installed_root.join(package_id)
    }

    fn staging_dir(&self, package_id: &str) -> PathBuf {
        self.installed_root.join(format!(".staging-{package_id}"))
    }

    /// Remove a package's staging directory (failed or abandoned
    /// acquisition). Idempotent; never touches an installed package.
    pub fn discard_staging(&self, package_id: &str) {
        let staging = self.staging_dir(package_id);
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
    }

    /// Remove every `.staging-*` directory left by a process that died
    /// mid-acquisition (restart sweep, like the temp-window registry).
    /// Returns the package ids whose residue was removed.
    pub fn sweep_staging(&self) -> Result<Vec<String>, InstallError> {
        let mut removed = Vec::new();
        if !self.installed_root.exists() {
            return Ok(removed);
        }
        for entry in std::fs::read_dir(&self.installed_root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_prefix(".staging-") {
                if entry.path().is_dir() {
                    std::fs::remove_dir_all(entry.path())?;
                    removed.push(id.to_string());
                }
            }
        }
        Ok(removed)
    }

    /// Begin staging: fresh staging directory (previous garbage removed).
    pub fn begin(&self, package_id: &str) -> Result<StagedInstall, InstallError> {
        let staging = self.staging_dir(package_id);
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(&staging)?;
        Ok(StagedInstall {
            package_id: package_id.into(),
            stage: InstallStage::Preparing,
            staging_dir: staging,
            verified_files: BTreeMap::new(),
        })
    }

    /// Ingest one file from bytes (download callback writes into staging);
    /// verifies size + hash against the expected entry.
    pub fn ingest_file(
        &self,
        staged: &mut StagedInstall,
        expect: &PackageFile,
        bytes: &[u8],
    ) -> Result<(), InstallError> {
        verify_relative_path(&expect.path)?;
        staged.stage = InstallStage::Downloading;
        let out = staged.staging_dir.join(&expect.path);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out, bytes)?;
        // Verify.
        staged.stage = InstallStage::Verifying;
        let got_hash = harbor_canonical::sha256_hex(bytes);
        if got_hash != expect.sha256 {
            let _ = std::fs::remove_file(&out);
            return Err(InstallError::HashMismatch(expect.path.clone()));
        }
        if bytes.len() as u64 != expect.size_bytes {
            let _ = std::fs::remove_file(&out);
            return Err(InstallError::SizeMismatch(expect.path.clone()));
        }
        staged
            .verified_files
            .insert(expect.path.clone(), expect.size_bytes);
        Ok(())
    }

    /// Validate the staged tree: manifest files all present and verified.
    pub fn validate(
        &self,
        staged: &StagedInstall,
        manifest: &PackageManifest,
    ) -> Result<ValidationReport, InstallError> {
        let mut problems = Vec::new();
        if manifest.schema != "harbor.model/v3" {
            problems.push(format!("unsupported manifest schema {}", manifest.schema));
        }
        for f in &manifest.files {
            if !staged.verified_files.contains_key(&f.path) {
                problems.push(format!("missing verified file: {}", f.path));
            }
        }
        let has_weights = manifest
            .files
            .iter()
            .any(|f| f.role == "weights" || f.role == "weights_shard");
        if !has_weights {
            problems.push("no weights in package".into());
        }
        Ok(ValidationReport {
            ok: problems.is_empty(),
            checked_files: manifest.files.len(),
            problems,
        })
    }

    /// Atomic commit: staging dir renamed into place. Interrupted commits
    /// leave the staging dir, never a partial package in the store.
    pub fn commit(
        &self,
        staged: &mut StagedInstall,
        manifest: &PackageManifest,
        now: DateTime<Utc>,
    ) -> Result<PathBuf, InstallError> {
        staged.stage = InstallStage::Committing;
        let final_dir = self.package_dir(&manifest.id);
        if final_dir.exists() {
            // Idempotent: already installed.
            staged.stage = InstallStage::Installed;
            return Ok(final_dir);
        }
        // Write manifest last inside staging; its presence marks a complete
        // staged package.
        let manifest_bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|e| InstallError::Manifest(e.to_string()))?;
        std::fs::write(
            staged.staging_dir.join("harbor_manifest.json"),
            manifest_bytes,
        )?;
        std::fs::rename(&staged.staging_dir, &final_dir)?;
        let _ = now;
        staged.stage = InstallStage::Installed;
        Ok(final_dir)
    }

    pub fn installed_packages(&self) -> Result<Vec<String>, InstallError> {
        let mut out = Vec::new();
        if !self.installed_root.exists() {
            return Ok(out);
        }
        for e in std::fs::read_dir(&self.installed_root)? {
            let p = e?.path();
            if p.is_dir() && p.join("harbor_manifest.json").exists() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with(".staging-") {
                        out.push(name.to_string());
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn load_manifest(&self, package_id: &str) -> Result<PackageManifest, InstallError> {
        let path = self.package_dir(package_id).join("harbor_manifest.json");
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(|e| InstallError::Manifest(e.to_string()))
    }

    /// Remove an installed package.
    pub fn remove(&self, package_id: &str) -> Result<(), InstallError> {
        let dir = self.package_dir(package_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}

fn verify_relative_path(path: &str) -> Result<(), InstallError> {
    if path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.split('/').any(|seg| seg == ".." || seg == ".")
    {
        return Err(InstallError::PathEscape(path.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_entry(path: &str, bytes: &[u8]) -> PackageFile {
        PackageFile {
            role: if path.ends_with(".gguf") {
                "weights".into()
            } else {
                "config".into()
            },
            path: path.into(),
            sha256: harbor_canonical::sha256_hex(bytes),
            size_bytes: bytes.len() as u64,
        }
    }

    fn manifest(id: &str, weights: &[u8], config: &[u8]) -> PackageManifest {
        PackageManifest {
            schema: "harbor.model/v3".into(),
            id: id.into(),
            reference_type: "installed_package".into(),
            files: vec![
                file_entry("model.gguf", weights),
                file_entry("config.json", config),
            ],
            runtime: RuntimeBinding {
                kind: "gguf/llama.cpp".into(),
                min_revision: "b4000".into(),
                targets: vec!["arm64".into(), "x64".into()],
            },
        }
    }

    #[test]
    fn staged_install_happy_path() {
        let dir = tempfile::tempdir().unwrap();
        let inst = PackageInstaller::new(dir.path().join("installed"));
        let weights = vec![1u8; 4096];
        let config = br#"{"ctx":4096}"#.to_vec();
        let m = manifest("tiny-test-model", &weights, &config);
        let mut staged = inst.begin("tiny-test-model").unwrap();
        inst.ingest_file(&mut staged, &m.files[0], &weights)
            .unwrap();
        inst.ingest_file(&mut staged, &m.files[1], &config).unwrap();
        let report = inst.validate(&staged, &m).unwrap();
        assert!(report.ok, "problems: {:?}", report.problems);
        let final_dir = inst.commit(&mut staged, &m, Utc::now()).unwrap();
        assert!(final_dir.join("harbor_manifest.json").exists());
        assert_eq!(
            inst.installed_packages().unwrap(),
            vec!["tiny-test-model".to_string()]
        );
        let loaded = inst.load_manifest("tiny-test-model").unwrap();
        assert_eq!(loaded.id, "tiny-test-model");
        // Idempotent commit.
        let again = inst.commit(&mut staged, &m, Utc::now()).unwrap();
        assert_eq!(again, final_dir);
        inst.remove("tiny-test-model").unwrap();
        assert!(inst.installed_packages().unwrap().is_empty());
    }

    #[test]
    fn hash_mismatch_rejects_file() {
        let dir = tempfile::tempdir().unwrap();
        let inst = PackageInstaller::new(dir.path().join("installed"));
        let m = manifest("m2", &[1, 2, 3], b"{}");
        let mut staged = inst.begin("m2").unwrap();
        let err = inst
            .ingest_file(&mut staged, &m.files[0], &[9, 9, 9])
            .unwrap_err();
        assert!(matches!(err, InstallError::HashMismatch(p) if p == "model.gguf"));
    }

    #[test]
    fn path_escape_rejected() {
        for bad in [
            "/abs/path",
            "../escape",
            "a/../..",
            "C:\\win",
            "back\\slash",
        ] {
            assert!(verify_relative_path(bad).is_err(), "{bad} must be rejected");
        }
        assert!(verify_relative_path("sub/dir/model.gguf").is_ok());
    }

    #[test]
    fn incomplete_package_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("installed");
        let inst = PackageInstaller::new(&root);
        let m = manifest("m3", &[1], b"{}");
        let mut staged = inst.begin("m3").unwrap();
        inst.ingest_file(&mut staged, &m.files[0], &[1]).unwrap();
        // Never committed: staging exists but package is NOT installed.
        assert!(inst.installed_packages().unwrap().is_empty());
        assert!(root.join(".staging-m3").exists());
    }
}
