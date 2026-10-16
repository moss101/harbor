//! Opt-in retrieval for the Word drafting skill (decision 0017): off or
//! unavailable it changes nothing; on, the passages become model context
//! and the numeric checker may accept a figure from a CITED passage — and
//! still refuses one that appears nowhere.

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};

use harbor_core::tools::{KnowledgeSearch, MemoryArtifacts, ToolContext, ToolRegistry};

struct Fixture;

impl KnowledgeSearch for Fixture {
    fn search(&self, _query: &str, _top_k: usize) -> Result<Value, String> {
        Ok(json!({"citations": [
            {"source_id": "contract-acme", "title": "Acme supply contract", "chunk_id": "c-1",
             "score": 0.81, "_text": "Payment terms are net 45 days. The late fee is 1.5% per month."},
            {"source_id": "contract-acme", "title": "Acme supply contract", "chunk_id": "c-1",
             "score": 0.80, "_text": "duplicate chunk must be dropped"},
            {"source_id": "blank", "title": "Blank", "chunk_id": "c-2", "score": 0.5, "_text": "  "},
        ]}))
    }
}

struct Broken;

impl KnowledgeSearch for Broken {
    fn search(&self, _q: &str, _k: usize) -> Result<Value, String> {
        Err("index is rebuilding".into())
    }
}

fn call(knowledge: Option<&dyn KnowledgeSearch>, tool: &str, args: Value) -> Value {
    let artifacts = MemoryArtifacts::new();
    let host = json!({});
    let cancel = AtomicBool::new(false);
    let mut ctx = ToolContext::new(&artifacts, &host, &cancel);
    ctx.knowledge = knowledge;
    let registry = ToolRegistry::builtin();
    let allow: BTreeSet<String> = registry.ids().into_iter().collect();
    registry.call(&ctx, tool, &args, &allow).unwrap().output
}

const BRIEF: &str = "Write a short report on the Acme relationship for the board.";

#[test]
fn off_unavailable_and_failing_retrieval_hand_the_brief_back_unchanged() {
    let cases: [(Option<&dyn KnowledgeSearch>, Value); 4] = [
        (Some(&Fixture as &dyn KnowledgeSearch), json!(false)),
        (Some(&Fixture as &dyn KnowledgeSearch), Value::Null),
        (None, json!(true)),
        (Some(&Broken as &dyn KnowledgeSearch), json!(true)),
    ];
    for (k, enabled) in cases {
        let out = call(
            k,
            "knowledge.gather",
            json!({"enabled": enabled, "base": BRIEF}),
        );
        assert_eq!(out["available"], false, "{out}");
        assert_eq!(out["source_text"], BRIEF);
        assert_eq!(out["citations"], json!([]));
        assert_eq!(out["passages_text"], "");
    }
}

#[test]
fn a_figure_from_a_cited_passage_is_grounded_and_an_invented_one_is_not() {
    let out = call(
        Some(&Fixture),
        "knowledge.gather",
        json!({"enabled": true, "base": BRIEF}),
    );
    assert_eq!(out["available"], true);
    // Duplicate chunk and blank passage dropped; the one passage is cited.
    let cites = out["citations"].as_array().unwrap();
    assert_eq!(cites.len(), 1, "{out}");
    assert_eq!(cites[0]["source_id"], "contract-acme");
    let source = out["source_text"].as_str().unwrap();
    assert!(source.starts_with(BRIEF) && source.contains("net 45 days"));

    let draft = |text: &str| {
        json!({"title": "Acme", "sections": [
            {"heading": "Summary", "paragraphs": [text]}]})
    };
    let verify = |text: &str, src: &str| {
        call(
            None,
            "document.verify_draft",
            json!({"draft": draft(text), "source": src, "required_headings": ["Summary"],
                   "max_paragraph_words": 140, "layout": "headed"}),
        )
    };
    let from_passage = "Acme's payment terms are net 45 days with a late fee of 1.5% per month.";
    // With the passage in the source the figures are grounded ...
    assert_eq!(verify(from_passage, source)["ok"], true);
    // ... and without retrieval the same sentence is refused as invented.
    assert_eq!(verify(from_passage, BRIEF)["ok"], false);
    // A figure in neither the brief nor any passage stays refused.
    let invented = verify("Acme's payment terms are net 90 days.", source);
    assert_eq!(invented["ok"], false, "{invented}");
}
