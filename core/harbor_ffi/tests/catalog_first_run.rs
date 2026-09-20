//! First-run recommendations (production plan C2): the app imports the
//! bundled signed catalog with the pinned root key, lists it offline with
//! installed state, and estimates Fit for a package it has not installed.

use std::ffi::{CStr, CString};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

struct Handle(*mut harbor_ffi::WorkspaceHandle);

impl Handle {
    fn open(root: &std::path::Path) -> Self {
        let root = CString::new(root.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-catalog").unwrap();
        let root_hex =
            CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10")
                .unwrap();
        let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, root_hex.as_ptr());
        assert!(!h.is_null(), "open failed");
        Handle(h)
    }

    fn raw(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let req =
            CString::new(serde_json::json!({"method": method, "args": args}).to_string()).unwrap();
        let raw = unsafe { harbor_core_call(self.0, req.as_ptr()) };
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        serde_json::from_str(&text).unwrap()
    }

    fn call(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let v = self.raw(method, args);
        assert_eq!(v["ok"], true, "{method}: {v}");
        v["result"].clone()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

#[test]
fn bundled_catalog_imports_lists_offline_and_estimates_fit() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());

    // Nothing imported yet: honest empty answer, not an error.
    let none = h.call("catalog.list", serde_json::json!({}));
    assert_eq!(none["imported"], false);
    assert_eq!(none["packages"].as_array().unwrap().len(), 0);

    let catalog: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("fixtures/catalog/signed_catalog.json")).unwrap(),
    )
    .unwrap();
    let root_hex = std::fs::read_to_string(repo_root().join("fixtures/catalog/root_public.hex"))
        .unwrap()
        .trim()
        .to_string();

    // A first import without the pinned root key is a typed refusal.
    let mut args = catalog.clone();
    let refused = h.raw("catalog.import", args.clone());
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("root_public_hex"),
        "{refused}"
    );
    // A malformed root key is a typed refusal too (no panic).
    args["root_public_hex"] = serde_json::json!("zz");
    let bad = h.raw("catalog.import", args.clone());
    assert_eq!(bad["ok"], false);
    assert!(bad["error"].as_str().unwrap().contains("root key"), "{bad}");

    args["root_public_hex"] = serde_json::json!(root_hex);
    let accepted = h.call("catalog.import", args);
    assert_eq!(accepted["accepted_epoch"], 1);

    let list = h.call("catalog.list", serde_json::json!({}));
    assert_eq!(list["imported"], true);
    assert_eq!(list["epoch"], 1);
    let packages = list["packages"].as_array().unwrap();
    assert!(packages.len() >= 3, "{list}");
    let qwen = packages
        .iter()
        .find(|p| p["id"] == "qwen2.5-1.5b-instruct")
        .expect("qwen in catalog");
    assert_eq!(qwen["quantization"], "Q4_K_M");
    assert_eq!(qwen["context_tokens"], 4096);
    assert_eq!(qwen["tiers"][0], "Balanced");
    assert_eq!(qwen["installed"], false);
    assert_eq!(qwen["files"][0]["role"], "weights");
    assert_eq!(qwen["files"][0]["sha256"].as_str().unwrap().len(), 64);

    // Fit for the not-yet-installed package on a small phone vs a laptop.
    let phone = h.call(
        "model.fit_estimate",
        serde_json::json!({
            "weights_bytes": 1_117_320_736u64,
            "context_tokens": 4096,
            "quantization": "Q4_K_M",
            "physical_ram": 3u64 << 30,
            "available_ram": 1u64 << 30,
            "gpu_backend": true,
            "accelerated": true
        }),
    );
    let laptop = h.call(
        "model.fit_estimate",
        serde_json::json!({
            "weights_bytes": 1_117_320_736u64,
            "context_tokens": 4096,
            "quantization": "Q4_K_M",
            "physical_ram": 16u64 << 30,
            "available_ram": 8u64 << 30,
            "gpu_backend": true,
            "accelerated": true
        }),
    );
    assert!(phone["band"].is_string() && laptop["band"].is_string());
    assert_ne!(
        phone["band"], laptop["band"],
        "phone {phone} vs laptop {laptop}"
    );
    assert!(laptop["estimated_peak_bytes"].as_u64().unwrap() > 1_117_320_736);
    let err = h.raw("model.fit_estimate", serde_json::json!({}));
    assert_eq!(err["ok"], false);
}
