//! A scanned PDF as the invoice skill's input (decision 0018), end to end
//! through the FFI: the real Apple Vision host reads the scan on-device, a
//! real chat model turns the OCR text into records, and the deterministic
//! tools check, total and propose the workbook. macOS only; skips without
//! the OCR host dylib or the chat fixture.

#![cfg(target_os = "macos")]

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use base64::Engine as _;
use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};
use harbor_inference::ocr::{register_ocr_host, OcrHostVtable};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use libloading::{Library, Symbol};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn install(models_root: &Path, gguf: &Path) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("chat-{}", &sha[..12]);
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
fn a_scanned_invoice_batch_becomes_the_same_verified_report() {
    let r = repo_root();
    let dylib = r.join("native/apple/ocr_host/build/libharbor_ocr_host.dylib");
    let qwen = r.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !dylib.exists() || !qwen.exists() {
        eprintln!("SKIP: OCR host or chat fixture absent");
        return;
    }
    unsafe {
        let lib = Library::new(&dylib).unwrap();
        let d: Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_char> =
            lib.get(b"harbor_ocr_host_descriptor").unwrap();
        let rec: Symbol<
            unsafe extern "C" fn(
                *const std::ffi::c_char,
                *const u8,
                usize,
                *const AtomicBool,
                *mut *mut std::ffi::c_char,
            ) -> std::ffi::c_int,
        > = lib.get(b"harbor_ocr_host_recognize").unwrap();
        let f: Symbol<unsafe extern "C" fn(*mut std::ffi::c_char)> =
            lib.get(b"harbor_ocr_host_free_string").unwrap();
        register_ocr_host(OcrHostVtable {
            descriptor: *d,
            recognize: *rec,
            free_string: *f,
        })
        .unwrap();
        std::mem::forget(lib);
    }
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("models")).unwrap();
    let chat = install(&data.join("models"), &qwen);
    let root = CString::new(data.to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-scan").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null());

    let pdf = std::fs::read(r.join("fixtures/ocr/scanned_invoice_batch.pdf")).unwrap();
    let start = call(
        h,
        "op.start_skill_run",
        serde_json::json!({
        "skill_id": "supplier-invoice-review", "chat_package": chat,
        "inputs": {"artifact_id": "scan", "as_of": "2026-10-09"},
        "artifacts": [{"id": "scan", "name": "scan.pdf",
                       "data_b64": base64::engine::general_purpose::STANDARD.encode(&pdf)}]}),
    );
    assert_eq!(start["ok"], true, "{start}");
    let op = start["result"]["op_id"].as_str().unwrap().to_string();
    let report = loop {
        let st = call(h, "op.status", serde_json::json!({"op_id": op}))["result"].clone();
        match st["state"].as_str() {
            Some("done") => break st["result"].clone(),
            Some("failed") | Some("cancelled") => panic!("op ended: {st}"),
            _ => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    };
    let bb = &report["blackboard"];
    println!(
        "OCR engine={} source={} pages={}",
        bb["doc"]["engine"], bb["doc"]["source"], bb["doc"]["pages"]
    );
    println!(
        "state={} review.ok={} problems={}",
        report["state"], bb["review"]["ok"], bb["review"]["problems"]
    );
    assert_eq!(bb["doc"]["source"], "ocr");
    assert_eq!(bb["doc"]["engine"], "apple-vision");
    assert_eq!(report["state"], "WAITING_APPROVAL", "{report}");
    // The same totals as the typed-text run: the scan changed nothing.
    assert_eq!(bb["review"]["summary"]["total_invoiced_cents"], 1_235_050);
    assert_eq!(bb["review"]["summary"]["total_overdue_cents"], 915_050);
    unsafe { harbor_core_close(h) };
}
