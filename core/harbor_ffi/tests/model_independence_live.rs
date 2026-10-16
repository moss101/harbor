//! Model independence, end to end through the FFI with real weights
//! (decision 0017): EmbeddingGemma 2 is the embedder; two DIFFERENT chat
//! models (Qwen2.5-1.5B-Instruct and the 260K-parameter stories fixture)
//! are selected in turn. Retrieval, memory and routing must not move when
//! the chat model changes, the Word drafting skill must draw on retrieved
//! passages with whichever chat model is chosen, and a model that cannot do
//! the job must stop the run explicitly instead of producing a document.
//!
//! Skips when the fixtures are absent (they are not in CI).

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn install(models_root: &Path, gguf: &Path, prefix: &str) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("{prefix}-{}", &sha[..12]);
    let installer = PackageInstaller::new(models_root);
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: gguf.file_name().unwrap().to_string_lossy().to_string(),
            sha256: sha,
            size_bytes: bytes.len() as u64,
        }],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin(&id).unwrap();
    installer
        .ingest_file(&mut staged, &manifest.files[0], &bytes)
        .unwrap();
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    id
}

struct Handle(*mut harbor_ffi::WorkspaceHandle);

impl Handle {
    fn raw(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let req =
            CString::new(serde_json::json!({"method": method, "args": args}).to_string()).unwrap();
        let raw = unsafe { harbor_core_call(self.0, req.as_ptr()) };
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        serde_json::from_str(&text).unwrap()
    }
    fn ok(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let v = self.raw(method, args);
        assert_eq!(v["ok"], true, "{method}: {v}");
        v["result"].clone()
    }
    fn run_skill(&self, args: serde_json::Value) -> serde_json::Value {
        let start = self.raw("op.start_skill_run", args);
        if start["ok"] != true {
            return serde_json::json!({"start_error": start["error"]});
        }
        let op = start["result"]["op_id"].as_str().unwrap().to_string();
        for _ in 0..4800 {
            let st = self.ok("op.status", serde_json::json!({"op_id": op}));
            match st["state"].as_str() {
                Some("done") => return st["result"].clone(),
                Some("failed") | Some("cancelled") => {
                    return serde_json::json!({"op_error": st});
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        panic!("skill run timed out");
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

#[test]
fn switching_the_chat_model_leaves_retrieval_memory_and_routing_untouched() {
    let r = repo_root();
    let eg2 = r.join("fixtures/models/embeddinggemma-2-Q8_0.gguf");
    let qwen = r.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    let tiny = r.join("fixtures/models/stories260K.gguf");
    let llama = r.join("fixtures/models/Llama-3.2-1B-Instruct-Q4_K_M.gguf");
    if !eg2.exists() || !qwen.exists() || !tiny.exists() {
        eprintln!("SKIP: fixtures absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("models")).unwrap();
    let emb = install(&data.join("models"), &eg2, "embed");
    let chat_a = install(&data.join("models"), &qwen, "chat");
    let chat_b = install(&data.join("models"), &tiny, "chat");
    let chat_c = llama
        .exists()
        .then(|| install(&data.join("models"), &llama, "chat"));

    let root_c = CString::new(data.to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-indep").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = Handle(harbor_core_open_ex(
        root_c.as_ptr(),
        ws.as_ptr(),
        0,
        hex.as_ptr(),
    ));
    assert!(!h.0.is_null());

    h.ok("knowledge.open", serde_json::json!({"package_id": emb}));
    h.ok(
        "knowledge.ingest",
        serde_json::json!({"sources": [{
            "id": "acme-contract", "title": "Acme supply contract",
            "text": "Acme Supplies contract. Payment terms are net 45 days from the invoice date. A late fee of 1.5% per month applies to overdue balances. The contract renews each January."
        }]}),
    );
    let mem = h.ok(
        "memory.add",
        serde_json::json!({"text": "The board prefers one-page summaries with the overdue total first."}),
    );

    let search = |q: &str| -> Vec<String> {
        h.ok(
            "knowledge.search",
            serde_json::json!({"question": q, "top_k": 3}),
        )["citations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["source_id"].as_str().unwrap().to_string())
            .collect()
    };
    let routed =
        |q: &str| -> serde_json::Value { h.ok("skills.suggest", serde_json::json!({"text": q})) };
    let q = "what are the payment terms and late fee for Acme?";
    let req =
        "Review these supplier invoices, identify overdue payments and prepare an Excel report";
    let search_before = search(q);
    let route_before = routed(req);
    assert_eq!(search_before[0], "acme-contract");

    let inputs = serde_json::json!({
        "kind": "report",
        "brief": "Write a short management report on the Acme Supplies relationship for the board: what the payment terms are and what happens when we pay late.",
        "use_sources": true
    });
    let mut chats = vec![("qwen2.5-1.5b", &chat_a), ("stories260K", &chat_b)];
    if let Some(c) = chat_c.as_ref() {
        chats.push(("llama-3.2-1b", c));
    }
    let invoices =
        std::fs::read_to_string(r.join("fixtures/business/supplier_invoices.txt")).unwrap();
    for (label, chat) in chats {
        let report = h.run_skill(serde_json::json!({
            "skill_id": "document-drafter", "inputs": inputs, "chat_package": chat}));
        let gather = &report["blackboard"]["gather"];
        // Retrieval reached the drafting skill with either chat model.
        assert_eq!(gather["available"], true, "{label}: {gather}");
        assert_eq!(
            gather["citations"][0]["source_id"], "acme-contract",
            "{label}"
        );
        if chat != &chat_b {
            // A capable instruct model drafts, grounded in the cited passage.
            assert_eq!(report["state"], "WAITING_APPROVAL", "{label}: {report}");
            let draft = report["blackboard"]["draft"].to_string();
            assert!(
                draft.contains("45") && draft.contains("1.5%"),
                "{label}: {draft}"
            );
            // The same Harbor skill contract on this model: the model reads,
            // the tool decides. Whatever it read, the totals are the tool's.
            let inv = h.run_skill(serde_json::json!({
                "skill_id": "supplier-invoice-review", "chat_package": chat,
                "inputs": {"invoices": invoices, "as_of": "2026-10-09"}}));
            let bb = &inv["blackboard"];
            println!(
                "{label}: invoice review state={} ok={} overdue_cents={}",
                inv["state"], bb["review"]["ok"], bb["review"]["summary"]["total_overdue_cents"]
            );
            if inv["state"] == "WAITING_APPROVAL" {
                assert_eq!(
                    bb["review"]["summary"]["total_overdue_cents"], 915050,
                    "{label}"
                );
            } else {
                // Refused rather than wrong: the problems are named.
                assert_eq!(bb["review"]["ok"], false, "{label}: {inv}");
            }
        } else {
            // A model that cannot do the job stops the run with a reason;
            // it never yields a document, and nothing waits for approval.
            assert_eq!(report["state"], "FAILED", "{label}: {report}");
            let why = report["status"]["error"].as_str().unwrap_or("");
            assert!(why.contains("context"), "{label}: {why}");
            assert!(report["blackboard"].get("proposal").is_none());
        }
        // Whatever the chat model, the embedder and index are unchanged.
        assert_eq!(search(q), search_before, "{label}: retrieval moved");
        assert_eq!(
            routed(req)["suggestions"],
            route_before["suggestions"],
            "{label}: routing moved"
        );
        let listed = h.ok("memory.list", serde_json::json!({}));
        assert!(listed["memories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == mem["id"]));
    }
}
