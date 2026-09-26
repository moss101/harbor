//! Write the files the authoring skills create, for a person to open in
//! Excel, PowerPoint, Word, Numbers, Keynote or Pages — the check no
//! Harbor reader can make (decision 0008: an Office "repair" prompt is a
//! defect Harbor's own parsers do not see). The specs are the committed
//! replay cassettes, so the files are exactly what those runs propose.
//!
//! ```text
//! # from core/
//! HARBOR_WRITE_SAMPLES=/tmp/harbor-samples \
//!   cargo test -p harbor_core --test created_samples -- --ignored --nocapture
//! ```

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};

use harbor_core::tools::{MemoryArtifacts, ToolContext, ToolRegistry};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn cassette_output(skill: &str, cassette: &str, key: &str) -> Value {
    let path = repo_root()
        .join("evals/skills")
        .join(skill)
        .join("cassettes")
        .join(cassette);
    let c: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let entry = c["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["trace_key"] == key)
        .unwrap();
    serde_json::from_str(entry["response"]["content"].as_str().unwrap()).unwrap()
}

#[test]
#[ignore = "writes files for a manual check in Office apps (HARBOR_WRITE_SAMPLES)"]
fn write_created_samples() {
    let out = std::path::PathBuf::from(
        std::env::var("HARBOR_WRITE_SAMPLES").expect("set HARBOR_WRITE_SAMPLES to a folder"),
    );
    std::fs::create_dir_all(&out).unwrap();
    let report = std::fs::read(repo_root().join("fixtures/office/quarterly_report.pdf")).unwrap();
    let artifacts = MemoryArtifacts::new().with("r", "quarterly_report.pdf", report);
    let cancel = AtomicBool::new(false);
    let host = json!({});
    let ctx = ToolContext::new(&artifacts, &host, &cancel);
    let registry = ToolRegistry::builtin();
    let allow: BTreeSet<String> = registry.ids().into_iter().collect();
    let call = |tool: &str, args: Value| registry.call(&ctx, tool, &args, &allow).unwrap().output;
    let write = |name: &str, proposal: &Value| {
        let batch =
            harbor_core::tools::builtin::batch_from_value(&proposal["batch"]).expect("batch");
        let bytes = harbor_core::tools::builtin::apply_batch(&[], &batch).expect("render");
        assert_eq!(
            harbor_canonical::sha256_hex(&bytes),
            proposal["proposed_output_hash"].as_str().unwrap()
        );
        let path = out.join(name);
        std::fs::write(&path, &bytes).unwrap();
        println!("wrote {} ({} bytes)", path.display(), bytes.len());
    };

    let budget = cassette_output("sheet-builder", "budget.json", "sheet-builder/draft#1");
    write(
        "October household budget.xlsx",
        &call("workbook.build", json!({"spec": budget})),
    );

    let doc = call("artifact.read", json!({"artifact_id": "r"}));
    let sentences = call("document.sentences", json!({"document": doc}));
    let outline = cassette_output(
        "report-to-slides",
        "quarterly_report.json",
        "report-to-slides/draft#1",
    );
    write(
        "Northwind Clinics Q3 2026.pptx",
        &call(
            "deck.build",
            json!({"outline": outline, "sentences": sentences}),
        ),
    );

    let deck = cassette_output(
        "presentation-builder",
        "launch_notes.json",
        "presentation-builder/draft#1",
    );
    write(
        "Harbor 1.1 launch review.pptx",
        &call("deck.build", json!({"outline": deck})),
    );

    let proposal = cassette_output(
        "document-drafter",
        "proposal.json",
        "document-drafter/draft#1",
    );
    write(
        "Moving the archive to local storage.docx",
        &call("docx.build", json!({"document": proposal})),
    );
    let letter = cassette_output(
        "document-drafter",
        "letter.json",
        "document-drafter/draft#1",
    );
    write(
        "Lease renewal for Unit 4.docx",
        &call(
            "docx.build",
            json!({"document": letter, "layout": "letter", "letter": {
                "date": "26 September 2026",
                "recipient": "Ms. Laura Grant\nHarbour Properties LLC\nPO Box 1120, Dubai",
                "sender": "Omar Haddad\nOperations Manager, Northwind Clinics"
            }}),
        ),
    );
}
