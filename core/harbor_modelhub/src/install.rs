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
    #[error("weights file {0} is not a GGUF container (data-only imports, SEC-021)")]
    NotGguf(String),
    #[error("package path escape: {0}")]
    PathEscape(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("package {0} is not installed")]
    NotFound(String),
    #[error(
        "deletion scope changed since preview for {0}: confirm against a fresh preview (SEC-024)"
    )]
    ScopeChanged(String),
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
        // SEC-021: model imports are DATA-ONLY. A weights file that is
        // not a GGUF container is refused here — a repository payload
        // that smuggles executable content (trust_remote_code-style)
        // never reaches the runtime, which only ever parses GGUF as
        // data (llama.cpp executes nothing from the file).
        if expect.role == "weights" && !bytes.starts_with(b"GGUF") {
            let _ = std::fs::remove_file(&out);
            return Err(InstallError::NotGguf(expect.path.clone()));
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
                    if !name.starts_with(".staging-") && !name.starts_with(".trash-") {
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

    /// The install root (SEC-029 preflight probes free space here).
    pub fn root(&self) -> &std::path::Path {
        &self.installed_root
    }

    /// SEC-024 deletion preview: the exact scope a `trash` will remove,
    /// bound by a digest the commit must echo. Scope comes from the
    /// installed manifest (owned-files accounting), not from a directory
    /// walk, so a corrupted or hand-tampered package cannot widen the
    /// blast radius; the walk total is reported for transparency only.
    pub fn deletion_preview(&self, package_id: &str) -> Result<DeletionPreview, InstallError> {
        if !self
            .package_dir(package_id)
            .join("harbor_manifest.json")
            .exists()
        {
            return Err(InstallError::NotFound(package_id.into()));
        }
        let manifest = self.load_manifest(package_id)?;
        let mut files = manifest.files.clone();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        for f in &files {
            // A manifest is data from the time of install; its paths are
            // re-validated here so a preview can never bless an escape.
            verify_relative_path(&f.path)?;
        }
        let manifest_bytes_total = files.iter().map(|f| f.size_bytes).sum();
        let dir_bytes_total = dir_size(&self.package_dir(package_id));
        Ok(DeletionPreview {
            scope_digest: deletion_scope_digest(package_id, &files),
            package_id: package_id.to_string(),
            files,
            manifest_bytes_total,
            dir_bytes_total,
        })
    }

    /// SEC-024 uninstall: move the package into the trash window instead
    /// of deleting outright (undo is a rename-back while the entry
    /// survives). Refuses unless `expected_scope_digest` matches a fresh
    /// preview — a deletion decided against stale scope never runs.
    /// Returns the trashed directory name and the bytes freed from the
    /// installed store.
    pub fn trash(
        &self,
        package_id: &str,
        expected_scope_digest: &str,
    ) -> Result<(String, u64), InstallError> {
        let preview = self.deletion_preview(package_id)?;
        if preview.scope_digest != expected_scope_digest {
            return Err(InstallError::ScopeChanged(package_id.into()));
        }
        let stamp = chrono::Utc::now().timestamp_millis();
        let entry = format!(".trash-{stamp}-{package_id}");
        std::fs::rename(
            self.package_dir(package_id),
            self.installed_root.join(&entry),
        )?;
        Ok((entry, preview.manifest_bytes_total))
    }

    /// SEC-024 undo: restore a trashed entry while it is still in the
    /// trash window. Fails honestly if the same package was reinstalled
    /// in the meantime (the target is occupied).
    pub fn restore_trashed(&self, trash_entry: &str) -> Result<String, InstallError> {
        let package_id = trash_package_id(trash_entry)
            .ok_or_else(|| InstallError::NotFound(trash_entry.into()))?;
        let from = self.installed_root.join(trash_entry);
        if !from.exists() {
            return Err(InstallError::NotFound(trash_entry.into()));
        }
        let to = self.package_dir(package_id);
        if to.exists() {
            return Err(InstallError::Manifest(format!(
                "package {package_id} is installed again; delete the new copy first"
            )));
        }
        std::fs::rename(from, to)?;
        Ok(package_id.to_string())
    }

    /// SEC-024 startup sweep: trash entries older than `max_age` are
    /// removed for real (the undo window has closed). Returns the
    /// package ids whose bytes were reclaimed.
    pub fn sweep_trash(&self, max_age: chrono::Duration) -> Result<Vec<String>, InstallError> {
        let mut reclaimed = Vec::new();
        if !self.installed_root.exists() {
            return Ok(reclaimed);
        }
        let cutoff = chrono::Utc::now().timestamp_millis() - max_age.num_milliseconds();
        for entry in std::fs::read_dir(&self.installed_root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(rest) = name.strip_prefix(".trash-") else {
                continue;
            };
            let Some((ms_str, id)) = rest.split_once('-') else {
                continue;
            };
            let Ok(ms) = ms_str.parse::<i64>() else {
                continue;
            };
            if ms < cutoff && entry.path().is_dir() {
                std::fs::remove_dir_all(entry.path())?;
                reclaimed.push(id.to_string());
            }
        }
        Ok(reclaimed)
    }
}

/// SEC-024 deletion scope: what a `trash` of one package will remove.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeletionPreview {
    /// Binds the commit to this exact scope (echoed back on trash).
    pub scope_digest: String,
    pub package_id: String,
    pub files: Vec<PackageFile>,
    /// Owned bytes per the manifest (the accounting uninstall is held to).
    pub manifest_bytes_total: u64,
    /// Actual on-disk total, for transparency; may exceed the manifest
    /// when a package was written by an older revision.
    pub dir_bytes_total: u64,
}

/// Digest binding a deletion decision to an exact owned-file scope.
fn deletion_scope_digest(package_id: &str, files: &[PackageFile]) -> String {
    let payload = format!(
        "harbor.deletion_scope/v1\npackage: {package_id}\n{}",
        files
            .iter()
            .map(|f| format!("{} {} {}\n", f.path, f.sha256, f.size_bytes))
            .collect::<String>()
    );
    harbor_canonical::sha256_hex(payload.as_bytes())
}

fn trash_package_id(trash_entry: &str) -> Option<&str> {
    let rest = trash_entry.strip_prefix(".trash-")?;
    let (_, id) = rest.split_once('-')?;
    Some(id)
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            total += dir_size(&p);
        } else if let Ok(meta) = e.metadata() {
            total += meta.len();
        }
    }
    total
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
        let mut weights = b"GGUF".to_vec();
        weights.extend(vec![1u8; 4096]);
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
        let m = manifest("m3", b"GGUF\x03", b"{}");
        let mut staged = inst.begin("m3").unwrap();
        inst.ingest_file(&mut staged, &m.files[0], b"GGUF\x03")
            .unwrap();
        // Never committed: staging exists but package is NOT installed.
        assert!(inst.installed_packages().unwrap().is_empty());
        assert!(root.join(".staging-m3").exists());
    }

    // -- SEC-024: deletion preview, scope binding, trash/undo window -----

    #[test]
    fn deletion_preview_lists_owned_files_and_binds_scope() {
        let dir = tempfile::tempdir().unwrap();
        let inst = PackageInstaller::new(dir.path().join("installed"));
        let weights = b"GGUF\x01\x02\x03\x04";
        let config = br#"{"ctx":4096}"#;
        let m = manifest("m-del", weights, config);
        let mut staged = inst.begin("m-del").unwrap();
        inst.ingest_file(&mut staged, &m.files[0], weights).unwrap();
        inst.ingest_file(&mut staged, &m.files[1], config).unwrap();
        inst.commit(&mut staged, &m, Utc::now()).unwrap();

        let preview = inst.deletion_preview("m-del").unwrap();
        assert_eq!(preview.files.len(), 2);
        assert_eq!(
            preview.manifest_bytes_total,
            (weights.len() + config.len()) as u64
        );
        assert!(preview.dir_bytes_total >= preview.manifest_bytes_total);
        assert_eq!(preview.scope_digest.len(), 64);
        // Scope is deterministic.
        let again = inst.deletion_preview("m-del").unwrap();
        assert_eq!(again.scope_digest, preview.scope_digest);
    }

    #[test]
    fn deletion_preview_refuses_unknown_package() {
        let dir = tempfile::tempdir().unwrap();
        let inst = PackageInstaller::new(dir.path().join("installed"));
        assert!(matches!(
            inst.deletion_preview("ghost"),
            Err(InstallError::NotFound(_))
        ));
    }

    #[test]
    fn trash_refuses_stale_scope_then_succeeds_on_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let inst = PackageInstaller::new(dir.path().join("installed"));
        let m = manifest("m-trash", b"GGUF\x09", b"{}");
        let mut staged = inst.begin("m-trash").unwrap();
        inst.ingest_file(&mut staged, &m.files[0], b"GGUF\x09")
            .unwrap();
        inst.commit(&mut staged, &m, Utc::now()).unwrap();
        let stale = inst.deletion_preview("m-trash").unwrap();

        // The package changed after the preview (reinstalled with
        // different bytes): the stale scope digest must not be honored.
        inst.remove("m-trash").unwrap();
        let m2 = manifest("m-trash", b"GGUF-DIFFERENT", b"{\"v\":2}");
        let mut staged2 = inst.begin("m-trash").unwrap();
        inst.ingest_file(&mut staged2, &m2.files[0], b"GGUF-DIFFERENT")
            .unwrap();
        inst.commit(&mut staged2, &m2, Utc::now()).unwrap();
        assert!(matches!(
            inst.trash("m-trash", &stale.scope_digest),
            Err(InstallError::ScopeChanged(_))
        ));

        // A fresh preview trashes (not deletes) the package.
        let fresh = inst.deletion_preview("m-trash").unwrap();
        let (entry, freed) = inst.trash("m-trash", &fresh.scope_digest).unwrap();
        assert!(freed > 0);
        assert!(entry.starts_with(".trash-"));
        assert!(entry.ends_with("-m-trash"));
        assert!(inst.installed_packages().unwrap().is_empty());
        // Trash retains the bytes (undo window), then restore returns them.
        assert!(inst.root().join(&entry).exists());
        assert_eq!(inst.restore_trashed(&entry).unwrap(), "m-trash");
        assert_eq!(
            inst.installed_packages().unwrap(),
            vec!["m-trash".to_string()]
        );
    }

    #[test]
    fn sweep_trash_reclaims_only_expired_entries() {
        let dir = tempfile::tempdir().unwrap();
        let inst = PackageInstaller::new(dir.path().join("installed"));
        for id in ["m-old", "m-new"] {
            let m = manifest(id, b"GGUF\x07", b"{}");
            let mut staged = inst.begin(id).unwrap();
            inst.ingest_file(&mut staged, &m.files[0], b"GGUF\x07")
                .unwrap();
            inst.commit(&mut staged, &m, Utc::now()).unwrap();
            let p = inst.deletion_preview(id).unwrap();
            inst.trash(id, &p.scope_digest).unwrap();
        }
        // Age the first entry beyond the window by renaming its stamp.
        let old = inst.root().join(".trash-1000-m-old");
        let _ = std::fs::rename(inst.root().join(".trash-0-m-old"), &old).is_ok();
        // The freshly-trashed entries carry the current timestamp; force
        // one to be old by rewriting its name.
        let fresh_names: Vec<String> = std::fs::read_dir(inst.root())
            .unwrap()
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.starts_with(".trash-").then_some(n)
            })
            .collect();
        for name in &fresh_names {
            if name.ends_with("-m-old") && name != ".trash-1000-m-old" {
                std::fs::rename(inst.root().join(name), &old).unwrap();
            }
        }
        let reclaimed = inst.sweep_trash(chrono::Duration::hours(72)).unwrap();
        assert_eq!(reclaimed, vec!["m-old".to_string()]);
        assert!(!inst.root().join(".trash-1000-m-old").exists());
        assert_eq!(
            inst.restore_trashed(
                &fresh_names
                    .iter()
                    .find(|n| n.ends_with("-m-new"))
                    .unwrap()
                    .clone()
            )
            .unwrap(),
            "m-new"
        );
    }

    #[test]
    fn restore_refuses_when_target_reinstalled() {
        let dir = tempfile::tempdir().unwrap();
        let inst = PackageInstaller::new(dir.path().join("installed"));
        let m = manifest("m-undo", b"GGUF\x0a", b"{}");
        let mut staged = inst.begin("m-undo").unwrap();
        inst.ingest_file(&mut staged, &m.files[0], b"GGUF\x0a")
            .unwrap();
        inst.commit(&mut staged, &m, Utc::now()).unwrap();
        let p = inst.deletion_preview("m-undo").unwrap();
        let entry = inst.trash("m-undo", &p.scope_digest).unwrap().0;
        // Reinstall the same id, then attempt undo: honest refusal.
        let mut staged2 = inst.begin("m-undo").unwrap();
        inst.ingest_file(&mut staged2, &m.files[0], b"GGUF\x0a")
            .unwrap();
        inst.commit(&mut staged2, &m, Utc::now()).unwrap();
        assert!(inst.restore_trashed(&entry).is_err());
    }
}
