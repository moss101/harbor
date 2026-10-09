//! Media retrieval quality on REAL data (decision 0015 follow-up).
//!
//! The smoke tests prove plumbing on synthetic media. This measures
//! retrieval on real photos with human captions (Flickr8k) and real speech
//! with transcripts (LibriSpeech): text -> media and media -> text
//! recall@k / MRR over the whole pool.
//!
//! Qualification-machine tier (needs the model + a fetched dataset):
//!
//! ```text
//! python3 tools/fetch_media_benchmark.py /tmp/mmbench 200 73
//! HARBOR_MM_BENCH_DIR=/tmp/mmbench \
//!   cargo test -p harbor_inference --features multimodal \
//!   --test multimodal_benchmark -- --ignored --nocapture
//! ```
//! Evidence: `evidence/media_retrieval/eg2-<weights-hash>.json`.
#![cfg(feature = "multimodal")]

use std::path::{Path, PathBuf};

use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::multimodal::{MediaEmbedder, MediaPart};
use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn root() -> PathBuf {
    // On-device runs push the fixtures next to the binary.
    if let Ok(r) = std::env::var("HARBOR_FIXTURE_ROOT") {
        return PathBuf::from(r);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
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

/// For each query, the 1-based rank of its own gold item among `pool`.
fn ranks(queries: &[(usize, Vec<f32>)], pool: &[Vec<f32>]) -> Vec<usize> {
    queries
        .iter()
        .map(|(gold, q)| {
            let g = cosine(q, &pool[*gold]);
            1 + pool
                .iter()
                .enumerate()
                .filter(|(i, d)| i != gold && cosine(q, d) > g)
                .count()
        })
        .collect()
}

fn summarize(label: &str, ranks: &[usize]) -> serde_json::Value {
    let n = ranks.len() as f64;
    let at = |k: usize| ranks.iter().filter(|r| **r <= k).count() as f64 / n;
    let mrr = ranks.iter().map(|r| 1.0 / *r as f64).sum::<f64>() / n;
    let median = {
        let mut s = ranks.to_vec();
        s.sort();
        s[s.len() / 2]
    };
    println!(
        "BENCH {label}: n={} R@1={:.3} R@5={:.3} R@10={:.3} MRR={:.3} median_rank={median}",
        ranks.len(),
        at(1),
        at(5),
        at(10),
        mrr
    );
    serde_json::json!({ "queries": ranks.len(), "r_at_1": at(1), "r_at_5": at(5),
        "r_at_10": at(10), "mrr": mrr, "median_rank": median })
}

#[test]
#[ignore = "needs the EmbeddingGemma 2 fixtures and HARBOR_MM_BENCH_DIR (tools/fetch_media_benchmark.py)"]
fn real_media_retrieval_quality() {
    let r = root();
    let weights = r.join("fixtures/models/embeddinggemma-2-Q8_0.gguf");
    let mmproj = r.join("fixtures/models/mmproj-embeddinggemma-2-Q8_0.gguf");
    let bench = PathBuf::from(std::env::var("HARBOR_MM_BENCH_DIR").expect("HARBOR_MM_BENCH_DIR"));
    assert!(weights.exists() && mmproj.exists(), "fixtures missing");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bench.join("manifest.json")).unwrap()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let id = "embeddinggemma2-bench".to_string();
    let installer = PackageInstaller::new(&models_root);
    let (mut files, mut blobs) = (Vec::new(), Vec::new());
    for (role, p) in [("weights", &weights), ("mmproj", &mmproj)] {
        let bytes = std::fs::read(p).unwrap();
        files.push(PackageFile {
            role: role.into(),
            path: p.file_name().unwrap().to_string_lossy().to_string(),
            sha256: harbor_canonical::sha256_hex(&bytes),
            size_bytes: bytes.len() as u64,
        });
        blobs.push(bytes);
    }
    let weights_hash = files[0].sha256.clone();
    let m = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files,
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin(&id).unwrap();
    for (f, b) in m.files.iter().zip(&blobs) {
        installer.ingest_file(&mut staged, f, b).unwrap();
    }
    assert!(installer.validate(&staged, &m).unwrap().ok);
    installer
        .commit(&mut staged, &m, chrono::Utc::now())
        .unwrap();

    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let model = ModelRef::InstalledPackage {
        package_id: id.clone(),
    };
    provider.load(&model).unwrap();
    let mm = MediaEmbedder::open(&provider, &id).unwrap();
    let q = |s: &str| {
        provider
            .embed(&model, &[format!("task: search result | query: {s}")])
            .unwrap()
            .remove(0)
    };
    let doc = |s: &str| {
        provider
            .embed(&model, &[format!("title: none | text: {s}")])
            .unwrap()
            .remove(0)
    };

    // ---- images ----
    let images = manifest["images"].as_array().unwrap();
    let t0 = std::time::Instant::now();
    let mut img_vecs = Vec::new();
    for it in images {
        let bytes = std::fs::read(bench.join(it["file"].as_str().unwrap())).unwrap();
        img_vecs.push(mm.embed(&[MediaPart::Image(bytes)]).expect("image embed"));
    }
    let img_ms = t0.elapsed().as_millis() as f64 / images.len() as f64;
    let mut t2i = Vec::new();
    let mut cap_docs = Vec::new(); // first caption as a text document, for i2t
    for (i, it) in images.iter().enumerate() {
        for c in it["captions"].as_array().unwrap() {
            t2i.push((i, q(c.as_str().unwrap())));
        }
        cap_docs.push(doc(it["captions"][0].as_str().unwrap()));
    }
    let t2i_ranks = ranks(&t2i, &img_vecs);
    let i2t: Vec<(usize, Vec<f32>)> = img_vecs.iter().cloned().enumerate().collect();
    let i2t_ranks = ranks(&i2t, &cap_docs);

    // ---- audio ----
    let clips = manifest["audio"].as_array().unwrap();
    let t0 = std::time::Instant::now();
    let mut aud_vecs = Vec::new();
    for it in clips {
        let bytes = std::fs::read(bench.join(it["file"].as_str().unwrap())).unwrap();
        aud_vecs.push(
            mm.embed(&[MediaPart::AudioFile(bytes)])
                .expect("audio embed"),
        );
    }
    let aud_ms = t0.elapsed().as_millis() as f64 / clips.len().max(1) as f64;
    let t2a: Vec<(usize, Vec<f32>)> = clips
        .iter()
        .enumerate()
        .map(|(i, c)| (i, q(c["text"].as_str().unwrap())))
        .collect();
    let t2a_ranks = ranks(&t2a, &aud_vecs);
    let a_docs: Vec<Vec<f32>> = clips
        .iter()
        .map(|c| doc(c["text"].as_str().unwrap()))
        .collect();
    let a2t: Vec<(usize, Vec<f32>)> = aud_vecs.iter().cloned().enumerate().collect();
    let a2t_ranks = ranks(&a2t, &a_docs);

    let report = serde_json::json!({
        "model": "embeddinggemma-2-Q8_0 + mmproj-Q8_0",
        "weights_sha256": weights_hash,
        "runtime": harbor_inference::runtime_identity(),
        "pool": { "images": images.len(), "audio": clips.len() },
        "data": { "images": "Flickr8k test (5 human captions each)",
                  "audio": "LibriSpeech dummy dev-clean (transcripts)" },
        "chance_r_at_1": { "images": 1.0 / images.len() as f64, "audio": 1.0 / clips.len().max(1) as f64 },
        "text_to_image": summarize("text->image", &t2i_ranks),
        "image_to_text": summarize("image->text", &i2t_ranks),
        "text_to_audio": summarize("text->audio", &t2a_ranks),
        "audio_to_text": summarize("audio->text", &a2t_ranks),
        "ms_per_image_embed": img_ms,
        "ms_per_audio_embed": aud_ms,
        "command": "HARBOR_MM_BENCH_DIR=<dir> cargo test -p harbor_inference --features multimodal --test multimodal_benchmark -- --ignored --nocapture",
    });
    // On a device the repo tree is not writable: evidence goes next to the
    // dataset and is pulled back with adb.
    let out = if std::env::var("HARBOR_FIXTURE_ROOT").is_ok() {
        bench.join("evidence")
    } else {
        r.join("evidence/media_retrieval")
    };
    std::fs::create_dir_all(&out).unwrap();
    let path = out.join(format!("eg2-{}.json", &weights_hash[..12]));
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("evidence: {}", path.display());

    // Floors, not quality claims: far above chance, so a broken projector,
    // wrong row scaling or a mis-ordered batch fails loudly.
    let r1 = |v: &[usize]| v.iter().filter(|x| **x == 1).count() as f64 / v.len() as f64;
    assert!(
        r1(&t2i_ranks) > 10.0 * (1.0 / images.len() as f64),
        "text->image near chance"
    );
    assert!(
        r1(&t2a_ranks) > 5.0 * (1.0 / clips.len().max(1) as f64),
        "text->audio near chance"
    );
}
