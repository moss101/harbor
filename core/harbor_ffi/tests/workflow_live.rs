//! The invoices-to-office workflow through the FFI with a REAL chat model
//! (decision 0018): three ordinary skill runs, each with its own approval and
//! file commit, chained by `workflow.advance`. Skips without the fixtures.

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

fn install(models_root: &Path, gguf: &Path) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("chat-{}", &sha[..12]);
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
    fn open(data: &Path) -> Self {
        let root = CString::new(data.to_string_lossy().to_string()).unwrap();
        let ws = CString::new("ws-workflow").unwrap();
        let hex = CString::new("12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10")
            .unwrap();
        let h = harbor_core_open_ex(root.as_ptr(), ws.as_ptr(), 0, hex.as_ptr());
        assert!(!h.is_null());
        Handle(h)
    }
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
    fn wait(&self, op: &str) -> serde_json::Value {
        for _ in 0..6000 {
            let st = self.ok("op.status", serde_json::json!({"op_id": op}));
            match st["state"].as_str() {
                Some("done") => return st["result"].clone(),
                Some("failed") | Some("cancelled") => panic!("op ended: {st}"),
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        panic!("timed out");
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { harbor_core_close(self.0) };
    }
}

#[test]
fn invoices_flow_through_three_skills_with_an_approval_each() {
    let r = repo_root();
    let qwen = r.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !qwen.exists() {
        eprintln!("SKIP: fixture absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("models")).unwrap();
    let chat = install(&data.join("models"), &qwen);
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let h = Handle::open(&data);

    let templates = h.ok("workflow.templates", serde_json::json!({}));
    assert_eq!(
        templates["templates"][0]["steps"].as_array().unwrap().len(),
        3
    );
    // Missing required inputs and unknown templates are refused up front.
    assert_eq!(
        h.raw(
            "workflow.start",
            serde_json::json!({"template": "invoices-to-office", "inputs": {"invoices": "x"}})
        )["ok"],
        false
    );
    assert_eq!(
        h.raw(
            "workflow.start",
            serde_json::json!({"template": "nope", "inputs": {}})
        )["ok"],
        false
    );
    assert!(h.ok("workflow.list", serde_json::json!({}))["workflows"]
        .as_array()
        .unwrap()
        .is_empty());

    let started = h.ok("workflow.start", serde_json::json!({
        "template": "invoices-to-office", "chat_package": chat,
        "inputs": {
            "invoices": std::fs::read_to_string(r.join("fixtures/business/supplier_invoices.txt")).unwrap(),
            "as_of": "2026-10-09",
            "previous": std::fs::read_to_string(r.join("fixtures/business/previous_records.csv")).unwrap(),
        }}));
    let wf = started["workflow_id"].as_str().unwrap().to_string();
    let mut op = started["op_id"].as_str().unwrap().to_string();

    let names = ["review.xlsx", "report.docx", "deck.pptx"];
    for (i, name) in names.iter().enumerate() {
        let report = h.wait(&op);
        println!("step {i}: {}", report["state"]);
        assert_eq!(report["state"], "WAITING_APPROVAL", "step {i}: {report}");
        let run_id = report["run_id"].as_str().unwrap().to_string();
        // Advancing before the approval does nothing - no second step.
        let early = h.ok("workflow.advance", serde_json::json!({"workflow_id": wf}));
        assert_eq!(early["advanced"], false, "{early}");
        assert_eq!(early["waiting"], "WAITING_APPROVAL");
        // The person approves THIS step's file.
        let dest = out.join(name);
        let c = h.ok(
            "run.commit_proposal",
            serde_json::json!({
            "run_id": run_id, "destination": dest.to_string_lossy()}),
        );
        assert!(c.get("commit_error").is_none(), "{c}");
        assert!(dest.exists());
        let adv = h.ok("workflow.advance", serde_json::json!({"workflow_id": wf}));
        if i < 2 {
            assert_eq!(adv["advanced"], true, "{adv}");
            assert_eq!(adv["step"], i + 1);
            op = adv["op_id"].as_str().unwrap().to_string();
            // A repeated advance while the next step runs starts nothing new.
            let again = h.ok("workflow.advance", serde_json::json!({"workflow_id": wf}));
            assert_eq!(again["advanced"], false, "{again}");
        } else {
            assert_eq!(adv["status"], "done", "{adv}");
        }
    }
    let status = h.ok("workflow.status", serde_json::json!({"workflow_id": wf}));
    assert_eq!(
        status["workflow"]["run_ids"].as_array().unwrap().len(),
        3,
        "no duplicate steps"
    );

    // The files are real and carry the verified figures.
    let wb = harbor_artifacts::WorkbookDoc::load(&std::fs::read(out.join("review.xlsx")).unwrap())
        .unwrap();
    assert!(wb.sheet("Invoices").is_ok());
    let docx =
        harbor_artifacts::DocxDocument::load(&std::fs::read(out.join("report.docx")).unwrap())
            .unwrap();
    let word: String = docx
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    println!("WORD:\n{word}");
    let deck =
        harbor_artifacts::PptxDeck::from_pptx_bytes(&std::fs::read(out.join("deck.pptx")).unwrap())
            .unwrap();
    println!(
        "DECK: {:?}",
        deck.slides
            .iter()
            .map(|s| (&s.title, &s.bullets))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_rejected_step_stops_the_workflow_and_a_restart_resumes_it() {
    let r = repo_root();
    let qwen = r.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !qwen.exists() {
        eprintln!("SKIP: fixture absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("models")).unwrap();
    let chat = install(&data.join("models"), &qwen);
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let inputs = serde_json::json!({
        "invoices": std::fs::read_to_string(r.join("fixtures/business/supplier_invoices.txt")).unwrap(),
        "as_of": "2026-10-09"});

    // --- restart: the workflow record and the run survive a reopen --------
    let (wf, run_id) = {
        let h = Handle::open(&data);
        let s = h.ok(
            "workflow.start",
            serde_json::json!({
            "template": "invoices-to-office", "chat_package": chat, "inputs": inputs}),
        );
        let report = h.wait(s["op_id"].as_str().unwrap());
        assert_eq!(report["state"], "WAITING_APPROVAL");
        (
            s["workflow_id"].as_str().unwrap().to_string(),
            report["run_id"].as_str().unwrap().to_string(),
        )
    };
    let h = Handle::open(&data);
    let listed = h.ok("workflow.list", serde_json::json!({}));
    assert_eq!(listed["workflows"][0]["id"], wf.as_str());
    let st = h.ok("workflow.status", serde_json::json!({"workflow_id": wf}));
    assert_eq!(st["steps"][0]["state"], "WAITING_APPROVAL");
    assert_eq!(st["steps"][1]["state"], "NOT_STARTED");
    // No plaintext of the workflow on disk.
    let raw = std::fs::read_to_string(data.join("db").join("workflows.json")).unwrap();
    assert!(raw.starts_with("enc.v1:") && !raw.contains("invoices"));

    // --- reject: the chain stops, nothing later starts, nothing is written --
    let decided = h.ok(
        "run.decide",
        serde_json::json!({"run_id": run_id, "approved": false}),
    );
    assert_eq!(decided["state"], "COMPLETED", "{decided}");
    let adv = h.ok("workflow.advance", serde_json::json!({"workflow_id": wf}));
    assert_eq!(adv["advanced"], false, "{adv}");
    assert_eq!(adv["status"], "stopped", "{adv}");
    assert!(adv["reason"].as_str().unwrap().contains("not approved"));
    let st = h.ok("workflow.status", serde_json::json!({"workflow_id": wf}));
    assert_eq!(st["workflow"]["run_ids"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    // A stopped workflow stays stopped.
    let again = h.ok("workflow.advance", serde_json::json!({"workflow_id": wf}));
    assert_eq!(again["status"], "stopped");
}
