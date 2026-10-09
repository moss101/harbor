//! Embedding instruction policies: the query/document text each model
//! family must be fed, owned HERE so production, the test backend and the
//! live qualification tier cannot drift.
//!
//! Before decision 0011 the e5 `query:`/`passage:` anchors existed only in
//! the live harness (keyed on the model file name) while the FFI service
//! embedded raw text — production silently diverged from the qualified
//! tier. The policy is now part of the index identity, so a policy change
//! re-embeds through the tested rebuild path instead of mixing vectors.

/// Which instruction text wraps embeddings for a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionPolicy {
    /// No anchors (bge family, generic test backends).
    None,
    /// `query: ` / `passage: ` (multilingual-e5 family).
    E5,
    /// EmbeddingGemma task prefixes (model card, revision 914f7f89):
    /// queries `task: search result | query: {q}`, documents
    /// `title: {title} | text: {text}` with `title: none` when untitled.
    GemmaEmbedding,
}

impl InstructionPolicy {
    /// Resolve the policy for an installed package id or model file
    /// name. Matching is by family substring — the same rule the live
    /// tier has always applied to e5 file names.
    pub fn for_package(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower.contains("embeddinggemma") {
            InstructionPolicy::GemmaEmbedding
        } else if lower.contains("e5") {
            InstructionPolicy::E5
        } else {
            InstructionPolicy::None
        }
    }

    /// Identity string hashed into the index identity: a policy change is
    /// a preprocessing change and must rebuild, never merge.
    pub fn identity(&self) -> &'static str {
        match self {
            InstructionPolicy::None => "none/1",
            InstructionPolicy::E5 => "e5/1",
            InstructionPolicy::GemmaEmbedding => "gemma-embedding/1",
        }
    }

    /// Text embedded for one document chunk.
    pub fn format_document(&self, title: &str, text: &str) -> String {
        match self {
            InstructionPolicy::None => text.to_string(),
            InstructionPolicy::E5 => format!("passage: {text}"),
            InstructionPolicy::GemmaEmbedding => {
                let title = title.trim();
                let title = if title.is_empty() { "none" } else { title };
                format!("title: {title} | text: {text}")
            }
        }
    }

    /// Text embedded for one search query / request.
    pub fn format_query(&self, query: &str) -> String {
        match self {
            InstructionPolicy::None => query.to_string(),
            InstructionPolicy::E5 => format!("query: {query}"),
            InstructionPolicy::GemmaEmbedding => {
                format!("task: search result | query: {query}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_resolution_by_package_name() {
        assert_eq!(
            InstructionPolicy::for_package("multilingual-e5-small"),
            InstructionPolicy::E5
        );
        assert_eq!(
            InstructionPolicy::for_package("EmbeddingGemma-2-Text"),
            InstructionPolicy::GemmaEmbedding
        );
        assert_eq!(
            InstructionPolicy::for_package("bge-m3"),
            InstructionPolicy::None
        );
        assert_eq!(
            InstructionPolicy::for_package("bge-small-en-v1.5"),
            InstructionPolicy::None
        );
        assert_eq!(
            InstructionPolicy::for_package("stories260k"),
            InstructionPolicy::None
        );
    }

    #[test]
    fn e5_anchors_match_the_qualified_rule() {
        let p = InstructionPolicy::E5;
        assert_eq!(p.format_query("contract value"), "query: contract value");
        assert_eq!(p.format_document("T", "body"), "passage: body");
    }

    #[test]
    fn gemma_prefixes_follow_the_model_card() {
        let p = InstructionPolicy::GemmaEmbedding;
        assert_eq!(
            p.format_query("risk clause"),
            "task: search result | query: risk clause"
        );
        assert_eq!(
            p.format_document("Contract", "body text"),
            "title: Contract | text: body text"
        );
        // The card: use `title: none` when no title is available.
        assert_eq!(
            p.format_document("  ", "body text"),
            "title: none | text: body text"
        );
    }

    #[test]
    fn none_policy_is_identity() {
        let p = InstructionPolicy::None;
        assert_eq!(p.format_query("q"), "q");
        assert_eq!(p.format_document("t", "d"), "d");
    }

    #[test]
    fn every_policy_has_a_distinct_identity_string() {
        let ids: Vec<&str> = [
            InstructionPolicy::None,
            InstructionPolicy::E5,
            InstructionPolicy::GemmaEmbedding,
        ]
        .iter()
        .map(|p| p.identity())
        .collect();
        assert_eq!(ids.len(), 3);
        for (i, a) in ids.iter().enumerate() {
            for b in ids.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }
}
