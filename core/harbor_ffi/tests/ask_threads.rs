//! Multi-turn Ask with durable threads through the real FFI boundary:
//! thread CRUD, conversation memory in generation, persistence across a
//! workspace restart, citation state after a source is removed, and
//! at-rest sealing of conversation content.

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
        let ws = CString::new("ws-threads").unwrap();
        // Injected device root: never the platform Keychain under a runner.
        let root_hex =
            CString::new("3c1f6a0d8b7e4952a1d03f76c8e5b2194f60a7d3e81c5b92d4a6f0378e1c59ab")
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
    for _ in 0..1200 {
        let st = h.call("op.status", serde_json::json!({"op_id": op_id}));
        match st["state"].as_str() {
            Some("done") => return st["result"].clone(),
            Some("failed") | Some("cancelled") => panic!("op ended: {st}"),
            _ => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    panic!("op timed out");
}

fn thread_ids(h: &Handle) -> Vec<String> {
    h.call("thread.list", serde_json::json!({}))["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["thread_id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn thread_lifecycle_through_the_ffi() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    assert!(thread_ids(&h).is_empty());

    let t = h.call("thread.create", serde_json::json!({}))["thread"].clone();
    let id = t["thread_id"].as_str().unwrap().to_string();
    assert!(id.starts_with("thr-"));
    assert_eq!(t["title"], "");

    // A retrieval-only turn is recorded and names the untitled thread.
    h.call(
        "thread.append_search",
        serde_json::json!({
            "thread_id": id,
            "question": "contract value",
            "citations": [{"source_id": "s", "title": "MSA", "content_hash": "h",
                           "state": "Current", "score": 0.5, "_text": "LEAK"}],
        }),
    );
    let got = h.call("thread.get", serde_json::json!({"thread_id": id}));
    assert_eq!(got["thread"]["title"], "contract value");
    assert_eq!(got["thread"]["turns"], 1);
    let msgs = got["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1]["kind"], "search");
    assert!(msgs[1]["meta"]["citations"][0].get("_text").is_none());

    // Survives a restart of the workspace.
    drop(h);
    let h = Handle::open(dir.path());
    assert_eq!(thread_ids(&h), vec![id.clone()]);

    h.call(
        "thread.rename",
        serde_json::json!({"thread_id": id, "title": "Renamed"}),
    );
    assert_eq!(
        h.call("thread.get", serde_json::json!({"thread_id": id}))["thread"]["title"],
        "Renamed"
    );
    assert!(h
        .call_err("thread.get", serde_json::json!({"thread_id": "thr-nope"}))
        .contains("not found"));
    assert_eq!(
        h.call("thread.delete", serde_json::json!({"thread_id": id}))["deleted"],
        true
    );
    assert!(thread_ids(&h).is_empty());

    h.call("thread.create", serde_json::json!({"title": "a"}));
    h.call("thread.create", serde_json::json!({"title": "b"}));
    assert_eq!(
        h.call("thread.delete_all", serde_json::json!({}))["deleted"],
        2
    );
}

#[test]
fn generation_into_an_unknown_thread_is_refused_before_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    let e = h.call_err(
        "op.start_generate",
        serde_json::json!({
            "question": "q", "chat_package": "stories260k", "thread_id": "thr-nope",
        }),
    );
    assert!(e.contains("thread not found"), "{e}");
}

#[test]
fn multi_turn_ask_remembers_persists_and_reflects_removed_sources() {
    let root = repo_root();
    let chat = root.join("fixtures/models/stories260K.gguf");
    let bge = root.join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    assert!(
        chat.exists() && bge.exists(),
        "fixture models missing ({} / {})",
        chat.display(),
        bge.display()
    );
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    h.call(
        "models.install_from_path",
        serde_json::json!({"package_id": "stories260k", "path": chat.to_string_lossy()}),
    );
    h.call(
        "models.install_from_path",
        serde_json::json!({"package_id": "bge-small-en-v1.5", "path": bge.to_string_lossy()}),
    );
    h.call("knowledge.open", serde_json::json!({}));
    h.call(
        "knowledge.ingest",
        serde_json::json!({"sources": [{
            "id": "contract", "title": "Master Services Agreement",
            "text": "The contract value is 5000 USD. The agreement ends on 2026-12-31.",
        }]}),
    );

    let id = h.call("thread.create", serde_json::json!({}))["thread"]["thread_id"]
        .as_str()
        .unwrap()
        .to_string();
    let ask = |q: &str| {
        let op = h.call(
            "op.start_generate",
            serde_json::json!({
                "question": q, "chat_package": "stories260k",
                "max_tokens": 16, "thread_id": id,
            }),
        );
        wait_op(&h, op["op_id"].as_str().unwrap())
    };

    let first = ask("What is the contract value?");
    assert_eq!(first["history_turns"], 0, "first turn has no memory");
    assert_eq!(first["thread_id"], id.as_str());
    assert!(first["thread_error"].is_null(), "{first}");
    // A short follow-up carries the first turn as conversation memory.
    let second = ask("and the end date?");
    assert_eq!(second["history_turns"], 1, "{second}");
    assert!(second["thread_error"].is_null(), "{second}");

    let got = h.call("thread.get", serde_json::json!({"thread_id": id}));
    assert_eq!(got["thread"]["title"], "What is the contract value?");
    assert_eq!(got["thread"]["turns"], 2);
    let msgs = got["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[2]["text"], "and the end date?");
    assert_eq!(msgs[1]["text"], first["answer"]);
    let cite = &msgs[1]["meta"]["citations"][0];
    assert_eq!(cite["source_id"], "contract");
    assert_eq!(cite["state"], "Current");
    assert!(cite.get("_text").is_none(), "chunk text must not be stored");

    // A history too long for the model's context degrades to a shorter
    // memory instead of failing the answer: stories260K trains on 512
    // tokens, so four maximal turns cannot fit and must be shed.
    {
        use harbor_ffi::knowledge::ChatHandle;
        use harbor_ffi::threads::HistoryTurn;
        // Arabic tokenizes to several tokens per character on this 512-entry
        // vocabulary, so the bounded history still overflows the context.
        let big = "سؤال عن العقد ".repeat(60);
        let history: Vec<HistoryTurn> = (0..4)
            .map(|_| HistoryTurn {
                question: big.clone(),
                answer: big.clone(),
            })
            .collect();
        let chat_handle = ChatHandle::new(&dir.path().join("models"));
        let answer = chat_handle
            .generate_rag_with_history(
                "stories260k",
                "What is the contract value?",
                vec![serde_json::json!({"title": "t", "_text": "The contract value is 5000 USD."})],
                &history,
                16,
                &std::sync::atomic::AtomicBool::new(false),
                None,
            )
            .expect("history must be shed, not fail the answer");
        assert!(answer.used_citations);
        assert!(answer.completion_tokens > 0);
    }

    // The conversation survives a restart; the index must be reopened
    // for citation states to resolve against it.
    drop(h);
    let h = Handle::open(dir.path());
    h.call("knowledge.open", serde_json::json!({}));
    let got = h.call("thread.get", serde_json::json!({"thread_id": id}));
    assert_eq!(got["messages"].as_array().unwrap().len(), 4);

    // Removing the source: the historical answer stays, its citation
    // resolves as Removed (never silently current).
    h.call(
        "knowledge.remove_source",
        serde_json::json!({"source_id": "contract"}),
    );
    let got = h.call("thread.get", serde_json::json!({"thread_id": id}));
    let msgs = got["messages"].as_array().unwrap();
    assert_eq!(msgs[1]["text"], first["answer"]);
    assert_eq!(msgs[1]["meta"]["citations"][0]["state"], "Removed");

    // At rest: nothing from the conversation is readable on disk.
    for f in std::fs::read_dir(dir.path().join("db")).unwrap().flatten() {
        let bytes = std::fs::read(f.path()).unwrap();
        for needle in ["and the end date?", "What is the contract value?"] {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
                "plaintext conversation text in {:?}",
                f.path()
            );
        }
    }
}

fn install_and_index(h: &Handle, chat_pkg: &str, chat: &std::path::Path, bge: &std::path::Path) {
    h.call(
        "models.install_from_path",
        serde_json::json!({"package_id": chat_pkg, "path": chat.to_string_lossy()}),
    );
    h.call(
        "models.install_from_path",
        serde_json::json!({"package_id": "bge-small-en-v1.5", "path": bge.to_string_lossy()}),
    );
    h.call("knowledge.open", serde_json::json!({}));
    h.call(
        "knowledge.ingest",
        serde_json::json!({"sources": [
            {"id": "acme", "title": "Acme Agreement",
             "text": "The Acme agreement has a contract value of 5000 USD. The Acme agreement ends on 2026-12-31."},
            {"id": "borealis", "title": "Borealis Agreement",
             "text": "The Borealis agreement has a contract value of 9000 USD. The Borealis agreement ends on 2027-06-30."},
        ]}),
    );
}

/// The point of the feature, with a model that can actually follow a
/// conversation: "And when does it end?" names no agreement, so it is
/// only answerable from the thread. Two near-identical contracts make
/// guessing visible — the control asks the same words with no thread.
#[test]
fn real_chat_model_resolves_a_follow_up_from_thread_memory() {
    let root = repo_root();
    let chat = root.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    let bge = root.join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    assert!(
        chat.exists() && bge.exists(),
        "fixture models missing ({} / {})",
        chat.display(),
        bge.display()
    );
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    install_and_index(&h, "qwen2.5-1.5b", &chat, &bge);

    let ask = |q: &str, thread: Option<&str>| {
        let mut args = serde_json::json!({
            "question": q, "chat_package": "qwen2.5-1.5b", "max_tokens": 48,
        });
        if let Some(t) = thread {
            args["thread_id"] = t.into();
        }
        let op = h.call("op.start_generate", args);
        wait_op(&h, op["op_id"].as_str().unwrap())
    };
    let id = h.call("thread.create", serde_json::json!({}))["thread"]["thread_id"]
        .as_str()
        .unwrap()
        .to_string();

    let first = ask(
        "What is the contract value of the Acme agreement?",
        Some(&id),
    );
    assert!(
        first["answer"].as_str().unwrap().contains("5000"),
        "sanity: the model answers the first question: {first}"
    );
    let follow_up = ask("And when does it end?", Some(&id));
    let answer = follow_up["answer"].as_str().unwrap();
    assert_eq!(follow_up["history_turns"], 1);
    assert!(
        answer.contains("2026-12-31") || answer.contains("December 31, 2026"),
        "the follow-up must resolve 'it' to Acme through the thread: {answer}"
    );
    assert!(
        !answer.contains("2027-06-30"),
        "must not answer for the other agreement: {answer}"
    );

    // Control (informational, not asserted: model output can vary by
    // hardware): the same words with no thread have nothing to resolve
    // 'it' against, and the model hedges across both agreements.
    let control = ask("And when does it end?", None);
    assert_eq!(control["history_turns"], 0);
    eprintln!("follow-up (thread): {answer}");
    eprintln!("follow-up (control): {}", control["answer"]);
}

/// Purge-derived-content through the FFI: previewed first, then explicit;
/// removing the source alone never touches the conversation.
#[test]
fn removing_a_source_keeps_chat_until_an_explicit_previewed_purge() {
    let root = repo_root();
    let chat = root.join("fixtures/models/stories260K.gguf");
    let bge = root.join("fixtures/models/bge-small-en-v1.5-q8_0.gguf");
    let dir = tempfile::tempdir().unwrap();
    let h = Handle::open(dir.path());
    install_and_index(&h, "stories260k", &chat, &bge);

    let id = h.call("thread.create", serde_json::json!({}))["thread"]["thread_id"]
        .as_str()
        .unwrap()
        .to_string();
    let ask = |q: &str| {
        let op = h.call(
            "op.start_generate",
            serde_json::json!({"question": q, "chat_package": "stories260k",
                               "max_tokens": 8, "thread_id": id}),
        );
        wait_op(&h, op["op_id"].as_str().unwrap())
    };
    ask("What is the Acme contract value?");
    ask("What is the Borealis contract value?");

    // Which source did each turn cite? (top-3 over two sources cites both.)
    let preview = h.call(
        "thread.derived_from_source",
        serde_json::json!({"source_id": "acme"}),
    );
    assert_eq!(preview["threads"][0]["thread_id"], id.as_str());
    assert!(preview["turns"].as_u64().unwrap() >= 1, "{preview}");
    // Preview is read-only.
    assert_eq!(
        h.call("thread.get", serde_json::json!({"thread_id": id}))["messages"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    // Removing the source does NOT purge chat.
    h.call(
        "knowledge.remove_source",
        serde_json::json!({"source_id": "acme"}),
    );
    assert_eq!(
        h.call("thread.get", serde_json::json!({"thread_id": id}))["messages"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    // The explicit purge removes the derived turns, and only those.
    let report = h.call(
        "thread.purge_source",
        serde_json::json!({"source_id": "acme"}),
    );
    assert_eq!(report["turns_removed"], preview["turns"]);
    assert!(h.call(
        "thread.derived_from_source",
        serde_json::json!({"source_id": "acme"})
    )["threads"]
        .as_array()
        .unwrap()
        .is_empty());
}
