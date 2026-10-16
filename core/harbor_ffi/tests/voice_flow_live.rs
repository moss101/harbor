//! Voice (decision 0019): a spoken request is transcribed by the operating
//! system's own on-device speech model, then enters the SAME typed pipeline
//! as typed text - EmbeddingGemma 2 routes it to a skill. Nothing is
//! authorized by speech. macOS 26+ only; skips without the adapter dylib or
//! the embedding fixture.

#![cfg(target_os = "macos")]

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};
use harbor_inference::asr::{register_asr_host, AsrHostVtable};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use libloading::{Library, Symbol};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn install(models_root: &Path, gguf: &Path) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("embeddinggemma2-{}", &sha[..12]);
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

fn call(
    h: *mut harbor_ffi::WorkspaceHandle,
    method: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let req =
        CString::new(serde_json::json!({"method": method, "args": args}).to_string()).unwrap();
    let raw = unsafe { harbor_core_call(h, req.as_ptr()) };
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
    unsafe { harbor_core_string_free(raw) };
    serde_json::from_str(&text).unwrap()
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .replace(['-', ','], " ")
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

fn wer(got: &str, want: &str) -> f64 {
    let (g, w) = (words(got), words(want));
    let mut prev: Vec<usize> = (0..=w.len()).collect();
    for (i, gi) in g.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, wj) in w.iter().enumerate() {
            let sub = prev[j] + usize::from(gi != wj);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[w.len()] as f64 / w.len().max(1) as f64
}

#[test]
fn a_spoken_request_is_transcribed_on_device_and_routed_like_typed_text() {
    let r = root();
    let dylib = r.join("native/apple/asr_host/build/libharbor_asr_host.dylib");
    let eg2 = r.join("fixtures/models/embeddinggemma-2-Q8_0.gguf");
    if !dylib.exists() || !eg2.exists() {
        eprintln!("SKIP: speech adapter or embedding fixture absent");
        return;
    }
    unsafe {
        let lib = Library::new(&dylib).unwrap();
        let d: Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_char> =
            lib.get(b"harbor_asr_host_descriptor").unwrap();
        let t: Symbol<
            unsafe extern "C" fn(
                *const std::ffi::c_char,
                *const u8,
                usize,
                *const std::sync::atomic::AtomicBool,
                *mut *mut std::ffi::c_char,
            ) -> std::ffi::c_int,
        > = lib.get(b"harbor_asr_host_transcribe").unwrap();
        let f: Symbol<unsafe extern "C" fn(*mut std::ffi::c_char)> =
            lib.get(b"harbor_asr_host_free_string").unwrap();
        register_asr_host(AsrHostVtable {
            descriptor: *d,
            transcribe: *t,
            free_string: *f,
        })
        .unwrap();
        std::mem::forget(lib);
    }
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("models")).unwrap();
    let emb = install(&data.join("models"), &eg2);
    let root_c = CString::new(data.to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-voice").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root_c.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null());
    assert_eq!(
        call(h, "knowledge.open", serde_json::json!({"package_id": emb}))["ok"],
        true
    );

    let truth: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(r.join("fixtures/speech/truth.json")).unwrap(),
    )
    .unwrap();
    let transcribe = |name: &str, locale: &str| {
        let wav = std::fs::read(r.join("fixtures/speech").join(format!("{name}.wav"))).unwrap();
        call(
            h,
            "tools.read",
            serde_json::json!({
            "tool": "speech.transcribe", "args": {"artifact_id": "rec", "locale": locale},
            "artifacts": [{"id": "rec", "name": format!("{name}.wav"),
                           "data_b64": base64::engine::general_purpose::STANDARD.encode(&wav)}]}),
        )
    };

    for name in ["review_overdue", "invoice_amount", "business_report"] {
        let out = transcribe(name, "en_US");
        assert_eq!(out["ok"], true, "{out}");
        let text = out["result"]["text"].as_str().unwrap();
        let w = wer(text, truth[name].as_str().unwrap());
        println!(
            "SPEECH {name}: WER={w:.3} engine={} :: {text}",
            out["result"]["engine"]
        );
        assert_eq!(out["result"]["engine"], "apple-speech-transcriber");
        if name != "invoice_amount" {
            assert!(w <= 0.10, "{name}: WER {w}");
        }
    }
    // Spoken numbers come back as digits: the figure survives, in a form the
    // invoice checks can ground.
    let amount = transcribe("invoice_amount", "en_US")["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        amount.contains("4,200") || amount.contains("4200"),
        "{amount}"
    );
    assert!(
        amount.contains("1001") || amount.contains("1,001"),
        "{amount}"
    );

    // Arabic is not covered by this OS model: refused with the languages that are.
    let ar = transcribe("review_overdue", "ar_SA");
    assert_eq!(ar["ok"], false, "{ar}");
    assert!(
        ar["error"].as_str().unwrap().contains("not supported"),
        "{ar}"
    );
    assert!(ar["error"].as_str().unwrap().contains("en_US"), "{ar}");

    // The transcript is ordinary text: it routes exactly like typed text.
    let spoken = transcribe("review_overdue", "en_US")["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let routed = call(h, "skills.suggest", serde_json::json!({"text": spoken}));
    println!("ROUTED: {}", routed["result"]);
    assert_eq!(routed["ok"], true, "{routed}");
    // Calibrated EmbeddingGemma 2 recommends it with confidence (not an abstention).
    assert_eq!(routed["result"]["abstained"], false, "{routed}");
    assert_eq!(
        routed["result"]["ranked"][0]["skill_id"],
        "supplier-invoice-review"
    );
    let typed = call(
        h,
        "skills.suggest",
        serde_json::json!({"text": "Review the supplier invoices and tell me which payments are overdue."}),
    );
    assert_eq!(
        routed["result"]["suggestions"][0]["skill_id"],
        typed["result"]["suggestions"][0]["skill_id"]
    );
    unsafe { harbor_core_close(h) };
}
