//! Security scenario SEC-021 (09_Security_Test_Matrix.csv) as an
//! executable control: arbitrary code in an HF repo.
//! `security.sec_021` — model imports are DATA-ONLY:
//! - a weights file that is not a GGUF container is refused at install
//!   (a payload smuggling executable content never reaches the runtime,
//!   which only ever parses GGUF as data);
//! - the acquisition path never honors `trust_remote_code`-style flags
//!   (static scan of the hub/downloader sources).

use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn manifest(id: &str, sha: &str, size: u64) -> PackageManifest {
    PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.into(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: "model.gguf".into(),
            sha256: sha.into(),
            size_bytes: size,
        }],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.into()],
        },
    }
}

/// SEC-021 part 1: non-GGUF weights are refused at ingest — the
/// data-only boundary is enforced before anything is committed.
#[test]
fn sec_021_non_gguf_weights_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let installer = PackageInstaller::new(dir.path());
    // A plausible-looking payload that is NOT a GGUF container (e.g. a
    // script or pickle smuggled as "weights").
    let payload = b"import os; os.system('echo pwned')".to_vec();
    let sha = harbor_canonical::sha256_hex(&payload);
    let m = manifest("evil", &sha, payload.len() as u64);
    let mut staged = installer.begin("evil").unwrap();
    let err = installer
        .ingest_file(&mut staged, &m.files[0], &payload)
        .unwrap_err();
    assert!(
        matches!(err, harbor_modelhub::install::InstallError::NotGguf(_)),
        "non-GGUF weights must be refused as data-only imports, got: {err}"
    );
    // Nothing was committed.
    assert!(!dir.path().join("evil").exists());
}

/// SEC-021 part 2: a real GGUF header passes the boundary.
#[test]
fn sec_021_gguf_container_passes() {
    let dir = tempfile::tempdir().unwrap();
    let installer = PackageInstaller::new(dir.path());
    // Minimal GGUF header: magic + version + tensor/_kv counts (the
    // parser reads more, but ingest only checks the container magic).
    let mut payload = b"GGUF".to_vec();
    payload.extend_from_slice(&3u32.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    let sha = harbor_canonical::sha256_hex(&payload);
    let m = manifest("ok", &sha, payload.len() as u64);
    let mut staged = installer.begin("ok").unwrap();
    installer
        .ingest_file(&mut staged, &m.files[0], &payload)
        .unwrap();
    let report = installer.validate(&staged, &m).unwrap();
    assert!(report.ok, "{:?}", report.problems);
}

/// SEC-021 part 3 (static half): the acquisition CODE never honors
/// remote-code execution flags. The check reads non-comment, non-test
/// source only — documentation may say the word, and test modules may
/// run git for commit-bound evidence; the import path itself must have
/// no such hook.
#[test]
fn sec_021_no_remote_code_honored_anywhere_in_modelhub() {
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for entry in std::fs::read_dir(&src_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // Strip the trailing #[cfg(test)] module (repo convention).
        let code = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => &text[..],
        };
        // Strip line and block comments with a byte-index scan.
        let bytes = code.as_bytes();
        let mut cleaned = String::with_capacity(code.len());
        let mut i = 0usize;
        let mut in_block = false;
        while i < bytes.len() {
            let two = if i + 1 < bytes.len() {
                &bytes[i..i + 2]
            } else {
                b""
            };
            if in_block {
                if two == b"*/" {
                    in_block = false;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            if two == b"/*" {
                in_block = true;
                cleaned.push(' ');
                i += 2;
                continue;
            }
            if two == b"//" {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            cleaned.push(bytes[i] as char);
            i += 1;
        }
        for needle in ["trust_remote_code", "eval(", "Command::new"] {
            if cleaned.contains(needle) {
                violations.push(format!("{}: {}", path.display(), needle));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "modelhub must stay data-only: {violations:?}"
    );
}
