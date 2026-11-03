//! The GPU qualification store through the production knowledge path
//! (decision 0022 phase 1), on whatever device runs it:
//!
//! - Mac (Metal): the canary passes on the GPU; the key is `qualified`; a
//!   second open stays on the GPU.
//! - iOS simulator / a GPU that embeds wrongly: the first open falls back
//!   (`cpu_fallback`) and records `canary_failed`; a second open goes
//!   straight to the CPU (`cpu_pinned`) without retrying the GPU.
//! - CPU-only build or device: `cpu`, nothing recorded.
//!
//! Prints the verdict so on-device runs leave a readable record. Skips when
//! the fixture is absent (`HARBOR_EG2_FIXTURE` points at it on devices).

use std::path::{Path, PathBuf};

use harbor_ffi::knowledge::KnowledgeService;
use harbor_inference::resilience::{runtime_dir_for, QualState, RuntimeState};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use harbor_store::keys::KeyMaterial;

fn fixture() -> Option<PathBuf> {
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
    let id = "embeddinggemma2-qual".to_string();
    let file = PackageFile {
        role: "weights".into(),
        path: gguf.file_name().unwrap().to_string_lossy().to_string(),
        sha256: harbor_canonical::sha256_hex(&bytes),
        size_bytes: bytes.len() as u64,
    };
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files: vec![file.clone()],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let installer = PackageInstaller::new(models_root);
    let mut staged = installer.begin(&id).unwrap();
    installer.ingest_file(&mut staged, &file, &bytes).unwrap();
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    id
}

#[test]
fn the_store_remembers_the_gpu_verdict_across_opens() {
    let Some(gguf) = fixture() else {
        eprintln!("SKIP: embeddinggemma-2-Q8_0.gguf fixture absent");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let models = dir.path().join("models");
    let pkg = install(&models, &gguf);

    let first = KnowledgeService::open(dir.path(), &pkg, KeyMaterial::random())
        .unwrap()
        .backend_state();
    let state = RuntimeState::open(runtime_dir_for(&models));
    let records = state.records();
    let second = KnowledgeService::open(dir.path(), &pkg, KeyMaterial::random())
        .unwrap()
        .backend_state();
    println!(
        "GPU-QUALIFICATION runtime={} first={first} second={second} records={:?}",
        harbor_inference::runtime_identity(),
        records
            .iter()
            .map(|r| (
                r.key.backend.as_str(),
                r.key.device.as_str(),
                r.state.as_str()
            ))
            .collect::<Vec<_>>()
    );
    match first {
        "gpu" => {
            assert_eq!(second, "gpu");
            assert_eq!(records.len(), 1, "{records:?}");
            assert_eq!(records[0].state, QualState::Qualified);
        }
        "cpu_fallback" => {
            assert_eq!(second, "cpu_pinned", "the pin must survive the open");
            assert_eq!(records.len(), 1, "{records:?}");
            assert_eq!(records[0].state, QualState::CanaryFailed);
        }
        "cpu" => {
            assert_eq!(second, "cpu");
            assert!(records.is_empty(), "{records:?}");
        }
        other => panic!("unexpected first backend state {other}"),
    }
}
