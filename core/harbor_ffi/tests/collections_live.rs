//! Library collections with real weights: a collection-restricted
//! `knowledge.search` / `op.start_generate` never returns or cites another
//! collection's chunk, an unknown collection grounds on nothing, labels are
//! matched case-insensitively and validated, and the grouping survives a
//! restart.
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
fn collection_scope_never_leaks_other_collections_and_persists() {
    let r = repo_root();
    let bge = r.join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    let qwen = r.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !bge.exists() || !qwen.exists() {
        eprintln!("SKIP: fixtures absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("models")).unwrap();
    let emb = install(&data.join("models"), &bge, "embed");
    let chat = install(&data.join("models"), &qwen, "chat");
    let root_c = CString::new(data.to_string_lossy().to_string()).unwrap();
    let ws = CString::new("ws-collections-live").unwrap();
    let hex =
        CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10").unwrap();
    let open = || {
        let h = Handle(harbor_core_open_ex(
            root_c.as_ptr(),
            ws.as_ptr(),
            0,
            hex.as_ptr(),
        ));
        assert!(!h.0.is_null());
        h.ok("knowledge.open", serde_json::json!({"package_id": emb}));
        h
    };
    let h = open();
    h.ok(
        "knowledge.ingest",
        serde_json::json!({"sources": [
            {"id": "contract", "title": "Contract",
             "text": "The contract value is 5000 USD and ends 2026-12-31."},
            {"id": "travel", "title": "Travel",
             "text": "Employees may claim up to 180 USD per night for hotels."},
            {"id": "leave", "title": "Leave",
             "text": "Annual leave entitlement is 24 days per year."},
            {"id": "vault", "title": "Vault",
             "text": "The vault access code is 7391 and is changed every Monday."},
        ]}),
    );
    // Nothing is grouped yet.
    assert_eq!(
        h.ok("knowledge.collections", serde_json::json!({})),
        serde_json::json!([])
    );
    for (sid, c) in [
        ("contract", "Finance"),
        ("travel", " finance "),
        ("leave", "HR"),
        ("vault", "hr"),
    ] {
        h.ok(
            "knowledge.set_collection",
            serde_json::json!({"source_id": sid, "collection": c}),
        );
    }
    // Case-insensitive uniqueness: the first spelling wins.
    let cols = h.ok("knowledge.collections", serde_json::json!({}));
    assert_eq!(
        cols,
        serde_json::json!([
            {"name": "Finance", "source_count": 2},
            {"name": "HR", "source_count": 2},
        ])
    );

    let ids = |r: &serde_json::Value| -> Vec<String> {
        r["citations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["source_id"].as_str().unwrap().to_string())
            .collect()
    };
    let q = "What is the vault access code?";
    // Unrestricted: the vault is the best hit.
    let all = h.ok(
        "knowledge.search",
        serde_json::json!({"question": q, "top_k": 4}),
    );
    assert_eq!(ids(&all)[0], "vault");
    // Restricted to Finance: the vault chunk can never appear.
    let fin = h.ok(
        "knowledge.search",
        serde_json::json!({"question": q, "top_k": 4, "collection": "FINANCE"}),
    );
    let fin_ids = ids(&fin);
    assert!(!fin_ids.is_empty());
    assert!(
        fin_ids.iter().all(|s| s == "contract" || s == "travel"),
        "{fin_ids:?}"
    );
    // Restricted to HR: the vault is found, Finance never.
    let hr = h.ok(
        "knowledge.search",
        serde_json::json!({"question": q, "top_k": 4, "collection": "hr"}),
    );
    assert_eq!(ids(&hr)[0], "vault");
    assert!(ids(&hr).iter().all(|s| s == "leave" || s == "vault"));
    // Unknown collection: empty, never a fallback to everything.
    let none = h.ok(
        "knowledge.search",
        serde_json::json!({"question": q, "top_k": 4, "collection": "Nope"}),
    );
    assert!(ids(&none).is_empty(), "{none}");
    // A non-string scope is refused, not widened.
    let bad = h.raw(
        "knowledge.search",
        serde_json::json!({"question": q, "collection": 7}),
    );
    assert_eq!(bad["ok"], false);

    // Sources listing carries the collection.
    let list = h.ok("knowledge.sources", serde_json::json!({}));
    let vault = list["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source_id"] == "vault")
        .unwrap();
    assert_eq!(vault["collection"], "HR");

    // Validation.
    for bad in [
        serde_json::json!({"source_id": "vault", "collection": "   "}),
        serde_json::json!({"source_id": "vault", "collection": "x".repeat(65)}),
        serde_json::json!({"source_id": "missing", "collection": "HR"}),
    ] {
        assert_eq!(h.raw("knowledge.set_collection", bad)["ok"], false);
    }

    // Answers: a Finance-scoped question about the vault code cannot be
    // grounded on the vault and says so; the scope is reported.
    let a = h.generate(serde_json::json!({
        "question": q, "chat_package": chat, "max_tokens": 32, "collection": "Finance",
    }));
    eprintln!("answer (Finance): {a}");
    assert_eq!(a["scope"]["kind"], "collection");
    assert_eq!(a["scope"]["name"], "Finance");
    assert!(a["citations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["source_id"] != "vault" && c["source_id"] != "leave"));
    assert!(!a["answer"].as_str().unwrap().contains("7391"), "{a}");
    // Unknown collection: no citations, no grounded answer.
    let a = h.generate(serde_json::json!({
        "question": q, "chat_package": chat, "max_tokens": 32, "collection": "Nope",
    }));
    assert_eq!(a["citations"], serde_json::json!([]), "{a}");
    assert_eq!(a["used_citations"], false, "{a}");
    assert!(!a["answer"].as_str().unwrap().contains("7391"), "{a}");
    // HR scope answers from the vault.
    let a = h.generate(serde_json::json!({
        "question": q, "chat_package": chat, "max_tokens": 32, "collection": "HR",
    }));
    eprintln!("answer (HR): {a}");
    assert!(a["answer"].as_str().unwrap().contains("7391"), "{a}");

    // Restart: the grouping is durable.
    drop(h);
    let h = open();
    assert_eq!(
        h.ok("knowledge.collections", serde_json::json!({})),
        cols,
        "collections survive a restart"
    );
    let fin = h.ok(
        "knowledge.search",
        serde_json::json!({"question": q, "top_k": 4, "collection": "finance"}),
    );
    assert!(ids(&fin).iter().all(|s| s == "contract" || s == "travel"));

    // Re-ingesting a source keeps its collection.
    h.ok(
        "knowledge.ingest",
        serde_json::json!({"sources": [{"id": "contract", "title": "Contract",
            "text": "The contract value is now 6000 USD."}]}),
    );
    let list = h.ok("knowledge.sources", serde_json::json!({}));
    let contract = list["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source_id"] == "contract")
        .unwrap();
    assert_eq!(contract["collection"], "Finance");

    // Clearing and removing update the counts.
    h.ok(
        "knowledge.set_collection",
        serde_json::json!({"source_id": "travel", "collection": null}),
    );
    h.ok(
        "knowledge.remove_source",
        serde_json::json!({"source_id": "contract"}),
    );
    let cols = h.ok("knowledge.collections", serde_json::json!({}));
    assert_eq!(cols, serde_json::json!([{"name": "HR", "source_count": 2}]));
}
