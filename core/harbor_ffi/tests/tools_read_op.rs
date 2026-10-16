//! `op.start_tools_read`: OCR / speech reads run as cancellable ops, and the
//! same validation as the synchronous `tools.read` applies.
use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};
use std::ffi::{CStr, CString};

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
fn a_read_runs_as_an_op_and_only_read_tools_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-ops").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null());

    // No artifact: ocr.extract hands back the typed fallback, no engine needed.
    let started = call(
        h,
        "op.start_tools_read",
        serde_json::json!({"tool": "ocr.extract", "args": {"fallback": "Invoice 1001 4,200"}}),
    );
    assert_eq!(started["ok"], true, "{started}");
    let op = started["result"]["op_id"].as_str().unwrap().to_string();
    let mut last = serde_json::Value::Null;
    for _ in 0..200 {
        let st = call(h, "op.status", serde_json::json!({"op_id": op}));
        last = st["result"].clone();
        if last["state"] == "done" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(last["state"], "done", "{last}");
    assert_eq!(last["result"]["text"], "Invoice 1001 4,200");

    // Anything that is not a Read-class OCR/speech tool is refused up front,
    // and cancelling an op is accepted.
    for tool in ["fs.read_workspace_file", "no.such.tool", ""] {
        let r = call(h, "op.start_tools_read", serde_json::json!({"tool": tool}));
        assert_eq!(r["ok"], false, "{tool}: {r}");
    }
    let c = call(h, "op.cancel", serde_json::json!({"op_id": op}));
    assert_eq!(c["ok"], true, "{c}");
    unsafe { harbor_core_close(h) };
}
