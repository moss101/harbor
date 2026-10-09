//! EmbeddingGemma 2 on the patched runtime (decision 0013).
//!
//! The official `ggml-org/embeddinggemma-2-GGUF` declares
//! `general.architecture = gemma-embedding2`, which stock llama-cpp-sys-2
//! 0.1.156 cannot load. Harbor's vendored copy back-ports the architecture;
//! this proves the model really loads and embeds through the PRODUCTION
//! path (installer -> KnowledgeService -> instruction policy -> sealed
//! index), not just a bare provider call. Skips (passes) when the
//! fixture is absent.
//!
//! The model card forbids float16 (activations overflow to NaN or
//! silently degrade), so finite, unit-scale, deterministic output is
//! asserted explicitly.

use std::path::{Path, PathBuf};

use harbor_ffi::knowledge::KnowledgeService;
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use harbor_store::keys::KeyMaterial;

fn fixture() -> Option<PathBuf> {
    // On-device runs (Android emulator, iOS simulator) push the model next
    // to the test binary and point at it.
    if let Ok(p) = std::env::var("HARBOR_EG2_FIXTURE") {
        let p = PathBuf::from(p);
        return p.exists().then_some(p);
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models/embeddinggemma-2-Q8_0.gguf");
    p.exists().then(|| p.canonicalize().unwrap())
}

fn install(models_root: &Path, gguf: &Path) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    // "embeddinggemma" in the id selects the Gemma instruction policy.
    let id = format!("embeddinggemma2-{}", &sha[..12]);
    let installer = PackageInstaller::new(models_root);
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: gguf.file_name().unwrap().to_string_lossy().to_string(),
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
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    id
}

#[test]
fn embeddinggemma2_loads_and_retrieves_across_languages() {
    let Some(gguf) = fixture() else {
        eprintln!("SKIP: embeddinggemma-2-Q8_0.gguf fixture absent");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &gguf);

    let svc = KnowledgeService::open(dir.path(), &pkg, KeyMaterial::random())
        .expect("EmbeddingGemma 2 must load on the patched runtime");
    assert_eq!(svc.embedding_dimension(), 768, "native dimension");
    // The numerics canary must have verified SOME backend; a GPU path that
    // fails it (iOS simulator) is transparently replaced by CPU.
    assert_ne!(svc.backend_state(), "cpu_unverified");
    println!("EG2_BACKEND {}", svc.backend_state());

    svc.ingest(&[
        (
            "contract".into(),
            "Contract".into(),
            "The contract value is 5000 USD and ends 2026-12-31.".into(),
        ),
        (
            "travel".into(),
            "Travel".into(),
            "Employees may claim up to 180 USD per night for hotels.".into(),
        ),
        (
            "leave".into(),
            "Leave".into(),
            "Annual leave entitlement is 24 days per year.".into(),
        ),
        (
            "memory:m-tea".into(),
            "Drinks green tea".into(),
            "The user drinks green tea every morning and dislikes coffee.".into(),
        ),
    ])
    .expect("ingest embeds every chunk with finite vectors");

    let top = |q: &str| -> String {
        let r = svc.search(q, 3).unwrap();
        r["citations"][0]["source_id"].as_str().unwrap().to_string()
    };
    assert_eq!(top("What is the contract value in USD?"), "contract");
    assert_eq!(top("How much can I claim for a hotel night?"), "travel");
    assert_eq!(top("How many vacation days do I get?"), "leave");
    // Cross-lingual: Arabic and French questions against English text.
    assert_eq!(top("كم عدد أيام الإجازة السنوية؟"), "leave");
    assert_eq!(top("Quelle est la valeur du contrat ?"), "contract");

    // Memory stays isolated from documents and recalls by meaning.
    let mem = svc
        .search_memory("what does the user like to drink?", 3)
        .unwrap();
    assert_eq!(mem[0].0, "m-tea", "{mem:?}");

    // Determinism and finiteness (the float16 failure mode is NaN or
    // silently degraded vectors).
    let a = svc.search("What is the contract value in USD?", 1).unwrap();
    let b = svc.search("What is the contract value in USD?", 1).unwrap();
    let sa = a["citations"][0]["score"].as_f64().unwrap();
    let sb = b["citations"][0]["score"].as_f64().unwrap();
    assert!(sa.is_finite() && sa > 0.3, "top score {sa}");
    // Machine-readable for cross-backend comparison (Metal vs CPU).
    println!("EG2_TOP_SCORE {sa:.6}");
    assert!((sa - sb).abs() < 1e-6, "non-deterministic: {sa} vs {sb}");
}

#[test]
fn matryoshka_truncation_shrinks_vectors_keeps_retrieval_and_never_mixes() {
    let Some(gguf) = fixture() else {
        eprintln!("SKIP: embeddinggemma-2-Q8_0.gguf fixture absent");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &gguf);
    let key = KeyMaterial::random();
    let docs = || {
        vec![
            (
                "contract".to_string(),
                "Contract".to_string(),
                "The contract value is 5000 USD and ends 2026-12-31.".to_string(),
            ),
            (
                "travel".to_string(),
                "Travel".to_string(),
                "Employees may claim up to 180 USD per night for hotels.".to_string(),
            ),
            (
                "leave".to_string(),
                "Leave".to_string(),
                "Annual leave entitlement is 24 days per year.".to_string(),
            ),
        ]
    };

    // Native index first.
    let native = KnowledgeService::open(dir.path(), &pkg, key.clone()).unwrap();
    native.ingest(&docs()).unwrap();
    let native_identity = native.identity_hash();
    assert_eq!(native.truncation(), None);
    drop(native);

    // Reopen truncated to 256: different identity -> rebuilt from sealed
    // texts, 256-d, still retrieves, scores still unit-scale (re-normalized).
    let svc =
        KnowledgeService::open_with_dimension(dir.path(), &pkg, key.clone(), Some(256)).unwrap();
    assert_eq!(svc.embedding_dimension(), 256);
    assert_eq!(svc.truncation(), Some(256));
    assert_ne!(
        svc.identity_hash(),
        native_identity,
        "truncation is part of identity"
    );
    let top = |q: &str| {
        svc.search(q, 1).unwrap()["citations"][0]["source_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(top("What is the contract value in USD?"), "contract");
    assert_eq!(top("كم عدد أيام الإجازة السنوية؟"), "leave");
    let score = svc.search("hotel night reimbursement", 1).unwrap()["citations"][0]["score"]
        .as_f64()
        .unwrap();
    assert!(
        score.is_finite() && score <= 1.0001 && score > 0.3,
        "score {score}"
    );
    drop(svc);

    // Back to native: rebuilds again, never a mix.
    let back = KnowledgeService::open(dir.path(), &pkg, key.clone()).unwrap();
    assert_eq!(back.identity_hash(), native_identity);
    assert_eq!(back.embedding_dimension(), 768);
    drop(back);

    // A dimension the model was not trained for is refused.
    assert!(
        KnowledgeService::open_with_dimension(dir.path(), &pkg, key.clone(), Some(300)).is_err()
    );
    assert!(KnowledgeService::open_with_dimension(dir.path(), &pkg, key, Some(1024)).is_err());
}
