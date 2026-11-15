//! Cross-platform numerics canary (decision 0022 phase 5): what every
//! runtime bump and every CI platform must reproduce for the production
//! embedder, EmbeddingGemma 2.
//!
//! - the knowledge canary (a paraphrase outranks unrelated sentences, every
//!   value finite) on the default backend AND on the true CPU path;
//! - the default backend agrees with the CPU path (cosine per text);
//! - every trained Matryoshka truncation keeps the canary's ranking.
//!
//! Fixtures are gitignored; the test skips without them unless
//! `HARBOR_REQUIRE_FIXTURES=1` (the CI numerics job), where absence fails.
#![cfg(feature = "gguf-backend")]

use std::path::{Path, PathBuf};

use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

const CANARY: [&str; 4] = [
    "The cat is sitting on the mat.",
    "A cat sits on a mat.",
    "Quarterly revenue increased by ten percent.",
    "The stock market closed higher on Friday.",
];

/// Minimum agreement between the default backend and the CPU path. The
/// 0016 record measured Metal vs CPU within 0.5 % on retrieval scores.
const MIN_BACKEND_COSINE: f32 = 0.99;

fn fixture(name: &str) -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models")
        .join(name);
    if p.exists() {
        return Some(p);
    }
    assert!(
        std::env::var("HARBOR_REQUIRE_FIXTURES").as_deref() != Ok("1"),
        "fixture {name} missing and HARBOR_REQUIRE_FIXTURES=1"
    );
    eprintln!("SKIP: fixture {name} absent");
    None
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    d / (na.sqrt() * nb.sqrt())
}

fn install(root: &Path, weights: &Path) -> ModelRef {
    let id = "embeddinggemma2-canary";
    let bytes = std::fs::read(weights).unwrap();
    let file = PackageFile {
        role: "weights".into(),
        path: weights.file_name().unwrap().to_string_lossy().to_string(),
        sha256: harbor_canonical::sha256_hex(&bytes),
        size_bytes: bytes.len() as u64,
    };
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.into(),
        reference_type: "installed_package".into(),
        files: vec![file.clone()],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let installer = PackageInstaller::new(root);
    let mut staged = installer.begin(id).unwrap();
    installer.ingest_file(&mut staged, &file, &bytes).unwrap();
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    ModelRef::InstalledPackage {
        package_id: id.into(),
    }
}

/// EmbeddingGemma 2's document prompt, as `InstructionPolicy` applies it.
fn doc(t: &str) -> String {
    format!("title: none | text: {t}")
}

fn canary_margin(v: &[Vec<f32>]) -> f32 {
    assert_eq!(v.len(), 4);
    assert!(v
        .iter()
        .all(|x| !x.is_empty() && x.iter().all(|f| f.is_finite())));
    let near = cosine(&v[0], &v[1]);
    near - cosine(&v[0], &v[2]).max(cosine(&v[0], &v[3]))
}

#[test]
fn embeddinggemma2_canary_holds_on_every_backend_and_truncation() {
    let Some(weights) = fixture("embeddinggemma-2-Q8_0.gguf") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let model = install(dir.path(), &weights);
    let texts: Vec<String> = CANARY.iter().map(|t| doc(t)).collect();
    let package = "embeddinggemma2-canary";

    let default = GgufLlamaCppProvider::new(dir.path()).unwrap();
    default.load(&model).unwrap();
    let backend = default.backend_of(package).unwrap();
    let v_default = ModelProvider::embed(&default, &model, &texts).unwrap();

    // The true CPU path, with its own qualification store so the pin it
    // records does not touch the default provider's.
    let state = tempfile::tempdir().unwrap();
    let cpu = GgufLlamaCppProvider::with_runtime_dir(dir.path(), state.path()).unwrap();
    cpu.force_cpu(package);
    cpu.load(&model).unwrap();
    let v_cpu = ModelProvider::embed(&cpu, &model, &texts).unwrap();

    let m_default = canary_margin(&v_default);
    let m_cpu = canary_margin(&v_cpu);
    eprintln!(
        "runtime {} | backend {backend} margin {m_default:.4} | cpu margin {m_cpu:.4}",
        harbor_inference::runtime_identity()
    );
    assert!(m_cpu > 0.02, "CPU canary margin {m_cpu}");
    assert!(m_default > 0.02, "{backend} canary margin {m_default}");
    for (i, (a, b)) in v_default.iter().zip(&v_cpu).enumerate() {
        let c = cosine(a, b);
        eprintln!("  text {i}: {backend} vs cpu cosine {c:.5}");
        assert!(
            c >= MIN_BACKEND_COSINE,
            "{backend} disagrees with the CPU path on text {i}: cosine {c}"
        );
    }
    for d in harbor_inference::mrl::GEMMA_MRL_DIMENSIONS.iter() {
        if *d >= v_cpu[0].len() {
            continue;
        }
        let t: Vec<Vec<f32>> = v_cpu
            .iter()
            .map(|v| harbor_inference::mrl::truncate_normalize(v, *d).unwrap())
            .collect();
        let m = canary_margin(&t);
        eprintln!("  matryoshka {d}: margin {m:.4}");
        assert!(m > 0.02, "Matryoshka {d} canary margin {m}");
    }
}
