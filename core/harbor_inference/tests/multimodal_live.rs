//! EmbeddingGemma 2 multimodal embedding on real inputs (decision 0015).
//!
//! 1. Plumbing validity: text-only through the joint-embedding path must
//!    equal the stock text path (cosine ~1) — this proves the hand-built
//!    token rows (token_embd * sqrt(n_embd)) independent of any media.
//! 2. Retrieval: text queries must retrieve the right IMAGE and the right
//!    AUDIO clip, in the same vector space as text.
//!
//! Skips (passes) when the fixtures are absent.
#![cfg(feature = "multimodal")]

use std::path::{Path, PathBuf};

use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::multimodal::{MediaEmbedder, MediaPart};
use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn root() -> PathBuf {
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

fn install(models_root: &Path, weights: &Path, mmproj: &Path) -> String {
    let id = "embeddinggemma2-mm-live".to_string();
    let installer = PackageInstaller::new(models_root);
    let mut files = Vec::new();
    let mut blobs = Vec::new();
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
    for (f, b) in manifest.files.iter().zip(&blobs) {
        installer.ingest_file(&mut staged, f, b).unwrap();
    }
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    id
}

#[test]
fn media_embeddings_share_the_text_space_and_retrieve() {
    let r = root();
    let weights = r.join("fixtures/models/embeddinggemma-2-Q8_0.gguf");
    let mmproj = r.join("fixtures/models/mmproj-embeddinggemma-2-Q8_0.gguf");
    if !weights.exists() || !mmproj.exists() {
        eprintln!("SKIP: embeddinggemma-2 weights / mmproj fixtures absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &weights, &mmproj);
    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let model = ModelRef::InstalledPackage {
        package_id: pkg.clone(),
    };
    provider.load(&model).unwrap();
    let mm = MediaEmbedder::open(&provider, &pkg).expect("media towers open");
    assert!(mm.supports_vision() && mm.supports_audio());
    println!("MM audio_sample_rate {:?}", mm.audio_sample_rate());

    // 1. Plumbing: text through the joint path == stock text path.
    let t = "title: none | text: The quarterly revenue report shows sales grew by ten percent.";
    let stock = provider.embed(&model, &[t.to_string()]).unwrap().remove(0);
    let joint = mm.embed(&[MediaPart::Text(t.into())]).unwrap();
    let same = cosine(&stock, &joint);
    println!("MM text_equivalence_cosine {same:.5}");
    assert!(
        same > 0.999,
        "joint path diverges from stock text path: {same}"
    );

    // 2. Retrieval over real media.
    let media = r.join("fixtures/media");
    let img = |n: &str| MediaPart::Image(std::fs::read(media.join(n)).unwrap());
    let aud = |n: &str| MediaPart::AudioFile(std::fs::read(media.join(n)).unwrap());
    let docs: Vec<(&str, Vec<f32>)> = vec![
        ("invoice.jpg", mm.embed(&[img("invoice.jpg")]).unwrap()),
        ("landscape.jpg", mm.embed(&[img("landscape.jpg")]).unwrap()),
        ("chart.jpg", mm.embed(&[img("chart.jpg")]).unwrap()),
        (
            "speech_revenue.wav",
            mm.embed(&[aud("speech_revenue.wav")]).unwrap(),
        ),
        (
            "speech_weather.wav",
            mm.embed(&[aud("speech_weather.wav")]).unwrap(),
        ),
    ];
    let q = |s: &str| {
        provider
            .embed(&model, &[format!("task: search result | query: {s}")])
            .unwrap()
            .remove(0)
    };
    let rank = |query: &str, among: &[&str]| -> Vec<(String, f32)> {
        let qv = q(query);
        let mut v: Vec<(String, f32)> = docs
            .iter()
            .filter(|(n, _)| among.contains(n))
            .map(|(n, d)| (n.to_string(), cosine(&qv, d)))
            .collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        println!("MM rank {query:?}: {v:?}");
        v
    };
    let images = ["invoice.jpg", "landscape.jpg", "chart.jpg"];
    let audio = ["speech_revenue.wav", "speech_weather.wav"];
    assert_eq!(
        rank("an invoice with the total amount due", &images)[0].0,
        "invoice.jpg"
    );
    assert_eq!(
        rank("a sunny landscape with blue sky and green grass", &images)[0].0,
        "landscape.jpg"
    );
    assert_eq!(
        rank("a bar chart with four columns", &images)[0].0,
        "chart.jpg"
    );
    assert_eq!(
        rank("quarterly revenue and sales growth", &audio)[0].0,
        "speech_revenue.wav"
    );
    assert_eq!(
        rank("sunny weather and a walk in the park", &audio)[0].0,
        "speech_weather.wav"
    );

    // Interleaved input is a single finite vector in the same space.
    let both = mm
        .embed(&[
            MediaPart::Text("title: none | text: Supplier document ".into()),
            img("invoice.jpg"),
        ])
        .unwrap();
    assert!(both.iter().all(|x| x.is_finite()));
    assert_eq!(both.len(), stock.len());
}
