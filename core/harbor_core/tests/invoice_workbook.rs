//! The supplier-invoice report, end to end through the real tools: the
//! records the model would read -> invoice.review -> workbook.build -> the
//! bytes a user would save -> read back. The spreadsheet and the summary must
//! agree with the hand-computed fixture figures (decision 0017).
//!
//! `HARBOR_WRITE_SAMPLES=<dir>` also writes the file for a manual open in
//! Excel / Numbers (`-- --ignored`).

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};

use harbor_artifacts::WorkbookDoc;
use harbor_core::tools::{MemoryArtifacts, ToolContext, ToolRegistry};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn records() -> Value {
    let cassette: Value = serde_json::from_str(
        &std::fs::read_to_string(
            root().join("evals/skills/supplier-invoice-review/cassettes/batch.json"),
        )
        .unwrap(),
    )
    .unwrap();
    serde_json::from_str(
        cassette["entries"][0]["response"]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}

fn build() -> (Value, Value, Vec<u8>) {
    let source =
        std::fs::read_to_string(root().join("fixtures/business/supplier_invoices.txt")).unwrap();
    let previous =
        std::fs::read_to_string(root().join("fixtures/business/previous_records.csv")).unwrap();
    let artifacts = MemoryArtifacts::new();
    let host = json!({});
    let cancel = AtomicBool::new(false);
    let ctx = ToolContext::new(&artifacts, &host, &cancel);
    let registry = ToolRegistry::builtin();
    let allow: BTreeSet<String> = registry.ids().into_iter().collect();
    let call = |tool: &str, args: Value| registry.call(&ctx, tool, &args, &allow).unwrap().output;
    let review = call(
        "invoice.review",
        json!({"records": records()["records"], "source": source, "as_of": "2026-10-09", "previous": previous}),
    );
    assert_eq!(review["ok"], true, "{}", review["problems"]);
    let proposal = call("workbook.build", json!({"spec": review["spec"]}));
    let batch = harbor_core::tools::builtin::batch_from_value(&proposal["batch"]).unwrap();
    let bytes = harbor_core::tools::builtin::apply_batch(&[], &batch).unwrap();
    assert_eq!(
        harbor_canonical::sha256_hex(&bytes),
        proposal["proposed_output_hash"].as_str().unwrap()
    );
    (review, proposal, bytes)
}

#[test]
fn the_saved_workbook_matches_the_known_fixture_figures() {
    let (review, proposal, bytes) = build();
    let doc = WorkbookDoc::load(&bytes).expect("the bytes are a loadable workbook");
    let sheet = doc.sheet("Invoices").unwrap();
    let cell = |c: u32, r: u32| sheet.cells.get(&(c, r)).cloned().unwrap();
    // Header + 5 invoices + total row.
    assert_eq!(proposal["checks"]["errors"], json!([]));
    let totals = proposal["checks"]["totals"].as_array().unwrap();
    assert_eq!(totals.len(), 2);
    assert_eq!(totals[0]["column"], "Amount");
    assert_eq!(totals[0]["formula"], "=SUM(F2:F6)");
    assert_eq!(totals[0]["value"], "12350.5");
    assert_eq!(totals[1]["column"], "Overdue amount");
    assert_eq!(totals[1]["formula"], "=SUM(G2:G6)");
    assert_eq!(totals[1]["value"], "9150.5");
    // The spreadsheet total equals the summary, in cents.
    assert_eq!(review["summary"]["total_overdue_cents"], 915_050);
    assert_eq!(review["summary"]["total_invoiced_cents"], 1_235_050);
    // Rows are due-date ordered; the earliest due date is the paid INV-1004.
    let first_invoice = cell(1, 2);
    assert_eq!(
        format!("{:?}", first_invoice.cached.unwrap()),
        "Text(\"INV-1004\")"
    );
    assert!(
        sheet.cells.contains_key(&(5, 2)),
        "days overdue is a number cell"
    );
}

#[test]
fn the_summary_text_names_every_overdue_invoice_and_each_difference() {
    let (review, _, _) = build();
    let t = review["summary_text"].as_str().unwrap();
    for must in [
        "5 invoice(s) reviewed as of 2026-10-09",
        "3 overdue invoice(s) totalling USD 9,150.50",
        "INV-1001 (Acme Supplies): USD 4,200.00 due 2026-08-31, 39 day(s) overdue",
        "INV-1005 (Delta Logistics): USD 3,100.00 due 2026-09-19, 20 day(s) overdue",
        "INV-1002 (Northwind Parts): USD 1,850.50 due 2026-10-05, 4 day(s) overdue",
        "1,580.50",
        "INV-0990",
    ] {
        assert!(t.contains(must), "summary lacks {must:?}:\n{t}");
    }
    assert!(
        !t.contains("INV-1003 (Acme"),
        "a not-yet-due invoice must not be listed overdue"
    );
    assert!(
        !t.contains("INV-1004 (Delta"),
        "a paid invoice must not be listed overdue"
    );
}

#[test]
#[ignore = "writes the report for a manual open in Excel (HARBOR_WRITE_SAMPLES)"]
fn write_invoice_report() {
    let out = std::path::PathBuf::from(std::env::var("HARBOR_WRITE_SAMPLES").unwrap());
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("Supplier invoice review.xlsx"), build().2).unwrap();
}

// ---------------------------------------------------------------------------
// The cross-Office chain (decision 0017): invoices -> Excel analysis -> Word
// management report -> PowerPoint deck. Every figure the report and the deck
// carry is checked against the SAME source text (the deterministic review
// summary), and the three files are read back. The model-written parts are
// scripted here; the live-model chain is not claimed by this test.
// ---------------------------------------------------------------------------

use harbor_artifacts::{DocxDocument, PptxDeck};

fn tool_call(tool: &str, args: Value) -> Value {
    let artifacts = MemoryArtifacts::new();
    let host = json!({});
    let cancel = AtomicBool::new(false);
    let ctx = ToolContext::new(&artifacts, &host, &cancel);
    let registry = ToolRegistry::builtin();
    let allow: BTreeSet<String> = registry.ids().into_iter().collect();
    registry.call(&ctx, tool, &args, &allow).unwrap().output
}

fn render(proposal: &Value) -> Vec<u8> {
    let batch = harbor_core::tools::builtin::batch_from_value(&proposal["batch"]).unwrap();
    let bytes = harbor_core::tools::builtin::apply_batch(&[], &batch).unwrap();
    assert_eq!(
        harbor_canonical::sha256_hex(&bytes),
        proposal["proposed_output_hash"].as_str().unwrap()
    );
    bytes
}

#[test]
fn invoices_to_excel_to_word_to_powerpoint_keep_one_set_of_figures() {
    let (review, _proposal, xlsx) = build();
    let brief = review["summary_text"].as_str().unwrap().to_string();

    // Word management report: a scripted draft that uses only figures in the
    // brief. document.verify_draft must accept it.
    let draft = json!({
        "title": "Supplier payments: management report",
        "sections": [
            {"heading": "Position", "paragraphs": [
                "5 invoice(s) reviewed as of 2026-10-09; USD 12,350.50 invoiced in total."]},
            {"heading": "Overdue", "paragraphs": [
                "3 overdue invoice(s) totalling USD 9,150.50. INV-1001 (Acme Supplies) is 39 day(s) overdue."]},
        ]
    });
    let ok = tool_call(
        "document.verify_draft",
        json!({
        "draft": draft, "source": brief, "required_headings": ["Position", "Overdue"],
        "max_paragraph_words": 140, "layout": "headed"}),
    );
    assert_eq!(ok["ok"], true, "{ok}");
    let docx = render(&tool_call("docx.build", json!({"document": draft})));

    // The same draft with one invented figure is refused by the checker.
    let mut bad = draft.clone();
    bad["sections"][1]["paragraphs"][0] = json!("3 overdue invoice(s) totalling USD 19,150.50.");
    let refused = tool_call(
        "document.verify_draft",
        json!({
        "draft": bad, "source": brief, "required_headings": ["Position", "Overdue"],
        "max_paragraph_words": 140, "layout": "headed"}),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(refused.to_string().contains("19,150.50"), "{refused}");

    // PowerPoint: outline checked against the same brief, then built.
    let outline = json!({
        "title": "Supplier payments review",
        "subtitle": "As of 2026-10-09",
        "slides": [
            {"title": "Where we stand", "bullets": [
                "5 invoices, USD 12,350.50 invoiced", "3 overdue, USD 9,150.50"], "notes": null},
            {"title": "Largest exposure", "bullets": [
                "INV-1001 Acme Supplies: USD 4,200.00, 39 days late"], "notes": null},
        ]
    });
    let v = tool_call(
        "deck.verify_outline",
        json!({
        "outline": outline, "source": brief, "slides": null,
        "min_slides": 2, "max_slides": 8, "max_bullets": 4, "max_bullet_words": 22}),
    );
    assert_eq!(v["ok"], true, "{v}");
    let pptx = render(&tool_call("deck.build", json!({"outline": outline})));

    if let Ok(dir) = std::env::var("HARBOR_WRITE_SAMPLES") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Supplier invoice review.xlsx"), &xlsx).unwrap();
        std::fs::write(dir.join("Supplier payments management report.docx"), &docx).unwrap();
        std::fs::write(dir.join("Supplier payments review.pptx"), &pptx).unwrap();
    }

    // All three files read back and carry the same two headline figures.
    let wb = WorkbookDoc::load(&xlsx).unwrap();
    let sheet = wb.sheet("Invoices").unwrap();
    let invoiced = format!("{:?}", sheet.cells[&(6, 7)].cached);
    assert!(invoiced.contains("12350.5"), "{invoiced}");
    let word = DocxDocument::load(&docx).unwrap();
    let word_text: String = word
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let deck = PptxDeck::from_pptx_bytes(&pptx).unwrap();
    let deck_text: String = deck
        .slides
        .iter()
        .flat_map(|s| s.bullets.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");
    for text in [&word_text, &deck_text] {
        assert!(
            text.contains("12,350.50") || text.contains("9,150.50"),
            "{text}"
        );
    }
    assert!(word_text.contains("12,350.50") && word_text.contains("9,150.50"));
    assert!(deck_text.contains("12,350.50") && deck_text.contains("9,150.50"));
}
