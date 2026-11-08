//! `convert.text_to_pdf`: an answer drafted in chat (a CV, a letter) saved
//! as a one-column text PDF through the real FFI boundary.

use std::ffi::{CStr, CString};

use base64::Engine as _;
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
fn a_chat_cv_becomes_a_pdf_whose_text_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-text-pdf").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null());

    let cv = "# José Álvarez\nSystems Analyst\n\n## Skills\n- SQL, **UML**, BPMN\n";
    let r = call(h, "convert.text_to_pdf", serde_json::json!({ "text": cv }));
    assert_eq!(r["ok"], true, "{r}");
    let res = &r["result"];
    assert_eq!(
        res["title"], "José Álvarez",
        "title defaults to the first heading"
    );
    assert_eq!(res["pages"], 1);
    assert_eq!(res["unsupported_chars"], 0);
    let pdf = base64::engine::general_purpose::STANDARD
        .decode(res["data_b64"].as_str().unwrap())
        .unwrap();
    let text: String = harbor_render::pdf::extract_pages(&pdf)
        .unwrap()
        .pages
        .iter()
        .map(|p| p.text.clone())
        .collect();
    assert!(
        text.contains("Systems Analyst") && text.contains("SQL, UML, BPMN"),
        "{text}"
    );

    // Arabic cannot be drawn by the base-14 fonts: reported, not hidden.
    let r = call(
        h,
        "convert.text_to_pdf",
        serde_json::json!({ "text": "مرحبا" }),
    );
    assert_eq!(r["result"]["unsupported_chars"], 5, "{r}");

    let r = call(
        h,
        "convert.text_to_pdf",
        serde_json::json!({ "text": "  \n" }),
    );
    assert_eq!(r["ok"], false, "empty text is refused: {r}");
    unsafe { harbor_core_close(h) };
}
