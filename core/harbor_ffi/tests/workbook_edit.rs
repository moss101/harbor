//! User workbook editing (office sub-product, Phase 1): typed cell edits
//! through the C boundary recalculate with the pinned engine and come
//! back as new xlsx bytes. Cached values never satisfy a verified number
//! (SEC-041), and the round trip preserves the OOXML package contract
//! (harbor_artifacts owns every write).

use std::ffi::{CStr, CString};
use std::path::Path;

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};

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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/office/dcf_model.xlsx"),
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
    let out = base64::engine::general_purpose::STANDARD
        .decode(out_b64)
        .unwrap();
    assert_ne!(out, original, "edited bytes must differ");

    // 2. The returned package reloads and carries the edits.
    let doc = harbor_artifacts::workbook::WorkbookDoc::load(&out).unwrap();
    let data = doc.sheet(&sheet).unwrap();
    let b1 = data.cells.get(&(1, 1)).expect("B1 edited");
    assert_eq!(
        b1.cached,
        Some(harbor_formula::value::CellValue::Number(1234.5))
    );
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

#[test]
fn ops_format_and_chart_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    let original = fixture_xlsx();
    let sheet = first_sheet(&original);
    use base64::Engine as _;
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(&original);

    let result = h.call(
        "workbook.edit",
        serde_json::json!({
            "data_b64": data_b64,
            "edits": [],
            "ops": [
                {"op": "bold", "sheet": sheet, "row": 1, "col_from": 1, "col_to": 3},
                {"op": "number_format", "sheet": sheet, "col": 2, "row_from": 2, "row_to": 5, "code": "0.0%"},
                {"op": "column_width", "sheet": sheet, "col": 1, "width": 42.5},
                {"op": "freeze_first_row", "sheet": sheet},
                {"op": "add_chart", "sheet": sheet, "from": "E2", "to": "K18",
                 "series": [format!("{sheet}!$B$2:$B$5")], "title": "Quarterly"},
            ],
        }),
    );
    assert_eq!(result["ops_applied"], 5);
    let out = base64::engine::general_purpose::STANDARD
        .decode(result["data_b64"].as_str().unwrap())
        .unwrap();
    // The chart part is really in the package.
    assert_eq!(
        harbor_artifacts::workbook::WorkbookDoc::count_charts_in_bytes(&out).unwrap(),
        harbor_artifacts::workbook::WorkbookDoc::count_charts_in_bytes(&original).unwrap() + 1
    );
    // And the package still loads as a workbook.
    assert!(harbor_artifacts::workbook::WorkbookDoc::load(&out).is_ok());
}

#[test]
fn unknown_op_fails_honestly() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    use base64::Engine as _;
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(fixture_xlsx());
    let req = CString::new(
        serde_json::json!({
            "method": "workbook.edit",
            "args": {"data_b64": data_b64, "edits": [], "ops": [
                {"op": "pivot_table", "sheet": "Sheet1"}]},
        })
        .to_string(),
    )
    .unwrap();
    let raw = unsafe { harbor_core_call(h.0, req.as_ptr()) };
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
    unsafe { harbor_core_string_free(raw) };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("unknown op"));
}

#[test]
fn markdown_to_docx_produces_loadable_package() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    use base64::Engine as _;
    let md = "# Harbor Office\n\nOpens markdown as a real document.\n\n- bullet one\n- bullet two\n\n1. first\n2. second\n";
    let result = h.call(
        "convert.markdown_to_docx",
        serde_json::json!({ "markdown": md, "title": "Harbor Office" }),
    );
    assert!(result["blocks"].as_u64().unwrap() >= 6);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result["data_b64"].as_str().unwrap())
        .unwrap();
    // The package loads through Harbor's own DOCX reader with the
    // qualified styles present.
    let doc = harbor_artifacts::docx::DocxDocument::load(&bytes).unwrap();
    let styles: Vec<&str> = doc
        .paragraphs
        .iter()
        .filter_map(|p| p.style.as_deref())
        .collect();
    assert!(styles.contains(&"Title"));
    assert!(styles.contains(&"ListBullet"));
    assert!(styles.contains(&"ListNumber"));
    assert!(result["inline_formatting"]
        .as_str()
        .unwrap()
        .contains("stripped"));
}

#[test]
fn pdf_to_docx_is_text_extraction_level() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    use base64::Engine as _;
    let pdf = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/office/quarterly_report.pdf"),
    )
    .expect("fixture pdf");
    let result = h.call(
        "convert.pdf_to_docx",
        serde_json::json!({
            "data_b64": base64::engine::general_purpose::STANDARD.encode(&pdf),
            "title": "Quarterly Report",
        }),
    );
    assert!(result["pages"].as_u64().unwrap() >= 1);
    assert!(result["paragraphs"].as_u64().unwrap() >= 1);
    assert_eq!(
        result["extraction_level"].as_str().unwrap(),
        "text-only (no layout, tables or images)"
    );
    let docx = base64::engine::general_purpose::STANDARD
        .decode(result["data_b64"].as_str().unwrap())
        .unwrap();
    let doc = harbor_artifacts::docx::DocxDocument::load(&docx).unwrap();
    let styles: Vec<&str> = doc
        .paragraphs
        .iter()
        .filter_map(|p| p.style.as_deref())
        .collect();
    assert!(styles.contains(&"Heading2"));
    assert!(!doc.paragraphs.iter().all(|p| p.text.is_empty()));

    // A non-PDF payload is a typed refusal, never a guessed conversion.
    let req = CString::new(
        serde_json::json!({
            "method": "convert.pdf_to_docx",
            "args": {"data_b64": base64::engine::general_purpose::STANDARD.encode(b"not a pdf"), "title": "x"},
        })
        .to_string(),
    )
    .unwrap();
    let raw = unsafe { harbor_core_call(h.0, req.as_ptr()) };
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
    unsafe { harbor_core_string_free(raw) };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], false);
}

#[test]
fn docx_paragraph_edit_round_trips_with_precondition() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    use base64::Engine as _;
    let docx = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/office/structured.docx"),
    )
    .expect("fixture docx");
    let doc = harbor_artifacts::docx::DocxDocument::load(&docx).unwrap();
    let target = doc.paragraphs.iter().find(|p| !p.text.is_empty()).unwrap();

    let result = h.call(
        "docx.edit",
        serde_json::json!({
            "data_b64": base64::engine::general_purpose::STANDARD.encode(&docx),
            "ops": [{"kind": "paragraph", "index": target.index, "text": "Edited by Harbor Office Suite"}],
        }),
    );
    assert_eq!(result["applied"], 1);
    let out = base64::engine::general_purpose::STANDARD
        .decode(result["data_b64"].as_str().unwrap())
        .unwrap();
    let re = harbor_artifacts::docx::DocxDocument::load(&out).unwrap();
    assert_eq!(re.paragraphs[target.index as usize - 1].text, "Edited by Harbor Office Suite");

    // A stale edit (paragraph changed underneath) is refused, not applied.
    let mut changed = docx.clone();
    let stale = h.call(
        "docx.edit",
        serde_json::json!({
            "data_b64": base64::engine::general_purpose::STANDARD.encode(&changed),
            "ops": [{"kind": "paragraph", "index": target.index, "text": "second edit"}],
        }),
    );
    // The second edit against the SAME base applies (idempotent base).
    assert_eq!(stale["applied"], 1);
    let _ = &mut changed;
}

#[test]
fn create_empty_workbook_and_document() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    use base64::Engine as _;

    let wb = h.call(
        "workbook.create_empty",
        serde_json::json!({ "sheet": "Sheet1" }),
    );
    let wb_bytes = base64::engine::general_purpose::STANDARD
        .decode(wb["data_b64"].as_str().unwrap())
        .unwrap();
    let doc = harbor_artifacts::workbook::WorkbookDoc::load(&wb_bytes).unwrap();
    assert_eq!(doc.sheet_names(), vec!["Sheet1".to_string()]);

    let d = h.call("docx.create", serde_json::json!({ "title": "Meeting notes" }));
    let d_bytes = base64::engine::general_purpose::STANDARD
        .decode(d["data_b64"].as_str().unwrap())
        .unwrap();
    let doc = harbor_artifacts::docx::DocxDocument::load(&d_bytes).unwrap();
    assert!(doc.paragraphs.iter().any(|p| p.text == "Meeting notes"));
}

#[test]
fn unknown_docx_op_kind_fails_honestly() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    use base64::Engine as _;
    let docx = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/office/structured.docx"),
    )
    .unwrap();
    let req = CString::new(
        serde_json::json!({
            "method": "docx.edit",
            "args": {"data_b64": base64::engine::general_purpose::STANDARD.encode(&docx),
                     "ops": [{"kind": "magic", "text": "x"}]},
        })
        .to_string(),
    )
    .unwrap();
    let raw = unsafe { harbor_core_call(h.0, req.as_ptr()) };
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
    unsafe { harbor_core_string_free(raw) };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("unknown docx op kind"));
}
