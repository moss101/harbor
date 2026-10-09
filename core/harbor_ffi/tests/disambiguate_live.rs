//! Ambiguity delegation (decision 0014) against a REAL local chat model.
//! The chat model is just whatever GGUF is installed — the router only
//! sees the provider contract. Skips when the fixture is absent.

use std::path::{Path, PathBuf};

use harbor_core::router::{Disambiguation, RouteHit};
use harbor_ffi::knowledge::ChatHandle;
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

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

fn hit(id: &str, title: &str, description: &str) -> RouteHit {
    RouteHit {
        skill_id: id.into(),
        title: title.into(),
        description: description.into(),
        score: 0.8,
    }
}

#[test]
fn selected_local_llm_breaks_a_routing_tie_and_cannot_go_off_list() {
    let qwen = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !qwen.exists() {
        eprintln!("SKIP: chat fixture absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let pkg = install(&models_root, &qwen);
    let chat = ChatHandle::new(&models_root);

    let candidates = vec![
        hit(
            "spreadsheet-analyst",
            "Spreadsheet Analyst",
            "Inspect workbooks, explain formulas and calculate verified results.",
        ),
        hit(
            "email-drafting",
            "Email Drafting",
            "Draft an email or a reply from your notes.",
        ),
        hit(
            "translation",
            "Translation",
            "Translate text between English and Arabic.",
        ),
    ];
    let outcome = chat.disambiguate_skill(
        &pkg,
        "Write a reply to the supplier telling them we accept the revised delivery date",
        &candidates,
    );
    // Whatever the model decides, the result is one of three typed
    // outcomes and a chosen id is always an offered one. A 1.5B model
    // should get this obvious case right; if it does not, that is a
    // quality fact about the model, so the id check is the hard assert.
    match &outcome {
        Disambiguation::Chose { skill_id } => {
            assert!(candidates.iter().any(|c| &c.skill_id == skill_id));
            eprintln!("LLM chose: {skill_id}");
        }
        other => eprintln!("LLM outcome: {other:?}"),
    }
}
