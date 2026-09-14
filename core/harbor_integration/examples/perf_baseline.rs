//! Performance baseline measurements on the qualified reference device
//! (15_Performance_Qualification.yaml protocol, harbor.performance/v2).
//!
//! Records RAW samples for: model load, first-token latency, generation
//! throughput, artifact open/recalc/save, RAG indexing rate. Output is a
//! deterministic-structure JSON with device/model/protocol identity so the
//! independent qualification run can diff against it.
//!
//! Run: cargo run -p harbor_integration --example perf_baseline -- \
//!        <repo_root> <qwen_store_root>

use std::collections::BTreeMap;
use std::time::Instant;

use harbor_canonical::JsonValue;
use harbor_inference::provider::{Capabilities, ChatRequest, ModelProvider, ModelRef};
use harbor_inference::GgufLlamaCppProvider;
use harbor_modelhub::install::PackageInstaller;

fn ms(t: Instant) -> u128 {
    t.elapsed().as_millis()
}

fn p50(samples: &mut [u128]) -> u128 {
    samples.sort();
    samples[samples.len() / 2]
}

fn p95(samples: &mut [u128]) -> u128 {
    samples.sort();
    samples[(samples.len() as f64 * 0.95) as usize % samples.len()]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let repo_root = args.get(1).map(String::as_str).expect("repo_root arg");
    let models_root = args
        .get(2)
        .map(String::as_str)
        .expect("models store root arg")
        .to_string();

    // ---- store bootstrap: install both models from repo fixtures ----
    let store = std::path::Path::new(&models_root).to_path_buf();
    for (id, file, expected_sha) in [
        (
            "qwen2.5-1.5b-instruct",
            "qwen2.5-1.5b-instruct-q4_k_m.gguf",
            "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
        ),
        (
            "bge-small-en-v1.5",
            "bge-small-en-v1.5-q8_0.gguf",
            "f046db1dc724cf4f6f0a0c5917e922823b73eb1d27b8f9a9c2797f7866974804",
        ),
    ] {
        let installer = PackageInstaller::new(&store);
        if installer
            .installed_packages()
            .unwrap_or_default()
            .iter()
            .any(|p| p == id)
        {
            continue;
        }
        let bytes = std::fs::read(
            std::path::Path::new(&repo_root)
                .join("fixtures/models")
                .join(file),
        )
        .unwrap_or_else(|e| panic!("model fixture {file}: {e}"));
        let got = harbor_canonical::sha256_hex(&bytes);
        assert_eq!(got, expected_sha, "model fixture integrity: {file}");
        let manifest = harbor_modelhub::install::PackageManifest {
            schema: "harbor.model/v3".into(),
            id: id.into(),
            reference_type: "installed_package".into(),
            files: vec![harbor_modelhub::install::PackageFile {
                role: "weights".into(),
                path: file.into(),
                sha256: harbor_canonical::sha256_hex(&bytes),
                size_bytes: bytes.len() as u64,
            }],
            runtime: harbor_modelhub::install::RuntimeBinding {
                kind: "gguf/llama.cpp".into(),
                min_revision: "0.1.156".into(),
                targets: vec![std::env::consts::ARCH.to_string()],
            },
        };
        assert_eq!(manifest.files[0].sha256, expected_sha);
        assert_eq!(manifest.files[0].sha256, got);
        let mut staged = installer.begin(id).unwrap();
        installer
            .ingest_file(&mut staged, &manifest.files[0], &bytes)
            .unwrap();
        installer
            .commit(&mut staged, &manifest, chrono::Utc::now())
            .unwrap();
    }

    // ---- model load (Qwen2.5-1.5B, the bound production package) ----
    let mut load_samples = Vec::new();
    let mut load_cold_ms: u128 = 0;
    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let model = ModelRef::InstalledPackage {
        package_id: "qwen2.5-1.5b-instruct".into(),
    };
    for n in 0..3 {
        let t = Instant::now();
        provider.load(&model).unwrap();
        let dt = ms(t);
        if n == 0 {
            // First load of a fresh binary: one-time Metal kernel
            // compilation lands here (cold start), not in steady state.
            load_cold_ms = dt;
        } else {
            load_samples.push(dt);
        }
        provider.unload(&model).unwrap();
    }

    // ---- first-token and throughput (greedy, fixed prompt) ----
    provider.load(&model).unwrap();
    let mk_req = |max_tokens: u32| ChatRequest {
        model: model.clone(),
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str("Count from one to twenty.")),
        ])],
        max_tokens,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
    };
    let mut ttft_samples = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let r = provider.generate(mk_req(1)).unwrap();
        assert!(r.usage.completion_tokens >= 1);
        ttft_samples.push(ms(t));
    }
    let mut tps_samples = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let r = provider.generate(mk_req(64)).unwrap();
        let dt = t.elapsed().as_secs_f64();
        tps_samples.push((r.usage.completion_tokens as f64 / dt) as u128);
    }

    // ---- artifact open / recalc / save (board_demo fixture) ----
    let fixture = std::path::Path::new(&repo_root).join("fixtures/office/board_demo.xlsx");
    let bytes = std::fs::read(&fixture).expect("board_demo fixture");
    let mut open_samples = Vec::new();
    let mut recalc_samples = Vec::new();
    let mut save_samples = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let mut doc = harbor_artifacts::WorkbookDoc::load(&bytes).unwrap();
        open_samples.push(ms(t));
        let t = Instant::now();
        doc.recalculate_all().unwrap();
        recalc_samples.push(ms(t));
        let t = Instant::now();
        doc.to_bytes().unwrap();
        save_samples.push(ms(t));
    }

    // ---- RAG indexing rate (embed via bge through the pinned runtime) ----
    let embed_root = models_root.clone();
    let embed_provider = GgufLlamaCppProvider::new(&embed_root).unwrap();
    let embed_model = ModelRef::InstalledPackage {
        package_id: "bge-small-en-v1.5".into(),
    };
    let rag_rate: Option<u128> = if embed_provider.load(&embed_model).is_ok() {
        let docs: Vec<String> = (0..20)
            .map(|i| {
                format!(
                    "Document {i}: quarterly revenue grew steadily across all regions, \
                     with EMEA leading at twelve percent quarter over quarter growth."
                )
            })
            .collect();
        let t = Instant::now();
        let vectors = embed_provider.embed(&embed_model, &docs).unwrap();
        let dt = t.elapsed().as_secs_f64();
        let rate = (docs.len() as f64 / dt * 60.0) as u128;
        println!(
            "rag_docs_per_minute: {rate} ({} docs, dim {})",
            docs.len(),
            vectors[0].len()
        );
        Some(rate)
    } else {
        // Optional measurement requires an explicit unavailable reason,
        // never a fabricated zero (15_Performance_Qualification.yaml).
        println!("rag_docs_per_minute: UNAVAILABLE (bge model not installed)");
        None
    };

    let report = serde_json::json!({
        "schema": "harbor.performance/v2",
        "device": "fixtures/qualification/reference_device_macos_arm64.json",
        "model": {
            "package": "qwen2.5-1.5b-instruct",
            "sha256": "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
        },
        "runtime": "llama.cpp/llama-cpp-sys-2@0.1.156",
        "protocol": "raw samples; p50/p95 derived; greedy decoding",
        "samples": {
            "model_load_cold_first_ms": load_cold_ms,
            "model_load_warm_ms": load_samples,
            "ttft_1tok_ms": ttft_samples,
            "tokens_per_second_64tok": tps_samples,
            "artifact_open_ms": open_samples,
            "artifact_recalc_ms": recalc_samples,
            "artifact_save_ms": save_samples,
        },
        "derived": {
            "model_load_cold_first_ms": load_cold_ms,
            "model_load_warm_ms_p95": p95(&mut load_samples),
            "ttft_ms_p50": p50(&mut ttft_samples),
            "ttft_ms_p95": p95(&mut ttft_samples),
            "tokens_per_second_p50": p50(&mut tps_samples),
            "tokens_per_second_p95": p95(&mut tps_samples),
            "artifact_open_ms_p50": p50(&mut open_samples),
            "artifact_recalc_ms_p50": p50(&mut recalc_samples),
            "artifact_save_ms_p50": p50(&mut save_samples),
            "rag_docs_per_minute": rag_rate,
            "rag_docs_per_minute_note": match rag_rate {
                Some(_) => "measured via bge-small-en-v1.5 through the pinned runtime",
                None => "UNAVAILABLE: bge model not installed (explicit reason, not a zero)",
            },
        },
        "fixed_safety_slo": {
            "cancellation_ack_p95_ms": 250,
            "cancellation_unacknowledged_pause_after_ms": 5000,
            "evidence": "harbor_agent::cancellation tests",
        },
    });
    let out = std::path::Path::new(&repo_root).join("evidence/perf_baseline.json");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("written {}", out.display());
    let _ = BTreeMap::<String, u64>::new();
}
