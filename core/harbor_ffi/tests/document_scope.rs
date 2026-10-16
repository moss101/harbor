//! Document-scoped grounded generation ("Ask about this file"): the
//! optional `document` argument of `op.start_generate` is validated up
//! front, and the scoped path runs without the Knowledge index open (it
//! still needs a chat model package).

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
    serde_json::from_str(&text).expect("response parses")
}

#[test]
#[cfg(feature = "gguf")]
fn document_scope_is_validated_and_needs_no_knowledge_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-docscope").unwrap();
    let root_hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, root_hex.as_ptr());
    assert!(!h.is_null(), "open failed");

    // Malformed documents are refused synchronously with a clear error.
    for bad in [
        serde_json::json!({"passages": [{"text": "x"}]}),
        serde_json::json!({"name": "a.docx", "passages": [{"text": 7}]}),
        serde_json::json!({"name": "a.docx", "passages": []}),
        serde_json::json!({"name": "a.docx", "passages": {"text": "x"}}),
    ] {
        let r = call(
            h,
            "op.start_generate",
            serde_json::json!({"question": "q?", "chat_package": "none", "document": bad}),
        );
        assert_eq!(r["ok"], false, "{r}");
        assert!(
            r["error"]
                .as_str()
                .unwrap_or("")
                .contains("invalid document"),
            "{r}"
        );
    }

    // A chat model package is still required.
    let doc = serde_json::json!({
        "name": "Q3 report.docx",
        "passages": [{"id": "p1", "text": "Revenue grew 12% in Q3.", "locator": "¶1"}]
    });
    let r = call(
        h,
        "op.start_generate",
        serde_json::json!({"question": "How much did revenue grow?", "document": doc}),
    );
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("chat_package"), "{r}");

    // Valid scope, no Knowledge open: the op starts and gets as far as the
    // chat model (which is not installed here) — never "knowledge not open".
    let r = call(
        h,
        "op.start_generate",
        serde_json::json!({
            "question": "How much did revenue grow?",
            "chat_package": "not-installed-chat",
            "document": doc,
            "max_tokens": 8,
        }),
    );
    assert_eq!(r["ok"], true, "{r}");
    let op = r["result"]["op_id"].as_str().unwrap().to_string();
    let mut last = serde_json::Value::Null;
    for _ in 0..400 {
        last = call(h, "op.status", serde_json::json!({"op_id": op}))["result"].clone();
        if last["state"] != "running" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(last["state"], "failed", "{last}");
    let err = last["result"]["error"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(!err.contains("knowledge"), "{last}");
    assert!(
        err.contains("not-installed-chat") || err.contains("not found"),
        "{last}"
    );

    unsafe { harbor_core_close(h) };
}
