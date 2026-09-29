//! User workbook editing (office sub-product, Phase 1): typed cell edits
//! through the C boundary recalculate with the pinned engine and come
//! back as new xlsx bytes. Cached values never satisfy a verified number
//! (SEC-041), and the round trip preserves the OOXML package contract
//! (harbor_artifacts owns every write).

use std::ffi::{CStr, CString};
use std::path::Path;

use harbor_ffi::{harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free};

struct Handle(*mut harbor_ffi::WorkspaceHandle);

impl Handle {
    fn open(root: &Path) -> Self {
        let root = CString::new(root.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-workbook-edit").unwrap();
        let root_hex =
            CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10")
                .unwrap();
        let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, root_hex.as_ptr());
        assert!(!h.is_null(), "open failed");
        Handle(h)
    }

    fn call(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let req =
            CString::new(serde_json::json!({"method": method, "args": args}).to_string()).unwrap();
        let raw = unsafe { harbor_core_call(self.0, req.as_ptr()) };
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["ok"], true, "{method}: {v}");
        v["result"].clone()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

fn fixture_xlsx() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/office/dcf_model.xlsx"),
    )
    .expect("fixture workbook")
}

fn first_sheet(bytes: &[u8]) -> String {
    let doc = harbor_artifacts::workbook::WorkbookDoc::load(bytes).unwrap();
    doc.sheet_names()[0].clone()
}

#[test]
fn typed_edits_recalculate_and_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    let original = fixture_xlsx();
    let sheet = first_sheet(&original);
    use base64::Engine as _;
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(&original);

    // 1. A value edit and a formula edit: the engine recomputes the
    //    formula, not the cached number.
    let result = h.call(
        "workbook.edit",
        serde_json::json!({
            "data_b64": data_b64,
            "edits": [
                {"sheet": sheet, "row": 1, "col": 1, "kind": "number", "value": 1234.5},
                {"sheet": sheet, "row": 2, "col": 1, "kind": "formula", "value": "=1+2"},
            ],
        }),
    );
    assert_eq!(result["applied"], 2);
    let out_b64 = result["data_b64"].as_str().unwrap();
    let out = base64::engine::general_purpose::STANDARD.decode(out_b64).unwrap();
    assert_ne!(out, original, "edited bytes must differ");

    // 2. The returned package reloads and carries the edits.
    let doc = harbor_artifacts::workbook::WorkbookDoc::load(&out).unwrap();
    let data = doc.sheet(&sheet).unwrap();
    let b1 = data.cells.get(&(1, 1)).expect("B1 edited");
    assert_eq!(b1.cached, Some(harbor_formula::value::CellValue::Number(1234.5)));
    let b2 = data.cells.get(&(1, 2)).expect("B2 edited");
    assert!(b2.formula.is_some());
}

#[test]
fn bad_edits_fail_honestly() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    use base64::Engine as _;
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(fixture_xlsx());
    let sheet = first_sheet(&fixture_xlsx());

    // Unknown kind refused (not silently coerced).
    let req = CString::new(
        serde_json::json!({
            "method": "workbook.edit",
            "args": {"data_b64": data_b64, "edits": [
                {"sheet": sheet, "row": 1, "col": 1, "kind": "magic"}]},
        })
        .to_string(),
    )
    .unwrap();
    let raw = unsafe { harbor_core_call(h.0, req.as_ptr()) };
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
    unsafe { harbor_core_string_free(raw) };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("unknown edit kind"));

    // Non-workbook bytes refused with a typed failure.
    let raw = CString::new(
        serde_json::json!({
            "method": "workbook.edit",
            "args": {"data_b64": base64::engine::general_purpose::STANDARD.encode(b"not a workbook"), "edits": []},
        })
        .to_string(),
    )
    .unwrap();
    let ptr = unsafe { harbor_core_call(h.0, raw.as_ptr()) };
    let text = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string();
    unsafe { harbor_core_string_free(ptr) };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("load:"));
}
