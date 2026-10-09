//! Multimodal retrieval through the production service (decision 0015):
//! images and audio are indexed as `media:` sources into the SAME index as
//! text, found by ordinary text queries, searchable by an image, dropped
//! (not mixed) when the embedding identity changes, and freed on release.
//!
//! Needs the EmbeddingGemma 2 weights + mmproj fixtures; skips otherwise.
#![cfg(feature = "multimodal")]

use std::path::{Path, PathBuf};

use harbor_ffi::knowledge::KnowledgeService;
use harbor_inference::multimodal::MediaPart;
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use harbor_store::keys::KeyMaterial;

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

fn install(models_root: &Path, weights: &Path, mmproj: &Path) -> String {
    // "embeddinggemma" in the id selects the Gemma instruction policy.
    let id = "embeddinggemma2-multimodal".to_string();
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
fn images_and_audio_are_retrievable_beside_text_and_never_mixed_across_identities() {
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
    let key = KeyMaterial::random();
    let media = r.join("fixtures/media");
    let bytes = |n: &str| std::fs::read(media.join(n)).unwrap();
    let hash = |b: &[u8]| harbor_canonical::sha256_hex(b);

    let svc = KnowledgeService::open(dir.path(), &pkg, key.clone()).unwrap();
    assert!(svc.supports_media());

    // Text documents and media items share one index.
    svc.ingest(&[
        (
            "invoice-note".into(),
            "Supplier invoice".into(),
            "Invoice from Acme Trading: total due 1,250 USD, payment terms 30 days.".into(),
        ),
        (
            "weather-note".into(),
            "Weather".into(),
            "Tomorrow will be sunny with a light breeze.".into(),
        ),
    ])
    .unwrap();
    for (id, title, file) in [
        ("invoice", "Scanned invoice", "invoice.jpg"),
        ("landscape", "Park photo", "landscape.jpg"),
        ("chart", "Revenue chart", "chart.jpg"),
    ] {
        let b = bytes(file);
        svc.ingest_media(
            id,
            title,
            &format!("[image] {title}"),
            &[MediaPart::Image(b.clone())],
            &hash(&b),
        )
        .unwrap();
    }
    let b = bytes("speech_revenue.wav");
    svc.ingest_media(
        "revenue-call",
        "Revenue call recording",
        "[audio] Revenue call recording",
        &[MediaPart::AudioFile(b.clone())],
        &hash(&b),
    )
    .unwrap();

    // Ordinary TEXT search finds media (the point of one shared space).
    let top = |q: &str, n: usize| -> Vec<String> {
        svc.search(q, n).unwrap()["citations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["source_id"].as_str().unwrap().to_string())
            .collect()
    };
    let hits = top("a sunny landscape with blue sky and green grass", 3);
    assert!(hits.contains(&"media:landscape".to_string()), "{hits:?}");
    let hits = top("a bar chart of quarterly revenue", 3);
    assert!(hits.contains(&"media:chart".to_string()), "{hits:?}");
    let hits = top("recording about quarterly revenue and sales growth", 3);
    assert!(hits.contains(&"media:revenue-call".to_string()), "{hits:?}");

    // Search BY an image: the invoice picture finds the invoice text note
    // (cross-modal, image -> text) ahead of the weather note.
    let by_img = svc
        .search_by_media(&[MediaPart::Image(bytes("invoice.jpg"))], 6)
        .unwrap();
    let ids: Vec<String> = by_img["citations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["source_id"].as_str().unwrap().to_string())
        .collect();
    let pos = |s: &str| ids.iter().position(|x| x == s).unwrap_or(usize::MAX);
    assert!(pos("invoice-note") < pos("weather-note"), "{ids:?}");

    // Release frees the media towers; the next media call reopens them.
    svc.release();
    let again = svc
        .search_by_media(&[MediaPart::Image(bytes("landscape.jpg"))], 1)
        .unwrap();
    assert_eq!(again["citations"][0]["source_id"], "media:landscape");
    drop(svc);

    // Identity change (Matryoshka 256): text re-embeds from sealed text;
    // media cannot (its source is the media, which is not kept), so it is
    // DROPPED and reported — never silently turned into caption vectors.
    let svc = KnowledgeService::open_with_dimension(dir.path(), &pkg, key, Some(256)).unwrap();
    let mut dropped = svc.dropped_media().to_vec();
    dropped.sort();
    assert_eq!(
        dropped,
        vec![
            "media:chart",
            "media:invoice",
            "media:landscape",
            "media:revenue-call"
        ]
    );
    let sources = svc.sources().unwrap();
    let ids: Vec<&str> = sources["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["source_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"invoice-note") && !ids.iter().any(|i| i.starts_with("media:")));

    // And media can be re-imported into the truncated space (256-d).
    let b = bytes("chart.jpg");
    svc.ingest_media(
        "chart",
        "Revenue chart",
        "[image] Revenue chart",
        &[MediaPart::Image(b.clone())],
        &hash(&b),
    )
    .unwrap();
    let hits = top_of(&svc, "a bar chart of quarterly revenue");
    assert!(hits.contains(&"media:chart".to_string()), "{hits:?}");
}

fn top_of(svc: &KnowledgeService, q: &str) -> Vec<String> {
    svc.search(q, 3).unwrap()["citations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["source_id"].as_str().unwrap().to_string())
        .collect()
}

mod ops {
    //! The same capability through the JSON op surface the app uses
    //! (base64 in, background op out).
    use super::*;
    use base64::Engine as _;
    use std::ffi::{CStr, CString};

    struct Handle(*mut harbor_ffi::WorkspaceHandle);
    impl Handle {
        fn call(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
            let req = CString::new(serde_json::json!({"method": method, "args": args}).to_string())
                .unwrap();
            let raw = unsafe { harbor_ffi::harbor_core_call(self.0, req.as_ptr()) };
            let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
            unsafe { harbor_ffi::harbor_core_string_free(raw) };
            serde_json::from_str(&text).unwrap()
        }
        fn ok(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
            let v = self.call(method, args);
            assert!(v["ok"].as_bool().unwrap_or(false), "{method}: {v}");
            v["result"].clone()
        }
        fn run_op(&self, start: &str, args: serde_json::Value) -> serde_json::Value {
            let op = self.ok(start, args)["op_id"].as_str().unwrap().to_string();
            for _ in 0..600 {
                let st = self.ok("op.status", serde_json::json!({ "op_id": op }));
                match st["state"].as_str().unwrap() {
                    "running" => std::thread::sleep(std::time::Duration::from_millis(250)),
                    "done" => return st["result"].clone(),
                    other => panic!("{start} ended {other}: {st}"),
                }
            }
            panic!("{start} timed out");
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { harbor_ffi::harbor_core_close(self.0) };
        }
    }

    #[test]
    fn media_ops_roundtrip_and_guard_reserved_ids() {
        let r = root();
        let weights = r.join("fixtures/models/embeddinggemma-2-Q8_0.gguf");
        let mmproj = r.join("fixtures/models/mmproj-embeddinggemma-2-Q8_0.gguf");
        if !weights.exists() || !mmproj.exists() {
            eprintln!("SKIP: fixtures absent");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("models")).unwrap();
        let pkg = install(&data.join("models"), &weights, &mmproj);
        let root_c = CString::new(data.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-mm").unwrap();
        let hex = CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10")
            .unwrap();
        let h = harbor_ffi::harbor_core_open_ex(root_c.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
        assert!(!h.is_null());
        let h = Handle(h);

        let opened = h.ok("knowledge.open", serde_json::json!({ "package_id": pkg }));
        assert_eq!(opened["multimodal"], true);
        assert_eq!(opened["dropped_media"], serde_json::json!([]));

        // Media ids are reserved: a document ingest cannot claim one.
        let bad = h.call(
            "knowledge.ingest",
            serde_json::json!({"sources": [{"id": "media:x", "title": "t", "text": "x"}]}),
        );
        assert!(!bad["ok"].as_bool().unwrap_or(false));

        let b64 = |n: &str| {
            base64::engine::general_purpose::STANDARD
                .encode(std::fs::read(r.join("fixtures/media").join(n)).unwrap())
        };
        let res = h.run_op(
            "op.start_ingest_media",
            serde_json::json!({"id": "inv", "title": "Scanned invoice", "kind": "image",
                               "data_b64": b64("invoice.jpg"), "caption": "supplier invoice"}),
        );
        assert_eq!(res["source_id"], "media:inv");
        let found = h.ok(
            "knowledge.search",
            serde_json::json!({"question": "an invoice with the total amount due", "top_k": 3}),
        );
        assert_eq!(found["citations"][0]["source_id"], "media:inv");
        // Search by the same picture.
        let by = h.run_op(
            "op.start_search_media",
            serde_json::json!({"kind": "image", "data_b64": b64("invoice.jpg"), "top_k": 2}),
        );
        assert_eq!(by["citations"][0]["source_id"], "media:inv");
        // Bad kind / oversize / empty are typed errors, not panics.
        assert!(!h.call(
            "op.start_ingest_media",
            serde_json::json!({"title": "t", "kind": "video", "data_b64": b64("invoice.jpg")})
        )["ok"]
            .as_bool()
            .unwrap_or(false));
        assert!(!h.call(
            "op.start_ingest_media",
            serde_json::json!({"title": "t", "kind": "image", "data_b64": ""})
        )["ok"]
            .as_bool()
            .unwrap_or(false));
    }
}
