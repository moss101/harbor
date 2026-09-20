#![no_main]
//! The single JSON dispatcher (`harbor_core_call`) against arbitrary
//! request bytes: no panic, no abort, always a JSON envelope, and the
//! handle survives (a follow-up `identity.get` still answers). One
//! workspace is opened per process with an injected device root so no
//! platform keystore is touched.

use std::ffi::{CStr, CString};
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;

struct Handle(*mut harbor_ffi::WorkspaceHandle, #[allow(dead_code)] tempfile::TempDir);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

fn handle() -> &'static Handle {
    static H: OnceLock<Handle> = OnceLock::new();
    H.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-fuzz").unwrap();
        let hex =
            CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10")
                .unwrap();
        let h = harbor_ffi::harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
        assert!(!h.is_null());
        Handle(h, dir)
    })
}

/// Methods that could touch the network or spend minutes are dispatched
/// with the same code path but through a stub of their arguments: the
/// fuzzer still exercises the argument parsing of every method, and the
/// egress broker refuses without an explicit session anyway.
const METHODS: &[&str] = &[
    "run.create", "run.state", "run.pause", "blob.put", "blob.get", "formula.qualify",
    "trust.pulse", "identity.get", "models.installed", "model.fit_score",
    "artifact.preview", "skills.list", "tools.list", "op.start_skill_run", "run.decide",
    "run.commit_proposal", "run.snapshot", "eval.run_skill", "eval.run", "run.log_request",
    "diag.record", "diag.list", "catalog.import", "knowledge.sources", "knowledge.search",
    "op.status", "op.cancel", "op.list", "runs.list", "run.replay", "no.such.method",
];

fuzz_target!(|data: &[u8]| {
    let h = handle();
    // Two shapes: raw bytes as the whole request, and a method from the
    // catalog with arbitrary args (so the per-method parsers get depth).
    let raw = String::from_utf8_lossy(data).into_owned();
    let request = if let Some((first, rest)) = data.split_first() {
        let method = METHODS[(*first as usize) % METHODS.len()];
        let args: serde_json::Value = serde_json::from_slice(rest)
            .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(rest).into_owned()));
        serde_json::json!({"method": method, "args": args}).to_string()
    } else {
        raw
    };
    let Ok(c) = CString::new(request.replace('\0', "")) else { return };
    let out = unsafe { harbor_ffi::harbor_core_call(h.0, c.as_ptr()) };
    assert!(!out.is_null());
    let text = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
    unsafe { harbor_ffi::harbor_core_string_free(out) };
    let v: serde_json::Value = serde_json::from_str(&text).expect("envelope is JSON");
    assert!(v.get("ok").is_some());
    // The handle is still alive.
    let ping = CString::new(r#"{"method":"identity.get","args":{}}"#).unwrap();
    let out = unsafe { harbor_ffi::harbor_core_call(h.0, ping.as_ptr()) };
    let text = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
    unsafe { harbor_ffi::harbor_core_string_free(out) };
    assert!(text.contains("\"ok\":true"), "{text}");
});
