//! `models.install_from_path` with extra files: a multimodal package is the
//! weights plus an `mmproj` projector, installed atomically as ONE package.

use std::ffi::{CStr, CString};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};

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
fn weights_plus_projector_install_as_one_package() {
    let weights = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models/stories260K.gguf");
    assert!(weights.exists(), "committed tiny fixture");
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    // Any bytes will do for the projector: only the weights role is
    // GGUF-validated at install time.
    let proj = dir.path().join("mmproj-test.gguf");
    std::fs::write(&proj, b"not a real projector, installed verbatim").unwrap();

    let root = CString::new(data.to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-extra").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null());

    let r = call(
        h,
        "models.install_from_path",
        serde_json::json!({
            "package_id": "mm-test",
            "path": weights.to_string_lossy(),
            "extra_files": [{"path": proj.to_string_lossy(), "role": "mmproj"}],
        }),
    );
    assert!(r["ok"].as_bool().unwrap_or(false), "{r}");

    // Both files landed under ONE package, with their roles.
    let pkg = data.join("models").join("mm-test");
    assert!(pkg.join("stories260K.gguf").exists());
    assert_eq!(
        std::fs::read(pkg.join("mmproj-test.gguf")).unwrap(),
        b"not a real projector, installed verbatim"
    );
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(pkg.join("harbor_manifest.json")).expect("manifest written"),
    )
    .unwrap();
    let roles: Vec<&str> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, vec!["weights", "mmproj"]);

    // A bad extra file (missing) fails the whole install: nothing half-installed.
    let bad = call(
        h,
        "models.install_from_path",
        serde_json::json!({
            "package_id": "mm-bad",
            "path": weights.to_string_lossy(),
            "extra_files": [{"path": "/definitely/not/here.gguf", "role": "mmproj"}],
        }),
    );
    assert!(!bad["ok"].as_bool().unwrap_or(false));
    assert!(!data
        .join("models")
        .join("mm-bad")
        .join("harbor_manifest.json")
        .exists());
    unsafe { harbor_core_close(h) };
}
