//! With no embedding model installed (a phone with only a chat model),
//! `skills.suggest` still routes: keywords pick the skill a request names,
//! and nothing is picked for a request that names none and has no chat
//! model to ask.
#![cfg(feature = "gguf")]

use std::ffi::{CStr, CString};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};

struct Handle(*mut harbor_ffi::WorkspaceHandle);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) }
    }
}

impl Handle {
    fn open(root: &std::path::Path) -> Self {
        let root = CString::new(root.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-route-lite").unwrap();
        // Injected device root: never the platform Keychain under a runner.
        let hex = CString::new("3c1f6a0d8b7e4952a1d03f76c8e5b2194f60a7d3e81c5b92d4a6f0378e1c59ab")
            .unwrap();
        let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
        assert!(!h.is_null(), "open failed");
        Handle(h)
    }

    fn suggest(&self, text: &str, has_file: bool) -> serde_json::Value {
        let req = CString::new(
            serde_json::json!({"method": "skills.suggest",
                "args": {"text": text, "has_file": has_file}})
            .to_string(),
        )
        .unwrap();
        let raw = unsafe { harbor_core_call(self.0, req.as_ptr()) };
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["ok"], true, "{v}");
        v["result"].clone()
    }
}

#[test]
fn keywords_route_without_any_model() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    let r = h.suggest("write a cv for a systems analyst as a pdf", false);
    assert_eq!(r["router"], "keywords");
    assert_eq!(r["abstained"], false);
    assert_eq!(r["ranked"][0]["skill_id"], "document-drafter");

    let r = h.suggest("turn this report into slides", true);
    assert_eq!(r["ranked"][0]["skill_id"], "report-to-slides");

    // Names no skill, and there is no chat model to ask.
    let r = h.suggest("tell me something nice", false);
    assert_eq!(r["abstained"], true);
    assert_eq!(r["router"], "undecided");
    assert_eq!(r["none_fit"], false);
}
