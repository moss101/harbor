//! Pinned evaluation corpus (evals/en|ar|mixed) and its runner.
//!
//! The corpora are the single source of truth (included at compile time
//! from the repo's evals/ directory); their combined SHA-256 is the
//! `evaluation_corpus_sha256` recorded in 26_Qualification_Profiles.json.

use harbor_inference::backend::TestBackend;
use harbor_inference::provider::{ModelProvider, ModelRef};

use crate::chunk::Chunker;
use crate::chunk::ChunkerConfig;
use crate::eval::{run_eval, EvalCase, EvalReport, GroundedExtractor};
use crate::identity::{embed_model_identity, ChunkerConfig as CC, IndexIdentity, Normalization};
use crate::index::{KnowledgeIndex, Source, SourceChunk};

pub const CORPUS_EN: &str = include_str!("../../../evals/en/corpus.json");
pub const CORPUS_AR: &str = include_str!("../../../evals/ar/corpus.json");
pub const CORPUS_MIXED: &str = include_str!("../../../evals/mixed/corpus.json");

/// Combined identity of all pinned corpora.
pub fn evaluation_corpus_sha256() -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(CORPUS_EN.as_bytes());
    bytes.extend_from_slice(CORPUS_AR.as_bytes());
    bytes.extend_from_slice(CORPUS_MIXED.as_bytes());
    harbor_canonical::sha256_hex(&bytes)
}

#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("json: {0}")]
    Json(String),
}

#[derive(Debug, Clone)]
pub struct CorpusSource {
    pub id: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct PinnedCorpus {
    pub language: String,
    pub sources: Vec<CorpusSource>,
    pub cases: Vec<EvalCase>,
}

fn parse_corpus(json: &str) -> Result<PinnedCorpus, CorpusError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| CorpusError::Json(e.to_string()))?;
    let language = v["language"].as_str().unwrap_or_default().to_string();
    let mut sources = Vec::new();
    for s in v["sources"].as_array().expect("sources") {
        sources.push(CorpusSource {
            id: s["id"].as_str().unwrap_or_default().into(),
            title: s["title"].as_str().unwrap_or_default().into(),
            text: s["text"].as_str().unwrap_or_default().into(),
        });
    }
    let mut cases = Vec::new();
    for c in v["cases"].as_array().expect("cases") {
        cases.push(EvalCase {
            id: c["id"].as_str().unwrap_or_default().into(),
            language: "pinned",
            question: c["question"].as_str().unwrap_or_default().into(),
            expect_sources: c["expect_sources"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            must_include: vec![],
            expect_abstention: c["expect_abstention"].as_bool().unwrap_or(false),
            injection_probe: c["injection_probe"].as_bool().unwrap_or(false),
        });
    }
    Ok(PinnedCorpus {
        language,
        sources,
        cases,
    })
}

/// Build an index over a pinned corpus with the deterministic test
/// embedding (the same identity the qualification profile records).
pub fn build_index(corpus: &PinnedCorpus, provider: &TestBackend) -> KnowledgeIndex {
    let identity = IndexIdentity {
        embedding: embed_model_identity("test-hash", "v1", 8),
        chunker: "paragraph-window/1".into(),
        chunker_config: CC {
            target_graphemes: 800,
            overlap_graphemes: 80,
            respect_paragraphs: true,
        },
        tokenizer: "grapheme/1".into(),
        normalization: Normalization::Nfc,
        language_policy: "en,ar,mixed".into(),
        encryption_scope: "eval".into(),
    };
    let mut index = KnowledgeIndex::new(identity);
    let model = ModelRef::InstalledPackage {
        package_id: "eval-embed".into(),
    };
    let cfg = ChunkerConfig {
        target_graphemes: 800,
        overlap_graphemes: 80,
        respect_paragraphs: true,
    };
    for s in &corpus.sources {
        let chunks = Chunker::chunk(&s.text, &cfg);
        let sc: Vec<SourceChunk> = chunks
            .iter()
            .map(|c| SourceChunk {
                source_id: s.id.clone(),
                chunk_id: format!("{}-{}", s.id, c.ordinal),
                ordinal: c.ordinal,
                text: c.text.clone(),
                vector: provider
                    .embed(&model, std::slice::from_ref(&c.text))
                    .expect("deterministic embed")
                    .into_iter()
                    .next()
                    .unwrap(),
            })
            .collect();
        index
            .add_source(
                Source {
                    source_id: s.id.clone(),
                    title: s.title.clone(),
                    content_hash: harbor_canonical::sha256_hex(s.text.as_bytes()),
                    indexed_at: chrono::Utc::now(),
                },
                sc,
            )
            .expect("corpus source");
    }
    index
}

/// Run all three pinned corpora end-to-end.
pub fn run_pinned_evals() -> Result<(String, Vec<(String, EvalReport)>), CorpusError> {
    let provider = TestBackend::default();
    let model = ModelRef::InstalledPackage {
        package_id: "eval-embed".into(),
    };
    let pipeline = GroundedExtractor {
        provider: &provider,
        model: model.clone(),
    };
    let mut out = Vec::new();
    for json in [CORPUS_EN, CORPUS_AR, CORPUS_MIXED] {
        let corpus = parse_corpus(json)?;
        let index = build_index(&corpus, &provider);
        let embed = |q: &str| -> Option<Vec<f32>> {
            provider
                .embed(&model, &[q.to_string()])
                .ok()
                .and_then(|v| v.into_iter().next())
        };
        let report = run_eval(&index, &pipeline, &embed, &corpus.cases);
        out.push((corpus.language, report));
    }
    Ok((evaluation_corpus_sha256(), out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_corpora_pass_retrieval_abstention_and_injection() {
        let (hash, reports) = run_pinned_evals().unwrap();
        assert_eq!(hash.len(), 64);
        for (lang, report) in &reports {
            for case in &report.cases {
                assert!(
                    case.passed,
                    "{lang}/{} failed: retrieval={} citation={} abstain={} injection={}",
                    case.case_id,
                    case.retrieval_hit,
                    case.citation_supported,
                    case.abstained_correctly,
                    case.injection_resisted
                );
            }
            assert_eq!(report.failed, 0, "{lang}: {} failures", report.failed);
        }
        let total: usize = reports.iter().map(|(_, r)| r.passed).sum();
        // Every case in every pinned corpus must pass; the expected total
        // is derived from the corpora themselves (not hardcoded), so
        // corpus expansion cannot silently skip cases.
        let expected: usize = [CORPUS_EN, CORPUS_AR, CORPUS_MIXED]
            .iter()
            .map(|j| {
                serde_json::from_str::<serde_json::Value>(j).unwrap()["cases"]
                    .as_array()
                    .unwrap()
                    .len()
            })
            .sum();
        assert_eq!(total, expected, "all pinned corpus cases must pass");
    }

    #[test]
    fn corpus_hash_is_stable() {
        assert_eq!(evaluation_corpus_sha256(), evaluation_corpus_sha256());
    }
}
