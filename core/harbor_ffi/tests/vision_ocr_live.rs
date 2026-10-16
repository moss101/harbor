//! Portable OCR (decision 0019): the SAME fixtures the Apple Vision test
//! reads, read instead by vision-language models running on the GGUF
//! runtime - the engine every platform has. Measures character error rate
//! against the known text for each installed vision model. Skips without
//! the model fixtures.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use harbor_core::tools::{MemoryArtifacts, ToolContext, ToolRegistry};
use harbor_inference::ocr::register_ocr_engine;
use harbor_inference::provider::ModelRef;
use harbor_inference::vision_ocr::VisionOcr;
use harbor_inference::{GgufLlamaCppProvider, ModelProvider};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn install(models_root: &Path, id: &str, weights: &Path, mmproj: &Path) {
    let installer = PackageInstaller::new(models_root);
    let (mut files, mut blobs) = (Vec::new(), Vec::new());
    for (role, p) in [("weights", weights), ("mmproj", mmproj)] {
        let bytes = std::fs::read(p).unwrap();
        files.push(PackageFile {
            role: role.into(),
            path: p.file_name().unwrap().to_string_lossy().to_string(),
            sha256: harbor_canonical::sha256_hex(&bytes),
            size_bytes: bytes.len() as u64,
        });
        blobs.push(bytes);
    }
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.into(),
        reference_type: "installed_package".into(),
        files,
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin(id).unwrap();
    for (f, b) in manifest.files.iter().zip(&blobs) {
        installer.ingest_file(&mut staged, f, b).unwrap();
    }
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn norm(s: &str) -> Vec<char> {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect()
}

fn cer(got: &str, want: &str) -> f64 {
    let (g, w) = (norm(got), norm(want));
    edit_distance(&g, &w) as f64 / w.len().max(1) as f64
}

fn defer_clear() {
    harbor_inference::ocr::clear_ocr_host();
}

fn run(fixture: &str) -> Result<Value, String> {
    let bytes = std::fs::read(root().join("fixtures/ocr").join(fixture)).unwrap();
    let artifacts = MemoryArtifacts::new().with("scan", fixture, bytes);
    let host = json!({});
    let cancel = AtomicBool::new(false);
    let ctx = ToolContext::new(&artifacts, &host, &cancel);
    let registry = ToolRegistry::builtin();
    let allow: BTreeSet<String> = registry.ids().into_iter().collect();
    registry
        .call(&ctx, "ocr.extract", &json!({"artifact_id": "scan"}), &allow)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

#[test]
#[ignore = "loads two real vision-language models (~10 min incl. a CPU run); qualification-machine tier"]
fn vision_language_models_read_the_same_scans_on_the_portable_runtime() {
    let r = root();
    let models = r.join("fixtures/models");
    let candidates = [
        (
            "smolvlm2-500m",
            "SmolVLM2-500M-Video-Instruct-Q8_0.gguf",
            "mmproj-SmolVLM2-500M-Video-Instruct-Q8_0.gguf",
        ),
        (
            "qwen2.5-vl-3b",
            "Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf",
            "mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf",
        ),
    ];
    let truth: Value =
        serde_json::from_str(&std::fs::read_to_string(r.join("fixtures/ocr/truth.json")).unwrap())
            .unwrap();
    let want = |name: &str, page: usize| -> String {
        truth[name]["pages"][page]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut ran = 0;
    for (id, w, m) in candidates {
        let (w, m) = (models.join(w), models.join(m));
        if !w.exists() || !m.exists() {
            eprintln!("SKIP {id}: fixtures absent");
            continue;
        }
        ran += 1;
        let dir = tempfile::tempdir().unwrap();
        let models_root = dir.path().join("models");
        std::fs::create_dir_all(&models_root).unwrap();
        install(&models_root, id, &w, &m);
        let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
        provider
            .load(&ModelRef::InstalledPackage {
                package_id: id.into(),
            })
            .unwrap();
        if let Ok(only) = std::env::var("HARBOR_VLM_ONLY") {
            if only != id {
                continue;
            }
        }
        let (engine, backend) = VisionOcr::open_verified(
            &provider,
            &ModelRef::InstalledPackage {
                package_id: id.into(),
            },
            id,
        )
        .unwrap();
        println!("RESULT {id} backend={backend}");
        register_ocr_engine(Arc::new(engine));

        for (fixture, key, page) in [
            ("scanned_invoice_en.pdf", "scanned_invoice_en.pdf", 0usize),
            (
                "scanned_invoice_en_degraded.pdf",
                "scanned_invoice_en_degraded.pdf",
                0,
            ),
            ("photo_invoice_en.jpg", "scanned_invoice_en_degraded.pdf", 0),
            ("scanned_invoice_batch.pdf", "scanned_invoice_batch.pdf", 0),
            ("scanned_invoice_ar.pdf", "scanned_invoice_ar.pdf", 0),
        ] {
            let t0 = std::time::Instant::now();
            match run(fixture) {
                Ok(out) => {
                    let text = out["text"].as_str().unwrap();
                    println!(
                        "RESULT {id} {fixture}: CER={:.4} conf={} {:.1}s engine={}\n{}",
                        cer(text, &want(key, page)),
                        out["pages"][0]["confidence"],
                        t0.elapsed().as_secs_f64(),
                        out["engine"],
                        text
                    );
                }
                Err(e) => println!("RESULT {id} {fixture}: ERROR {e}"),
            }
        }
        let mixed = run("mixed_text_and_scan.pdf");
        // Drop the engine before its provider (a global registry would
        // otherwise keep the projector alive past backend teardown).
        defer_clear();
        println!(
            "RESULT {id} mixed: {}",
            mixed
                .as_ref()
                .map(|o| o["pages"].to_string())
                .unwrap_or_else(|e| e.clone())
        );
    }
    if ran == 0 {
        eprintln!("SKIP: no vision model fixtures");
    }
}
