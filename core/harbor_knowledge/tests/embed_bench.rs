//! Embedding cost on this machine (decision 0019): load time, per-passage
//! latency, throughput and resident memory for one embedder over the SAME
//! passages (the evaluation corpus chunks, both languages). Run once per
//! model:
//!
//! ```text
//! HARBOR_EMBED_MODEL_GGUF=fixtures/models/embeddinggemma-2-Q8_0.gguf \
//!   cargo test --release -p harbor_knowledge --test embed_bench -- --ignored --nocapture
//! ```
//! Memory is the process RSS sampled with `ps` (load, then after every 25
//! passages): an upper-bound proxy, not a PERF-01 measurement.

use std::path::{Path, PathBuf};

use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_inference::GgufLlamaCppProvider;
use harbor_knowledge::chunk::{Chunker, ChunkerConfig};
use harbor_knowledge::corpus::{parse_corpus, CORPUS_AR, CORPUS_EN};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}

#[test]
#[ignore = "needs HARBOR_EMBED_MODEL_GGUF; qualification-machine tier"]
fn embedding_cost() {
    let gguf = PathBuf::from(
        std::env::var("HARBOR_EMBED_MODEL_GGUF").expect("set HARBOR_EMBED_MODEL_GGUF"),
    );
    let bytes = std::fs::read(&gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let fname = gguf.file_name().unwrap().to_string_lossy().to_lowercase();
    let family = if fname.contains("embeddinggemma") {
        "embeddinggemma2"
    } else if fname.contains("bge-m3") {
        "bge-m3"
    } else {
        "multilingual-e5-small"
    };
    let id = format!("{family}-{}", &sha[..12]);
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let installer = PackageInstaller::new(&models_root);
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
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    drop(bytes);

    let rss0 = rss_mb();
    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let model = ModelRef::InstalledPackage {
        package_id: id.clone(),
    };
    let t0 = std::time::Instant::now();
    provider.load(&model).unwrap();
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let policy = harbor_knowledge::instructions::InstructionPolicy::for_package(&id);
    let cfg = ChunkerConfig {
        target_graphemes: 800,
        overlap_graphemes: 80,
        respect_paragraphs: true,
    };
    let mut passages: Vec<(String, String)> = Vec::new();
    for json in [CORPUS_EN, CORPUS_AR] {
        for s in parse_corpus(json).unwrap().sources {
            for c in Chunker::chunk(&s.text, &cfg) {
                passages.push((s.title.clone(), c.text));
            }
        }
    }
    passages.truncate(240);
    // Warm-up (shader/pipeline compilation) is not part of steady state.
    for (t, p) in passages.iter().take(5) {
        provider
            .embed(&model, &[policy.format_document(t, p)])
            .unwrap();
    }
    let mut lat = Vec::new();
    let mut peak = rss_mb();
    let total = std::time::Instant::now();
    for (i, (t, p)) in passages.iter().enumerate() {
        let input = policy.format_document(t, p);
        let s = std::time::Instant::now();
        let v = provider.embed(&model, &[input]).unwrap();
        lat.push(s.elapsed().as_secs_f64() * 1000.0);
        assert!(v[0].iter().all(|x| x.is_finite()));
        if i % 25 == 0 {
            peak = peak.max(rss_mb());
        }
    }
    let secs = total.elapsed().as_secs_f64();
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let dim = provider.embed(&model, &["x".into()]).unwrap()[0].len();
    println!(
        "BENCH {family}: weights={:.0}MB dim={dim} load={load_ms:.0}ms passages={} p50={:.1}ms p95={:.1}ms throughput={:.0}/min rss_before={rss0:.0}MB rss_peak={peak:.0}MB",
        std::fs::metadata(&gguf).unwrap().len() as f64 / 1e6,
        passages.len(),
        lat[lat.len() / 2],
        lat[lat.len() * 95 / 100],
        passages.len() as f64 / secs * 60.0
    );
    let _ = Path::new("");
}
