//! `knowledge.open` wall time, cold and warm (Session 61 observation).
//!
//! The Flutter live test once saw `knowledge.open` take 6 and 18 minutes
//! while other builds ran, and 0.7 s when quiet. This measures the same
//! call stage by stage so the number is attributable: provider load,
//! the first embed (Metal pipeline compile happens here), and the whole
//! `KnowledgeService::open`, for a fresh data root and for a reopen of an
//! already-populated one. Run it ALONE on a quiet machine:
//!
//!   cargo test -p harbor_ffi --test knowledge_open_timing -- --nocapture --test-threads=1
//!
//! The only assertion is a generous ceiling (a warm reopen must not take
//! minutes), so contention does not make it flaky; the printed
//! `OPEN_TIMING` lines are the record. Skips when a fixture is absent.

use std::path::{Path, PathBuf};
use std::time::Instant;

use harbor_ffi::knowledge::KnowledgeService;
use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use harbor_store::keys::KeyMaterial;

fn fixture(name: &str) -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models")
        .join(name);
    p.exists().then(|| p.canonicalize().unwrap())
}

fn install(models_root: &Path, gguf: &Path, prefix: &str) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    // "embeddinggemma" in the id selects the Gemma instruction policy.
    let id = format!("{}-{}", prefix, &sha[..12]);
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

fn measure(label: &str, file: &str, prefix: &str) {
    let Some(gguf) = fixture(file) else {
        eprintln!("SKIP: {file} fixture absent");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &gguf, prefix);
    let key = KeyMaterial::random();

    // Stage timing on a bare provider (same calls `open` makes first).
    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let model = ModelRef::InstalledPackage {
        package_id: pkg.clone(),
    };
    let t = Instant::now();
    provider.load(&model).unwrap();
    let load = t.elapsed();
    let t = Instant::now();
    provider
        .embed(&model, &["identity probe".to_string()])
        .unwrap();
    let first_embed = t.elapsed();
    let t = Instant::now();
    provider
        .embed(&model, &["identity probe".to_string()])
        .unwrap();
    let second_embed = t.elapsed();
    drop(provider);
    println!(
        "OPEN_TIMING {label} stages: load {load:?}, first embed {first_embed:?}, second embed {second_embed:?}"
    );

    let t = Instant::now();
    let svc = KnowledgeService::open(dir.path(), &pkg, key.clone()).expect("open");
    let cold = t.elapsed();
    svc.ingest(&[(
        "s".into(),
        "Contract".into(),
        "The contract value is 5000 USD.".into(),
    )])
    .unwrap();
    println!("OPEN_TIMING {label} backend {}", svc.backend_state());
    drop(svc);

    let t = Instant::now();
    let svc = KnowledgeService::open(dir.path(), &pkg, key.clone()).expect("reopen");
    let warm = t.elapsed();
    drop(svc);
    let t = Instant::now();
    let svc = KnowledgeService::open(dir.path(), &pkg, key).expect("reopen 2");
    let warm2 = t.elapsed();
    drop(svc);
    println!(
        "OPEN_TIMING {label} open (fresh root, process already used the model once): {cold:?}; reopen {warm:?}; reopen again {warm2:?}"
    );
    assert!(
        warm.as_secs() < 120,
        "a warm reopen took {warm:?}; something other than contention is wrong"
    );
}

#[test]
fn open_timing_embeddinggemma2() {
    measure(
        "embeddinggemma-2",
        "embeddinggemma-2-Q8_0.gguf",
        "embeddinggemma2",
    );
}

#[test]
fn open_timing_bge_small() {
    measure("bge-small", "bge-small-en-v1.5-q8_0.gguf", "bge-small");
}
