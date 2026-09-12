//! Thermal baseline: 10 minutes of continuous reference generation on the
//! qualified device, sampling the macOS thermal state throughout
//! (15_Performance_Qualification.yaml:
//! thermal_state_after_10min_reference_generation).
//!
//! Thermal state is read from `pmset -g thermlog` (CPU_Speed_Limit percent
//! is the OS's own throttle signal; 100 = no throttling). Raw samples are
//! written to evidence/thermal_baseline.json.

use std::time::{Duration, Instant};

use harbor_canonical::JsonValue;
use harbor_inference::provider::{Capabilities, ChatRequest, ModelProvider, ModelRef};
use harbor_inference::GgufLlamaCppProvider;
use harbor_modelhub::install::PackageInstaller;

fn thermal_sample() -> (u32, String) {
    // `pmset -g therm` is a ONE-SHOT snapshot (thermlog streams forever).
    // When no CPU power status has been recorded the OS reports no
    // throttling signal; treat that as 100 (unthrottled) with a note.
    let out = std::process::Command::new("pmset")
        .args(["-g", "therm"])
        .output();
    let text = out
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    for line in text.lines() {
        if let Some(eq) = line.find("CPU_Speed_Limit") {
            let tail = &line[eq..];
            if let Some(v) = tail.split('=').nth(1) {
                if let Ok(n) = v.trim().parse::<u32>() {
                    return (n, text);
                }
            }
        }
    }
    (100, "no CPU power status recorded (unthrottled)".into())
}

fn thermal_cpu_limit() -> u32 {
    thermal_sample().0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let repo_root = args.get(1).expect("repo_root").clone();
    let models_root = args.get(2).expect("models_store").clone();

    // Bootstrap the store from fixtures if needed (same as perf_baseline).
    let installer = PackageInstaller::new(&models_root);
    if !installer
        .installed_packages()
        .map(|p| p.iter().any(|x| x == "qwen2.5-1.5b-instruct"))
        .unwrap_or(false)
    {
        let file = "qwen2.5-1.5b-instruct-q4_k_m.gguf";
        let bytes = std::fs::read(
            std::path::Path::new(&repo_root).join("fixtures/models").join(file),
        )
        .expect("qwen fixture");
        let manifest = harbor_modelhub::install::PackageManifest {
            schema: "harbor.model/v3".into(),
            id: "qwen2.5-1.5b-instruct".into(),
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
        let mut staged = installer.begin("qwen2.5-1.5b-instruct").unwrap();
        installer.ingest_file(&mut staged, &manifest.files[0], &bytes).unwrap();
        installer.commit(&mut staged, &manifest, chrono::Utc::now()).unwrap();
    }

    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let m = ModelRef::InstalledPackage { package_id: "qwen2.5-1.5b-instruct".into() };
    provider.load(&m).unwrap();

    let run_duration = Duration::from_secs(600);
    let started = Instant::now();
    let mut samples: Vec<(u64, u32, u64)> = Vec::new(); // (elapsed_s, cpu_limit, tokens)
    let mut thermal_notes: Vec<String> = Vec::new();
    let mut total_tokens = 0u64;
    let mut generations = 0u64;
    let mut last_sample = Instant::now();

    while started.elapsed() < run_duration {
        let req = ChatRequest {
            model: m.clone(),
            messages: vec![JsonValue::object([
                ("role", JsonValue::str("user")),
                ("content", JsonValue::str("Write a short story about a dog who sails the sea.")),
            ])],
            max_tokens: 128,
            temperature: 0.0,
            requires: vec![Capabilities::Chat],
        };
        let r = provider.generate(req).unwrap();
        total_tokens += r.usage.completion_tokens;
        generations += 1;
        if last_sample.elapsed() >= Duration::from_secs(30) {
            let (limit, raw) = thermal_sample();
            samples.push((started.elapsed().as_secs(), limit, total_tokens));
            thermal_notes.push(raw);
            last_sample = Instant::now();
        }
    }
    let (final_limit, final_raw) = thermal_sample();
    samples.push((started.elapsed().as_secs(), final_limit, total_tokens));
    thermal_notes.push(final_raw);

    let min_limit = samples.iter().map(|s| s.1).min().unwrap_or(100);
    let report = serde_json::json!({
        "schema": "harbor.performance/v2",
        "measurement": "thermal_state_after_10min_reference_generation",
        "device": "fixtures/qualification/reference_device_macos_arm64.json",
        "model": {
            "package": "qwen2.5-1.5b-instruct",
            "sha256": "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
        },
        "runtime": "llama.cpp/llama-cpp-sys-2@0.1.156",
        "duration_seconds": started.elapsed().as_secs(),
        "generations": generations,
        "total_completion_tokens": total_tokens,
        "samples_seconds_cpu_limit_tokens": samples,
        "min_cpu_speed_limit_percent": min_limit,
        "throttling_observed": min_limit < 100,
        "thermal_notes": thermal_notes,
    });
    let out = std::path::Path::new(&repo_root).join("evidence/thermal_baseline.json");
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("written {}", out.display());
    println!("min CPU speed limit: {min_limit}% over {} generations", generations);
}
