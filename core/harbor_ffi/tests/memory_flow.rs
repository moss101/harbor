//! Semantic memory (decision 0012): records with provenance, sealed at
//! rest, indexed as `memory:` knowledge sources that document retrieval
//! never sees.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use harbor_ffi::knowledge::KnowledgeService;
use harbor_ffi::{
    harbor_core_call, harbor_core_close, harbor_core_open_ex, harbor_core_string_free,
};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use harbor_store::keys::KeyMaterial;

struct Handle(*mut harbor_ffi::WorkspaceHandle);

impl Handle {
    fn open(root: &Path) -> Self {
        let root = CString::new(root.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-memory").unwrap();
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
        let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
        unsafe { harbor_core_string_free(raw) };
        serde_json::from_str(&text).expect("response parses")
    }

    fn ok(&self, method: &str, args: serde_json::Value) -> serde_json::Value {
        let v = self.raw(method, args);
        assert!(
            v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false),
            "{method}: {v}"
        );
        v["result"].clone()
    }

    fn fails(&self, method: &str, args: serde_json::Value) -> bool {
        !self
            .raw(method, args)
            .get("ok")
            .and_then(|x| x.as_bool())
            .unwrap_or(false)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

#[test]
fn memory_ops_refuse_cleanly_without_an_index_and_never_leave_orphans() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    let h = Handle::open(&root);

    // No knowledge index open ⇒ nothing can be embedded ⇒ not remembered,
    // and the record is rolled back rather than left unsearchable.
    assert!(h.fails("memory.add", serde_json::json!({"text": "likes tea"})));
    let listed = h.ok("memory.list", serde_json::json!({}));
    assert_eq!(listed["memories"].as_array().unwrap().len(), 0);

    // Validation and provenance errors are typed, not silent.
    assert!(h.fails("memory.add", serde_json::json!({"text": "  "})));
    assert!(h.fails(
        "memory.add",
        serde_json::json!({"text": "x", "provenance": {"origin": "run", "run_id": "", "skill_id": "s"}})
    ));
    assert!(h.fails("memory.delete", serde_json::json!({"id": "mem-nope"})));

    // The reserved prefix cannot be claimed by a document ingest.
    assert!(h.fails(
        "knowledge.ingest",
        serde_json::json!({"sources": [{"id": "memory:evil", "title": "t", "text": "x"}]})
    ));
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn install(models_root: &Path, gguf: &Path) -> String {
    let bytes = std::fs::read(gguf).expect("fixture readable");
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("memory-{}", &sha[..12]);
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

#[test]
fn memory_search_is_semantic_isolated_from_documents_and_survives_reopen() {
    let bge = repo_root().join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    if !bge.exists() {
        eprintln!("SKIP: embedding fixture absent ({})", bge.display());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let data_root = dir.path();
    let models_root = data_root.join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &bge);
    let key = KeyMaterial::random();

    let svc = KnowledgeService::open(data_root, &pkg, key.clone()).unwrap();
    svc.ingest(&[
        (
            "memory:m-tea".into(),
            "Drinks green tea".into(),
            "The user drinks green tea every morning and dislikes coffee.".into(),
        ),
        (
            "memory:m-flight".into(),
            "Flight to Cairo".into(),
            "The user is flying to Cairo on the 14th for the board meeting.".into(),
        ),
        (
            "handbook".into(),
            "Handbook".into(),
            "Employees may claim 180 USD per night for hotels.".into(),
        ),
    ])
    .unwrap();

    // Semantic (not keyword) recall over memory only.
    let hits = svc
        .search_memory("what does the user like to drink?", 5)
        .unwrap();
    assert_eq!(
        hits[0].0, "m-tea",
        "paraphrase finds the tea memory: {hits:?}"
    );
    assert!(
        hits.iter().all(|(id, _)| id.starts_with("m-")),
        "document sources never appear in memory search"
    );

    // And the reverse: document search never returns a memory.
    let docs = svc.search("what does the user like to drink?", 10).unwrap();
    for c in docs["citations"].as_array().unwrap() {
        assert!(
            !c["source_id"].as_str().unwrap().starts_with("memory:"),
            "memory leaked into document retrieval: {c}"
        );
    }

    // Persisted sealed + rebuilt on open: still recallable after reopen.
    drop(svc);
    let svc = KnowledgeService::open(data_root, &pkg, key).unwrap();
    let hits = svc.search_memory("when is the trip to Egypt?", 5).unwrap();
    assert_eq!(hits[0].0, "m-flight", "{hits:?}");

    // Removal is immediate.
    svc.remove_source("memory:m-flight").unwrap();
    let hits = svc.search_memory("when is the trip to Egypt?", 5).unwrap();
    assert!(hits.iter().all(|(id, _)| id != "m-flight"));
}

#[test]
fn document_source_list_excludes_memory_records() {
    let bge = repo_root().join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    if !bge.exists() {
        eprintln!("SKIP: embedding fixture absent ({})", bge.display());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &bge);
    let svc = KnowledgeService::open(dir.path(), &pkg, KeyMaterial::random()).unwrap();
    svc.ingest(&[
        ("memory:m1".into(), "note".into(), "remember this".into()),
        ("doc".into(), "Doc".into(), "a document".into()),
    ])
    .unwrap();
    let listed = svc.sources().unwrap();
    let ids: Vec<&str> = listed["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["source_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["doc"]);
}

#[test]
fn embedder_can_be_released_and_reloads_transparently() {
    let bge = repo_root().join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    if !bge.exists() {
        eprintln!("SKIP: embedding fixture absent ({})", bge.display());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &bge);
    let svc = KnowledgeService::open(dir.path(), &pkg, KeyMaterial::random()).unwrap();
    svc.ingest(&[
        (
            "memory:m1".into(),
            "tea".into(),
            "The user drinks green tea.".into(),
        ),
        (
            "doc".into(),
            "Hotels".into(),
            "Hotel nights are reimbursed up to 180 USD.".into(),
        ),
    ])
    .unwrap();
    // Mobile memory pressure: drop the weights. Index and vectors stay.
    svc.release();
    let hits = svc.search_memory("what does the user drink?", 3).unwrap();
    assert_eq!(hits[0].0, "m1", "search reloads the embedder on demand");
    svc.release();
    svc.ingest(&[(
        "doc2".into(),
        "Leave".into(),
        "24 days of annual leave.".into(),
    )])
    .expect("ingest reloads the embedder on demand");
    let r = svc.search("how many leave days?", 1).unwrap();
    assert_eq!(r["citations"][0]["source_id"], "doc2");
}
