//! The decision layer's non-skill task: what KIND of action does a request
//! need (decision 0019)? The same embedding router that picks a skill, over
//! a fixed catalog of action kinds, with its own per-embedder calibration.
//!
//! This is the MediaPipe-Decision-Maker-shaped component the plan asked for,
//! served by whichever embedder is installed (EmbeddingGemma 2, BGE-M3, ...)
//! and independent of the user's chat model. It RECOMMENDS and may abstain; it
//! never starts, approves or authorizes anything - every action it names still
//! goes through the executor's admission and approval machinery.

use crate::router::RouteItem;

/// The action kinds a request can need, with the user-language descriptions
/// the router embeds. Ids are stable API (the app switches on them).
pub fn intent_catalog() -> Vec<RouteItem> {
    let examples: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(include_str!("intent_exemplars.json"))
            .expect("intent_exemplars.json parses (checked by test)");
    let i = |id: &str, title: &str, description: &str| RouteItem {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        examples: examples.get(id).cloned().unwrap_or_default(),
    };
    vec![
        i(
            "run_skill",
            "Do a task on my files",
            "Carry out a piece of work on a document, spreadsheet, presentation or data: build, edit, analyze, convert, clean, summarize or produce a file or report.",
        ),
        i(
            "search_documents",
            "Find something in my documents",
            "Look up information in the documents I have added, find where something is written, and answer with the source.",
        ),
        i(
            "remember_fact",
            "Remember something",
            "Save a fact, preference or note about me or my business so it can be recalled later.",
        ),
        i(
            "recall_memory",
            "Recall what I told you",
            "Remind me of something I previously asked you to remember, such as a preference, a name or an earlier decision.",
        ),
        i(
            "schedule_goal",
            "Do this on a schedule",
            "Run a task again later or repeatedly: every day, every week, on a date, or as a reminder to check something.",
        ),
        i(
            "read_scan",
            "Read a scan or photo",
            "Read the text from a scanned page, a photo of a document, a receipt or an image-only PDF.",
        ),
        i(
            "transcribe_audio",
            "Transcribe a recording",
            "Turn a voice memo, call or meeting recording into text.",
        ),
        i(
            "general_chat",
            "Ask or discuss",
            "Answer a general question, explain a concept, brainstorm, or have a conversation that needs no files.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_described() {
        let c = intent_catalog();
        let mut ids: Vec<&str> = c.iter().map(|i| i.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), c.len());
        assert!(c.iter().all(|i| i.description.len() > 40));
        // Every action kind carries examples in English, Arabic and French,
        // and none of them is a case of the evaluation corpus (no leakage).
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../../../evals/decision/intents.json")).unwrap();
        let eval: std::collections::BTreeSet<String> = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["input"].as_str().unwrap().trim().to_lowercase())
            .collect();
        for item in &c {
            assert!(
                item.examples.len() >= 8,
                "{} has {} examples",
                item.id,
                item.examples.len()
            );
            assert!(
                item.examples
                    .iter()
                    .any(|e| e.chars().any(|ch| ('\u{0600}'..='\u{06FF}').contains(&ch))),
                "{}: no Arabic example",
                item.id
            );
            for e in &item.examples {
                assert!(
                    !eval.contains(&e.trim().to_lowercase()),
                    "example leaks the eval corpus: {e}"
                );
            }
        }
    }
}
