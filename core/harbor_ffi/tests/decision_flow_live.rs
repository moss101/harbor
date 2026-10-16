//! The decision layer through the FFI (decision 0019): what kind of action
//! does a request need? Run with EmbeddingGemma 2 and with BGE-M3 - the
//! embedder is the only thing that changes; no chat model is involved.
//! Skips without the fixtures.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn install(models_root: &Path, gguf: &Path, family: &str) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("{family}-{}", &sha[..12]);
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

#[test]
fn the_same_decisions_come_from_either_embedder_and_nothing_is_authorized() {
    let r = root();
    let mut ran = 0;
    for (family, file) in [
        ("embeddinggemma2", "embeddinggemma-2-Q8_0.gguf"),
        ("bge-m3", "bge-m3-q8_0.gguf"),
    ] {
        let gguf = r.join("fixtures/models").join(file);
        if !gguf.exists() {
            eprintln!("SKIP {family}");
            continue;
        }
        ran += 1;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("models")).unwrap();
        let pkg = install(&data.join("models"), &gguf, family);
        let root_c = CString::new(data.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-decision").unwrap();
        let hex = CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10")
            .unwrap();
        let h = harbor_core_open_ex(root_c.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
        assert!(!h.is_null());
        assert_eq!(
            call(h, "knowledge.open", serde_json::json!({"package_id": pkg}))["ok"],
            true
        );

        for (text, want) in [
            (
                "Please remember that the Delta account manager is Karim",
                "remember_fact",
            ),
            ("Who is the Delta account manager again?", "recall_memory"),
            (
                "Turn the recording of this morning's call into text",
                "transcribe_audio",
            ),
            (
                "Every Friday at five, prepare the overdue payments summary",
                "schedule_goal",
            ),
            ("Read the words off this photo of the receipt", "read_scan"),
        ] {
            let d = call(h, "decision.choose", serde_json::json!({"text": text}));
            assert_eq!(d["ok"], true, "{d}");
            let top = &d["result"]["ranked"][0];
            println!(
                "DECIDE {family}: {text:?} -> {} ({:.2}) abstained={}",
                top["id"],
                top["score"].as_f64().unwrap(),
                d["result"]["abstained"]
            );
            assert_eq!(top["id"], want, "{family}: {text}");
            assert_eq!(d["result"]["recommend_only"], true);
            assert_eq!(d["result"]["task"], "intent");
            assert!(
                d["result"]["calibration"].is_object(),
                "calibrated for {family}"
            );
        }

        // The caller's own options are ranked but never recommended: there is
        // no benchmark for an arbitrary option set, so no bar to clear.
        let custom = call(
            h,
            "decision.choose",
            serde_json::json!({
            "text": "the printer on the second floor is out of toner",
            "options": [
                {"id": "facilities", "description": "Office equipment, supplies and building issues"},
                {"id": "finance", "description": "Invoices, payments and budgets"}]}),
        );
        assert_eq!(custom["ok"], true, "{custom}");
        assert_eq!(custom["result"]["ranked"][0]["id"], "facilities");
        assert_eq!(custom["result"]["abstained"], true);
        assert_eq!(custom["result"]["reason"], "uncalibrated");
        // Malformed option sets are refused.
        assert_eq!(
            call(
                h,
                "decision.choose",
                serde_json::json!({"text": "x", "options": [{"id": "a", "description": "d"}]})
            )["ok"],
            false
        );
        unsafe { harbor_core_close(h) };
    }
    if ran == 0 {
        eprintln!("SKIP: no embedding fixtures");
    }
}
