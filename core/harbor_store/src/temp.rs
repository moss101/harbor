//! Decrypted working windows: operation-bound plaintext files for native
//! libraries that cannot consume memory buffers.
//!
//! Policy (13_Network_and_Storage_Policy.md): an approved window is the
//! lifetime of one authorized parse/render/export operation; files use an
//! OS-protected app-private directory, random names, restrictive access and
//! an operation-bound registry entry. Handles close and files are removed
//! on completion, cancellation and lock; `sweep_on_restart` runs before new
//! workspace work and reports residue left by process death.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::error::Result;

pub struct TempHandle {
    path: PathBuf,
    operation: String,
    opened: Instant,
    removed: bool,
}

impl TempHandle {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Close and remove the plaintext file. Idempotent.
    pub fn close(mut self) {
        self.remove();
    }

    fn remove(&mut self) {
        if !self.removed {
            let _ = std::fs::remove_file(&self.path);
            self.removed = true;
        }
    }
}

impl Drop for TempHandle {
    fn drop(&mut self) {
        self.remove();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidueReport {
    pub removed_files: Vec<PathBuf>,
}

pub struct TempRegistry {
    dir: PathBuf,
    open: std::sync::Mutex<Vec<TempHandle>>,
}

impl TempRegistry {
    pub fn new(base: impl Into<PathBuf>) -> Result<Self> {
        let dir = base.into().join("working-windows");
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dir)?.permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(&dir, perms)?;
        }
        Ok(TempRegistry { dir, open: std::sync::Mutex::new(Vec::new()) })
    }

    /// Open an operation-bound plaintext window containing `bytes`.
    pub fn open_window(&self, operation: &str, bytes: &[u8]) -> Result<TempHandle> {
        let name = format!(
            "{}-{}.tmp",
            uuidv4(),
            harbor_canonical::sha256_hex(operation.as_bytes()).get(..12).unwrap_or("op").to_string()
        );
        let path = self.dir.join(name);
        std::fs::write(&path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }
        let handle = TempHandle { path, operation: operation.to_string(), opened: Instant::now(), removed: false };
        self.open.lock().unwrap().push(TempHandle {
            path: handle.path.clone(),
            operation: handle.operation.clone(),
            opened: handle.opened,
            removed: handle.removed,
        });
        Ok(handle)
    }

    /// Startup sweep: remove any residue from process death and report what
    /// was found. Runs before new workspace work.
    pub fn sweep_on_restart(&self) -> Result<ResidueReport> {
        let mut removed = Vec::new();
        self.open.lock().unwrap().clear();
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)? {
                let p = entry?.path();
                if p.is_file() {
                    std::fs::remove_file(&p)?;
                    removed.push(p);
                }
            }
        }
        Ok(ResidueReport { removed_files: removed })
    }

    pub fn open_count(&self) -> usize {
        self.open.lock().unwrap().len()
    }
}

fn uuidv4() -> String {
    // Random hex id; avoids a UUID dependency for this one use.
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = harbor_canonical::sha256_hex(&b);
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_roundtrip_and_close_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let reg = TempRegistry::new(dir.path()).unwrap();
        let h = reg.open_window("xlsx.render", b"PLAINSHEET").unwrap();
        assert_eq!(std::fs::read(h.path()).unwrap(), b"PLAINSHEET");
        let p = h.path().to_path_buf();
        h.close();
        assert!(!p.exists());
    }

    #[test]
    fn drop_closes_window() {
        let dir = tempfile::tempdir().unwrap();
        let reg = TempRegistry::new(dir.path()).unwrap();
        {
            let h = reg.open_window("pdf.parse", b"x").unwrap();
            let p = h.path().to_path_buf();
            assert!(p.exists());
            drop(h);
            assert!(!p.exists());
        }
    }

    #[test]
    fn restart_sweep_removes_residue() {
        let dir = tempfile::tempdir().unwrap();
        {
            let reg = TempRegistry::new(dir.path()).unwrap();
            // Simulate process death: write a file that outlives the registry
            // without being tracked.
            std::fs::write(dir.path().join("working-windows").join("orphan.tmp"), b"z").unwrap();
            let _ = reg.open_window("op", b"y").unwrap();
        }
        let reg2 = TempRegistry::new(dir.path()).unwrap();
        let report = reg2.sweep_on_restart().unwrap();
        assert_eq!(report.removed_files.len(), 1);
        assert!(reg2.sweep_on_restart().unwrap().removed_files.is_empty());
    }
}
