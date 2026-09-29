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

/// SEC-014: file scope escape — a package path that tries to climb out
/// of the install root is refused before anything is staged.
#[test]
fn sec_014_package_path_escape_refused() {
    let dir = tempfile::tempdir().unwrap();
    let installer = PackageInstaller::new(dir.path());
    let mut payload = b"GGUF".to_vec();
    payload.extend(vec![0u8; 32]);
    let sha = harbor_canonical::sha256_hex(&payload);
    let mut m = manifest("escape", &sha, payload.len() as u64);
    m.files[0].path = "../../outside-root.gguf".into();
    let mut staged = installer.begin("escape").unwrap();
    let err = installer
        .ingest_file(&mut staged, &m.files[0], &payload)
        .unwrap_err();
    assert!(
        matches!(err, harbor_modelhub::install::InstallError::PathEscape(_)),
        "a scope-escaping path must be refused: {err}"
    );
    assert!(!dir
        .path()
        .parent()
        .unwrap()
        .join("outside-root.gguf")
        .exists());
}

/// SEC-045: a partially installed multi-file package is INVISIBLE —
/// nothing appears in the installed list until every file is verified
/// and the atomic commit completes.
#[test]
fn sec_045_partial_install_is_invisible_until_atomic_commit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("installed");
    let installer = PackageInstaller::new(&root);
    let mut weights = b"GGUF".to_vec();
    weights.extend(vec![1u8; 64]);
    let config = b"{}".to_vec();
    let m = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: "partial".into(),
        reference_type: "installed_package".into(),
        files: vec![
            PackageFile {
                role: "weights".into(),
                path: "model.gguf".into(),
                sha256: harbor_canonical::sha256_hex(&weights),
                size_bytes: weights.len() as u64,
            },
            PackageFile {
                role: "config".into(),
                path: "config.json".into(),
                sha256: harbor_canonical::sha256_hex(&config),
                size_bytes: config.len() as u64,
            },
        ],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.into()],
        },
    };
    let mut staged = installer.begin("partial").unwrap();
    // Crash after the FIRST file of a multi-file package: one file
    // verified, the second never arrives, commit never runs.
    installer
        .ingest_file(&mut staged, &m.files[0], &weights)
        .unwrap();
    drop(staged); // the "crash"
    assert!(
        installer.installed_packages().unwrap().is_empty(),
        "a partial install must be invisible"
    );
    assert!(
        !root.join("partial").exists(),
        "no partial directory leaked"
    );
}

/// SEC-009: hub credentials ride the Authorization HEADER, never a
/// query string — a static scan of the acquisition sources keeps it
/// that way (tokens in URLs land in logs, captures and referers).
#[test]
fn sec_009_credentials_never_in_query_strings() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let code = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => &text[..],
        };
        for needle in ["?token=", "&token=", "?access_token=", "api_key="] {
            if code.contains(needle) {
                violations.push(format!("{}: {}", path.display(), needle));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "credentials must stay in headers: {violations:?}"
    );
}

/// SEC-012 (catalog tampering): the detached signature covers the
/// canonical bytes of {epoch, published_at, entries} — tampered bytes,
/// foreign signatures and unknown keys all fail verification.
#[test]
fn sec_012_tampered_catalog_entries_fail_verification() {
    use harbor_modelhub::catalog_signing::{sign_catalog, CatalogSigningKey, CatalogVerifier};
    let key = CatalogSigningKey::generate();
    let entries = harbor_canonical::parse(
        r#"{"packages":[{"id":"pkg-a","license":"MIT","context_tokens":512,
           "repo_id":"org/repo","revision":"main","quantization":"Q8_0","tiers":["Test"],
           "files":[{"role":"weights","path":"m.gguf","sha256":"aa","size_bytes":1}]}]}"#,
    )
    .unwrap();
    let signed = sign_catalog(&key, 7, "2026-09-29T00:00:00Z", entries).unwrap();
    let public = hex::encode(key.public_bytes());
    let mut verifier = CatalogVerifier::new(&public).unwrap();
    verifier.verify(&signed).unwrap();

    // Tamper: mutate a hash inside the signed entries (re-serialize with
    // the swap so canonical shape stays valid) — signature must fail.
    let swapped = serde_json::to_string(&signed.entries)
        .unwrap()
        .replace("aa", "bb");
    let mut tampered = signed.clone();
    tampered.entries = harbor_canonical::parse(&swapped).unwrap();
    assert!(
        verifier.verify(&tampered).is_err(),
        "tampered entries must fail the detached signature"
    );

    // Signature stripping/replay: a different entries value wearing the
    // original signature fails.
    let mut forged = signed.clone();
    forged.entries = harbor_canonical::parse(r#"{"packages":[{"id":"evil"}]}"#).unwrap();
    assert!(
        verifier.verify(&forged).is_err(),
        "a signature over different canonical bytes must fail"
    );

    // An attacker's own key is unknown to the verifier.
    let attacker = CatalogSigningKey::generate();
    let attack = sign_catalog(
        &attacker,
        8,
        "2026-09-29T02:00:00Z",
        harbor_canonical::parse(r#"{"packages":[]}"#).unwrap(),
    )
    .unwrap();
    assert!(verifier.verify(&attack).is_err());
}

/// SEC-044 (catalog key rollback/expiry): after accepting epoch N, a
/// catalog at a LOWER or equal epoch is rejected — rollback and replay
/// cannot walk the accepted catalog backwards.
#[test]
fn sec_044_catalog_epoch_rollback_rejected() {
    use harbor_modelhub::catalog_signing::{sign_catalog, CatalogSigningKey, CatalogVerifier};
    let key = CatalogSigningKey::generate();
    let entries = harbor_canonical::parse(r#"{"packages":[]}"#).unwrap();
    let v5 = sign_catalog(&key, 5, "2026-09-29T00:00:00Z", entries.clone()).unwrap();
    let v6 = sign_catalog(&key, 6, "2026-09-29T01:00:00Z", entries.clone()).unwrap();
    let public = hex::encode(key.public_bytes());
    let mut verifier = CatalogVerifier::new(&public).unwrap();

    // Accept epoch 6...
    verifier.verify(&v6).unwrap();
    // ...then epoch 5 (rollback) and a replay of epoch 6 are refused.
    let err = verifier.verify(&v5).unwrap_err();
    assert!(matches!(
        err,
        harbor_modelhub::catalog_signing::CatalogSignError::StaleEpoch {
            accepted: 6,
            got: 5
        }
    ));
    let err = verifier.verify(&v6).unwrap_err();
    assert!(matches!(
        err,
        harbor_modelhub::catalog_signing::CatalogSignError::StaleEpoch {
            accepted: 6,
            got: 6
        }
    ));
}

fn multi_manifest(id: &str, weights: &[u8]) -> PackageManifest {
    let config = b"{}".to_vec();
    PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.into(),
        reference_type: "installed_package".into(),
        files: vec![
            PackageFile {
                role: "weights".into(),
                path: "model.gguf".into(),
                sha256: harbor_canonical::sha256_hex(weights),
                size_bytes: weights.len() as u64,
            },
            PackageFile {
                role: "config".into(),
                path: "config.json".into(),
                sha256: harbor_canonical::sha256_hex(&config),
                size_bytes: config.len() as u64,
            },
        ],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.into()],
        },
    }
}

/// SEC-013 (model substitution): the install is pinned and immutable —
/// the committed package's bytes cannot be replaced by a second install
/// of the same id (idempotent commit keeps the original), and content
/// that does not hash to the manifest never lands.
#[test]
fn sec_013_installed_package_is_pinned_and_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("installed");
    let installer = PackageInstaller::new(&root);
    let mut weights = b"GGUF".to_vec();
    weights.extend(vec![7u8; 128]);
    let m = multi_manifest("pinned", &weights);
    let mut staged = installer.begin("pinned").unwrap();
    installer
        .ingest_file(&mut staged, &m.files[0], &weights)
        .unwrap();
    installer
        .ingest_file(&mut staged, &m.files[1], b"{}")
        .unwrap();
    installer
        .commit(&mut staged, &m, chrono::Utc::now())
        .unwrap();
    let committed_path = root.join("pinned").join("model.gguf");
    assert_eq!(std::fs::read(&committed_path).unwrap(), weights);

    // A substitution attempt: "reinstall" the same id with different
    // bytes. The manifest hash pins the content — ingest refuses.
    let mut evil = b"GGUF".to_vec();
    evil.extend(vec![9u8; 128]);
    let evil_sha = harbor_canonical::sha256_hex(&evil);
    let mut m2 = multi_manifest("pinned", &weights);
    m2.files[0].sha256 = evil_sha; // attacker's manifest claims evil hash
    let mut staged2 = installer.begin("pinned").unwrap();
    installer
        .ingest_file(&mut staged2, &m2.files[0], &evil)
        .unwrap();
    installer
        .ingest_file(&mut staged2, &m2.files[1], b"{}")
        .unwrap();
    // Commit is idempotent: the existing install is kept, untouched.
    installer
        .commit(&mut staged2, &m2, chrono::Utc::now())
        .unwrap();
    assert_eq!(
        std::fs::read(&committed_path).unwrap(),
        weights,
        "the committed package is immutable against reinstallation"
    );
    // And a tampered manifest hash (claiming the original hash over new
    // bytes) never passes ingest at all.
    let mut staged3 = installer.begin("pinned").unwrap();
    let err = installer
        .ingest_file(&mut staged3, &m.files[0], &evil)
        .unwrap_err();
    assert!(
        matches!(err, harbor_modelhub::install::InstallError::HashMismatch(_)),
        "bytes must match the pinned hash: {err}"
    );
}
