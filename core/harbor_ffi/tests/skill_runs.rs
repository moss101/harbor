//! Skill graph runs through the real FFI boundary: JSON in / JSON out via
//! `harbor_core_call`, background op polling, approval decision and the
//! durable snapshot. No model: placeholder-fill is fully deterministic.

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
        let ws = CString::new("ws-skill").unwrap();
        // Injected device root (the Android/CI path): never the platform
        // Keychain, which prompts under a test runner.
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
        assert!(!raw.is_null());
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        serde_json::from_str(&text).unwrap()
    }

    fn call(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let v = self.raw(method, args);
        assert_eq!(v["ok"], true, "{method}: {v}");
        v["result"].clone()
    }

    fn call_err(&self, method: &str, args: serde_json::Value) -> String {
        let v = self.raw(method, args);
        assert_eq!(v["ok"], false, "{method} unexpectedly succeeded: {v}");
        v["error"].as_str().unwrap_or_default().to_string()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

fn wait_op(h: &Handle, op_id: &str) -> serde_json::Value {
    for _ in 0..600 {
        let st = h.call("op.status", serde_json::json!({"op_id": op_id}));
        match st["state"].as_str() {
            Some("done") => return st["result"].clone(),
            Some("failed") | Some("cancelled") => panic!("op ended: {st}"),
            _ => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    panic!("op timed out");
}

#[test]
fn placeholder_fill_runs_through_the_ffi_to_approval_and_completion() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());

    // skills.list exposes graph metadata and runnability honestly.
    let skills = h.call("skills.list", serde_json::json!({}));
    let list = skills["skills"].as_array().unwrap();
    let fill = list.iter().find(|s| s["id"] == "placeholder-fill").unwrap();
    assert_eq!(fill["runnable"], true);
    assert_eq!(fill["schema"], "harbor.skill/v2");
    assert_eq!(fill["graph"]["model_nodes"], 0);
    assert!(fill["graph"]["node_count"].as_u64().unwrap() >= 6);
    let prose = list.iter().find(|s| s["id"] == "doc-intelligence").unwrap();
    assert_eq!(prose["runnable"], false);
    assert!(prose["graph"].is_null());
    let tools = h.call("tools.list", serde_json::json!({}));
    assert!(tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == "artifact.fill_placeholders" && t["risk"] == "propose"));

    // Start the run with the template attached as bytes.
    use base64::Engine as _;
    let tpl = std::fs::read(repo_root().join("fixtures/office/letter_template.docx")).unwrap();
    let start = h.call(
        "op.start_skill_run",
        serde_json::json!({
            "skill_id": "placeholder-fill",
            "inputs": {"artifact_id": "tpl", "values": {"name": "Amina", "ref": "HB-42", "AMOUNT": "1,250.00", "sender": "Harbor Team"}},
            "artifacts": [{"id": "tpl", "name": "letter_template.docx", "data_b64": base64::engine::general_purpose::STANDARD.encode(&tpl)}]
        }),
    );
    let op_id = start["op_id"].as_str().unwrap().to_string();
    let report = wait_op(&h, &op_id);
    assert_eq!(report["state"], "WAITING_APPROVAL", "{report}");
    let run_id = report["run_id"].as_str().unwrap().to_string();
    let approval = &report["status"]["approval"];
    assert_eq!(approval["effect_class"], "artifact.commit");
    assert_eq!(approval["batch"]["operations"].as_array().unwrap().len(), 3);

    // The durable run is visible through the existing run surface.
    let state = h.call("run.state", serde_json::json!({"run_id": run_id}));
    assert_eq!(state["state"], "WAITING_APPROVAL", "{state}");
    let snap = h.call("run.snapshot", serde_json::json!({"run_id": run_id}));
    assert_eq!(snap["skill_id"], "placeholder-fill");
    assert_eq!(
        snap["pending_approval"]["proposed_output_hash"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(snap["trail"].as_array().unwrap().len(), 4);

    // Decide → completes; replay verifies the chain and names nodes.
    let decided = h.call(
        "run.decide",
        serde_json::json!({"run_id": run_id, "approved": true}),
    );
    assert_eq!(decided["state"], "COMPLETED", "{decided}");
    assert_eq!(
        decided["status"]["outputs"]["approvals.approve"]["approved"],
        true
    );
    let replay = h.call("run.replay", serde_json::json!({"run_id": run_id}));
    assert_eq!(replay["final_state"], "COMPLETED", "{replay}");
    let summaries: Vec<String> = replay["trail"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["summary"].as_str().map(str::to_string))
        .collect();
    assert!(
        summaries
            .iter()
            .any(|s| s.contains("node fill: artifact.fill_placeholders")),
        "{summaries:?}"
    );
    // Deciding twice is refused: the run is no longer waiting.
    let err = h.call_err(
        "run.decide",
        serde_json::json!({"run_id": run_id, "approved": true}),
    );
    assert!(err.contains("expected WAITING_APPROVAL"), "{err}");

    // The replay-tier eval suite runs through the boundary too.
    let evals = h.call(
        "eval.run_skill",
        serde_json::json!({"skill_id": "placeholder-fill", "evals_root": repo_root().to_string_lossy()}),
    );
    assert_eq!(evals["failed"], 0, "{evals}");
    assert!(evals["passed"].as_u64().unwrap() >= 3);
}

#[test]
fn model_skills_require_a_chat_package_and_prose_skills_cannot_run() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    let err = h.call_err(
        "op.start_skill_run",
        serde_json::json!({"skill_id": "meeting-notes", "inputs": {"transcript": "x"}}),
    );
    assert!(err.contains("chat_package"), "{err}");
    let err = h.call_err(
        "op.start_skill_run",
        serde_json::json!({"skill_id": "doc-intelligence", "inputs": {}}),
    );
    assert!(err.contains("no graph"), "{err}");
}
