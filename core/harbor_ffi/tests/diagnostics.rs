//! Diagnostics through the FFI boundary (production plan C1): boundary
//! errors are recorded automatically, the app records its own errors,
//! listing is redacted, and the export is a zip with no document content.

use std::ffi::{CStr, CString};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};

struct Handle(*mut harbor_ffi::WorkspaceHandle);

impl Handle {
    fn open(root: &std::path::Path) -> Self {
        let root = CString::new(root.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-diag").unwrap();
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
fn boundary_errors_app_records_and_export_go_through_the_diagnostics_log() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());

    // A failing call is recorded with its method as context.
    let err = h.raw(
        "run.decide",
        serde_json::json!({"run_id": "nope", "approved": true}),
    );
    assert_eq!(err["ok"], false);
    // The app records its own errors; paths are redacted by the log.
    h.call(
        "diag.record",
        serde_json::json!({
            "level": "error",
            "source": "app",
            "message": "RenderFlex overflowed while showing /Users/amina/Documents/Q3-plan.docx",
            "context": "package:harbor_app/surfaces/work_surface.dart",
            "stack": "#0 main (package:harbor_app/main.dart:10)"
        }),
    );
    let bad = h.raw(
        "diag.record",
        serde_json::json!({"level": "loud", "message": "x"}),
    );
    assert_eq!(bad["ok"], false);

    let list = h.call("diag.list", serde_json::json!({"limit": 10}));
    let records = list["records"].as_array().unwrap();
    assert!(records.len() >= 3, "{list}"); // run.decide error, app record, diag.record error
    let app = records
        .iter()
        .find(|r| r["source"] == "app")
        .expect("app record");
    assert!(
        app["message"].as_str().unwrap().contains("<path:.docx>"),
        "{app}"
    );
    assert!(!app["message"].as_str().unwrap().contains("amina"));
    assert_eq!(
        app["context"],
        "package:harbor_app/surfaces/work_surface.dart"
    );
    let ffi = records
        .iter()
        .find(|r| r["source"] == "ffi" && r["context"] == "run.decide")
        .expect("boundary error recorded");
    assert!(ffi["message"].as_str().unwrap().contains("nope"));
    assert!(list["redaction_policy"].as_array().unwrap().len() >= 3);

    // The log on disk is sealed: none of the messages appear in plaintext.
    let raw = std::fs::read(dir.path().join("diagnostics").join("log.hdiag")).unwrap();
    assert!(!raw.windows(10).any(|w| w == b"RenderFlex"));

    // Export: a zip at a user-chosen path with build facts + records.
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("harbor-diagnostics.zip");
    let report = h.call(
        "diag.export",
        serde_json::json!({"destination": dest.to_string_lossy(), "app_version": "1.1.0+2"}),
    );
    assert!(dest.exists());
    assert!(report["records"].as_u64().unwrap() >= 3, "{report}");
    assert_eq!(report["sha256"].as_str().unwrap().len(), 64);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
    let mut manifest = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("diagnostics.json").unwrap(), &mut manifest)
        .unwrap();
    let m: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(m["schema"], "harbor.diagnostics/v1");
    assert_eq!(m["facts"]["app_version"], "1.1.0+2");
    assert_eq!(m["facts"]["privacy_mode"], "LOCAL_ONLY");
    assert_eq!(m["facts"]["device_hash"].as_str().unwrap().len(), 16);
    assert!(m["never_contains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "document content"));
    // Never overwrites.
    let again = h.raw(
        "diag.export",
        serde_json::json!({"destination": dest.to_string_lossy()}),
    );
    assert_eq!(again["ok"], false);
    assert!(again["error"].as_str().unwrap().contains("already exists"));
}
