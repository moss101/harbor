//! ACC-055 (index identity): reopening a durable knowledge index with a
//! DIFFERENT embedding model rebuilds it from the sealed texts — vectors
//! from two models are never mixed into one searchable index, and search
//! keeps working after the swap.
//!
//! Runs against the real pinned embedding fixtures when present
//! (`fixtures/models/bge-small-en-v1.5-q8_0.gguf` and the Qwen chat
//! fixture, whose mean-pooled embeddings have a different dimension);
//! prints SKIP and passes when the fixtures are absent (CI has no
//! weights — the qualification machine runs this for real).

use std::path::{Path, PathBuf};

use harbor_ffi::knowledge::{KnowledgeService, SourceInput};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use harbor_store::keys::KeyMaterial;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Install a GGUF through the real staged installer (same path the app
/// uses) and return its package id.
fn install(models_root: &Path, gguf: &Path) -> String {
    let bytes = std::fs::read(gguf).expect("fixture readable");
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("identity-{}", &sha[..12]);
    let file_name = gguf
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model.gguf".into());
    let installer = PackageInstaller::new(models_root);
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: file_name,
            sha256: sha,
            size_bytes: bytes.len() as u64,
        }],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin(&id).unwrap();
    installer
        .ingest_file(&mut staged, &manifest.files[0], &bytes)
        .unwrap();
    let report = installer.validate(&staged, &manifest).unwrap();
    assert!(report.ok, "{:?}", report.problems);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    id
}

fn sources() -> Vec<SourceInput> {
    vec![
        (
            "contract".into(),
            "Contract".into(),
            "The contract value is 5000 USD and ends 2026-12-31. \
             This entry is recorded in the official policy register."
                .into(),
        ),
        (
            "travel".into(),
            "Travel".into(),
            "Employees may claim up to 180 USD per night for hotels. \
             Receipts are required above 20 USD."
                .into(),
        ),
        (
            "leave".into(),
            "Leave".into(),
            "Annual leave entitlement is 24 days per year. \
             Sick leave allowance is 10 days per year."
                .into(),
        ),
    ]
}

#[test]
fn identity_change_rebuilds_vectors_and_search_keeps_working() {
    let root = repo_root();
    let bge = root.join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    let qwen = root.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !bge.exists() || !qwen.exists() {
        eprintln!(
            "SKIP: embedding fixtures absent ({} / {})",
            bge.display(),
            qwen.display()
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let data_root = dir.path();
    // The service resolves its embedding packages under <root>/models.
    let models_root = data_root.join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let bge_pkg = install(&models_root, &bge);
    let qwen_pkg = install(&models_root, &qwen);
    let key = KeyMaterial::random();

    // Open under the BGE identity, ingest, search.
    let svc = KnowledgeService::open(data_root, &bge_pkg, key.clone()).unwrap();
    svc.ingest(&sources()).unwrap();
    let bge_identity = svc.identity_hash();
    let bge_dim = svc.embedding_dimension();
    assert_eq!(bge_dim, 384, "bge-small-v1.5 embedding dimension");
    let hits = svc.search("What is the contract value in USD?", 3).unwrap();
    let top = hits["citations"][0]["source_id"].as_str().unwrap();
    assert_eq!(top, "contract", "search finds the right source under bge");
    drop(svc);

    // Reopen with a DIFFERENT embedding model (different dimension): the
    // persisted vectors are incompatible; the service must rebuild from
    // the sealed texts instead of mixing them in.
    let svc = KnowledgeService::open(data_root, &qwen_pkg, key.clone()).unwrap();
    let qwen_identity = svc.identity_hash();
    assert_ne!(
        bge_identity, qwen_identity,
        "identity must bind the embedding model"
    );
    assert_eq!(svc.embedding_dimension(), 1536);
    let sources_list = svc.sources().unwrap();
    assert_eq!(
        sources_list["sources"].as_array().unwrap().len(),
        3,
        "rebuild keeps every source"
    );
    let hits = svc.search("What is the contract value in USD?", 3).unwrap();
    let top = hits["citations"][0]["source_id"].as_str().unwrap();
    assert_eq!(
        top, "contract",
        "search still finds the right source after the identity rebuild"
    );

    // Reopening under the ORIGINAL identity rebuilds back and still works.
    let svc = KnowledgeService::open(data_root, &bge_pkg, key).unwrap();
    assert_eq!(svc.identity_hash(), bge_identity);
    let hits = svc
        .search("How many days of annual leave per year?", 3)
        .unwrap();
    let top = hits["citations"][0]["source_id"].as_str().unwrap();
    assert_eq!(top, "leave");
}
