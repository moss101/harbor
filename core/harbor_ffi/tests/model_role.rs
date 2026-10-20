//! `models.installed` reports each package's role (read from the GGUF
//! header) and the core refuses an embedder as the chat model up front.

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

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models")
        .join(name)
}

#[test]
#[cfg(feature = "gguf")]
fn installed_reports_role_and_embedder_is_refused_as_chat() {
    let chat = fixture("stories260K.gguf");
    assert!(chat.exists(), "committed tiny fixture");
    let embed = fixture("bge-small-en-v1.5-q8_0.gguf");
    let dir = tempfile::tempdir().unwrap();
    let root = CString::new(dir.path().to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-role").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
    assert!(!h.is_null());

    let r = call(
        h,
        "models.install_from_path",
        serde_json::json!({"package_id": "tiny-chat", "path": chat.to_string_lossy()}),
    );
    assert_eq!(r["ok"], true, "{r}");
    let have_embedder = embed.exists();
    if have_embedder {
        let r = call(
            h,
            "models.install_from_path",
            serde_json::json!({"package_id": "bge-small", "path": embed.to_string_lossy()}),
        );
        assert_eq!(r["ok"], true, "{r}");
    }

    let r = call(h, "models.installed", serde_json::json!({}));
    let models = r["result"]["models"].as_array().unwrap().clone();
    let role_of = |id: &str| {
        models
            .iter()
            .find(|m| m["id"] == id)
            .map(|m| m["role"].as_str().unwrap().to_string())
    };
    assert_eq!(role_of("tiny-chat").as_deref(), Some("chat"), "{r}");
    if have_embedder {
        assert_eq!(role_of("bge-small").as_deref(), Some("embedder"), "{r}");
        let g = call(
            h,
            "op.start_generate",
            serde_json::json!({"question": "q?", "chat_package": "bge-small"}),
        );
        assert_eq!(g["ok"], false, "{g}");
        assert!(
            g["error"]
                .as_str()
                .unwrap_or("")
                .contains("bge-small is an embedding model, not a chat model"),
            "{g}"
        );
    }
    // A chat-role package is NOT refused (the op starts).
    let g = call(
        h,
        "op.start_generate",
        serde_json::json!({"question": "q?", "chat_package": "tiny-chat", "max_tokens": 4}),
    );
    assert_eq!(g["ok"], true, "{g}");
    // Let the op finish before the temp workspace goes away.
    let op = g["result"]["op_id"].as_str().unwrap().to_string();
    for _ in 0..1200 {
        let st = call(h, "op.status", serde_json::json!({"op_id": op}));
        if st["result"]["state"] != "running" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    unsafe { harbor_core_close(h) };
}
