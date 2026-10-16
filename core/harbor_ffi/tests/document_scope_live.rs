//! "Ask about this file" with real weights: `op.start_generate` with a
//! `document` grounds ONLY on the supplied passages (with and without the
//! Knowledge index open), cites them by passage id / locator, abstains on
//! questions the passages cannot answer, and records a document-scoped
//! run step by name.
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
    fn generate(&self, args: serde_json::Value) -> serde_json::Value {
        let op = self.ok("op.start_generate", args)["op_id"]
            .as_str()
            .unwrap()
            .to_string();
        for _ in 0..3000 {
            let st = self.ok("op.status", serde_json::json!({"op_id": op}));
            match st["state"].as_str() {
                Some("done") => return st["result"].clone(),
                Some("failed") | Some("cancelled") => panic!("generate failed: {st}"),
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        panic!("generate timed out");
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

#[test]
fn asking_about_one_file_grounds_on_its_passages_only() {
    let r = repo_root();
    let eg2 = r.join("fixtures/models/embeddinggemma-2-Q8_0.gguf");
    let qwen = r.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !eg2.exists() || !qwen.exists() {
        eprintln!("SKIP: fixtures absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("models")).unwrap();
    let emb = install(&data.join("models"), &eg2, "embed");
    let chat = install(&data.join("models"), &qwen, "chat");

    let root_c = CString::new(data.to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-docscope-live").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let h = Handle(harbor_core_open_ex(
        root_c.as_ptr(),
        ws.as_ptr(),
        0,
        hex.as_ptr(),
    ));
    assert!(!h.0.is_null());

    let doc = serde_json::json!({
        "name": "Acme contract.docx",
        "passages": [
            {"id": "p1", "text": "Acme Supplies agreement between Acme Supplies and Harbor Ltd.", "locator": "¶1"},
            {"id": "p2", "text": "Payment terms are net 45 days from the invoice date.", "locator": "¶2"},
            {"id": "p3", "text": "The agreement renews automatically each January.", "locator": "¶3"},
        ]
    });

    // 1. Knowledge NOT open: the whole (small) file is the evidence.
    let run_id = h.ok("run.create", serde_json::json!({}))["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    let a = h.generate(serde_json::json!({
        "question": "What are the payment terms?",
        "chat_package": chat,
        "max_tokens": 48,
        "document": doc,
        "run_id": run_id,
    }));
    eprintln!("answer (no index): {a}");
    assert_eq!(a["used_citations"], true, "{a}");
    assert!(a["answer"].as_str().unwrap().contains("45"), "{a}");
    assert_eq!(a["scope"]["kind"], "document");
    assert_eq!(a["scope"]["ranking"], "all");
    assert_eq!(a["scope"]["passages_used"], 3);
    let cites = a["citations"].as_array().unwrap();
    assert_eq!(cites.len(), 3);
    assert!(cites.iter().all(|c| c["title"] == "Acme contract.docx"));
    let p2 = cites.iter().find(|c| c["passage_id"] == "p2").unwrap();
    assert_eq!(p2["locator"], "¶2");
    assert_eq!(p2["state"], "Current");
    assert!(p2["score"].as_f64().unwrap() > 0.0);

    // The run trail records the step as document-scoped, by name.
    let trail = h.ok("run.replay", serde_json::json!({"run_id": run_id}));
    let trail_s = trail.to_string();
    assert!(
        trail_s.contains("[document-scoped: Acme contract.docx]"),
        "{trail}"
    );

    // 2. A question the file cannot answer abstains.
    let a = h.generate(serde_json::json!({
        "question": "What is the name of Harbor Ltd's chief executive?",
        "chat_package": chat,
        "max_tokens": 24,
        "document": doc,
    }));
    eprintln!("answer (unsupported): {a}");
    assert_eq!(
        a["answer"].as_str().unwrap().trim(),
        "INSUFFICIENT_EVIDENCE",
        "{a}"
    );

    // 3. Knowledge open, a long file and an indexed source that would
    //    answer differently: only the file's passages are cited, chosen
    //    by embedding rank.
    h.ok("knowledge.open", serde_json::json!({"package_id": emb}));
    h.ok(
        "knowledge.ingest",
        serde_json::json!({"sources": [{
            "id": "other", "title": "Other contract",
            "text": "Payment terms are net 90 days for the Zenith account."
        }]}),
    );
    let filler = "This section restates general definitions, notices and \
                  boilerplate that apply across the agreement without \
                  setting any commercial term. "
        .repeat(5);
    let mut passages: Vec<serde_json::Value> = (0..30)
        .map(|i| serde_json::json!({"id": format!("f{i}"), "text": filler, "locator": format!("¶{i}")}))
        .collect();
    passages[21] = serde_json::json!({
        "id": "pay", "text": "Invoices must be settled within forty-five days of issue.", "locator": "¶21"
    });
    let a = h.generate(serde_json::json!({
        "question": "How long does the customer have to pay an invoice?",
        "chat_package": chat,
        "max_tokens": 48,
        "document": {"name": "Long contract.pdf", "passages": passages},
    }));
    eprintln!("answer (long, index open): {a}");
    assert_eq!(a["scope"]["ranking"], "embedding", "{a}");
    let cites = a["citations"].as_array().unwrap();
    assert_eq!(cites[0]["passage_id"], "pay", "{a}");
    assert!(
        cites.iter().all(|c| c["title"] == "Long contract.pdf"),
        "{a}"
    );
    assert!(
        !a.to_string().contains("Zenith"),
        "index must not be searched: {a}"
    );
}
