//! Knowledge index: versioned sources, identity-checked vectors,
//! citation-aware search with current/changed/removed states.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::identity::IndexIdentity;

#[derive(Debug, Clone)]
pub struct Source {
    pub source_id: String,
    pub title: String,
    /// Content hash of the source as indexed (version binding).
    pub content_hash: String,
    pub indexed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SourceChunk {
    pub source_id: String,
    pub chunk_id: String,
    pub ordinal: u32,
    pub text: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceVersionState {
    Current,
    Changed,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Citation {
    pub source_id: String,
    pub title: String,
    pub chunk_id: String,
    pub score: f32,
    pub state: SourceVersionState,
    /// Hash of the source content AS CITED.
    pub content_hash: String,
}

/// A citation plus its chunk text (generation input, not display IR).
#[derive(Debug, Clone, PartialEq)]
pub struct CitationWithText {
    pub citation: Citation,
    pub text: String,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("index identity mismatch: {0}")]
    IdentityMismatch(String),
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },
    #[error("source not found: {0}")]
    SourceNotFound(String),
}

pub struct KnowledgeIndex {
    pub identity: IndexIdentity,
    sources: BTreeMap<String, Source>,
    chunks: Vec<SourceChunk>,
    /// Revoked sources are excluded from retrieval immediately.
    revoked: std::collections::BTreeSet<String>,
}

impl KnowledgeIndex {
    /// Create an index; the identity is baked in for its lifetime.
    pub fn new(identity: IndexIdentity) -> Self {
        KnowledgeIndex {
            identity,
            sources: BTreeMap::new(),
            chunks: Vec::new(),
            revoked: Default::default(),
        }
    }

    /// Attach vectors produced by another identity: hard error.
    pub fn attach_foreign(&self, other: &IndexIdentity) -> Result<(), IndexError> {
        self.identity
            .require_compatible(other)
            .map_err(IndexError::IdentityMismatch)
    }

    pub fn add_source(
        &mut self,
        source: Source,
        chunks: Vec<SourceChunk>,
    ) -> Result<(), IndexError> {
        for c in &chunks {
            if c.vector.len() != self.identity.embedding.dimension as usize {
                return Err(IndexError::DimensionMismatch {
                    expected: self.identity.embedding.dimension as usize,
                    got: c.vector.len(),
                });
            }
        }
        self.sources.insert(source.source_id.clone(), source);
        self.chunks.extend(chunks);
        Ok(())
    }

    /// Source removal excludes future retrieval immediately (03 §11) and
    /// marks any previously cited state as Removed.
    pub fn remove_source(&mut self, source_id: &str) -> Result<(), IndexError> {
        if self.sources.remove(source_id).is_none() {
            return Err(IndexError::SourceNotFound(source_id.into()));
        }
        self.chunks.retain(|c| c.source_id != source_id);
        self.revoked.insert(source_id.into());
        Ok(())
    }

    /// Update a source to a new content hash: previously cited versions
    /// report Changed, new citations bind the new hash.
    pub fn mark_source_changed(
        &mut self,
        source_id: &str,
        new_hash: &str,
    ) -> Result<(), IndexError> {
        let Some(s) = self.sources.get_mut(source_id) else {
            return Err(IndexError::SourceNotFound(source_id.into()));
        };
        s.content_hash = new_hash.into();
        s.indexed_at = Utc::now();
        Ok(())
    }

    /// Cosine search over live chunks with citation states resolved
    /// against the CURRENT registry.
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<Citation> {
        let mut scored: Vec<(f32, &SourceChunk)> = self
            .chunks
            .iter()
            .filter(|c| !self.revoked.contains(&c.source_id))
            .map(|c| (cosine(query, &c.vector), c))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(top_k)
            .filter_map(|(score, chunk)| {
                let s = self.sources.get(&chunk.source_id)?;
                Some(Citation {
                    source_id: chunk.source_id.clone(),
                    title: s.title.clone(),
                    chunk_id: chunk.chunk_id.clone(),
                    score,
                    state: SourceVersionState::Current,
                    content_hash: s.content_hash.clone(),
                })
            })
            .collect()
    }

    /// Like [`search`], but each citation carries its chunk text so
    /// grounded generation can quote evidence.
    pub fn search_with_text(&self, query: &[f32], top_k: usize) -> Vec<CitationWithText> {
        self.search(query, top_k)
            .into_iter()
            .map(|c| {
                let text = self
                    .chunks
                    .iter()
                    .find(|ch| ch.chunk_id == c.chunk_id)
                    .map(|ch| ch.text.clone())
                    .unwrap_or_default();
                CitationWithText { citation: c, text }
            })
            .collect()
    }

    /// Resolve a past citation against current registry state.
    pub fn citation_state(&self, source_id: &str, cited_hash: &str) -> SourceVersionState {
        if self.revoked.contains(source_id) {
            return SourceVersionState::Removed;
        }
        match self.sources.get(source_id) {
            None => SourceVersionState::Removed,
            Some(s) => {
                if s.content_hash == cited_hash {
                    SourceVersionState::Current
                } else {
                    SourceVersionState::Changed
                }
            }
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Chunker;
    use crate::identity::ChunkerConfig;
    use crate::identity::{embed_model_identity, ChunkerConfig as CC, Normalization};

    fn identity() -> IndexIdentity {
        IndexIdentity {
            embedding: embed_model_identity("test-hash", "v1", 8),
            chunker: "paragraph-window/1".into(),
            chunker_config: CC {
                target_graphemes: 100,
                overlap_graphemes: 10,
                respect_paragraphs: true,
            },
            tokenizer: "grapheme/1".into(),
            normalization: Normalization::Nfc,
            language_policy: "en,ar,mixed".into(),
            encryption_scope: "test".into(),
        }
    }

    fn vec8(seed: u32) -> Vec<f32> {
        (0..8)
            .map(|i| ((seed + i * 7) % 13) as f32 / 13.0)
            .collect()
    }

    #[test]
    fn foreign_identity_cannot_attach() {
        let idx = KnowledgeIndex::new(identity());
        let mut other = identity();
        other.embedding.dimension = 16;
        assert!(idx.attach_foreign(&other).is_err());
    }

    #[test]
    fn wrong_dimension_rejected() {
        let mut idx = KnowledgeIndex::new(identity());
        let err = idx
            .add_source(
                Source {
                    source_id: "s1".into(),
                    title: "Doc".into(),
                    content_hash: "h1".into(),
                    indexed_at: Utc::now(),
                },
                vec![SourceChunk {
                    source_id: "s1".into(),
                    chunk_id: "s1-0".into(),
                    ordinal: 0,
                    text: "text".into(),
                    vector: vec![0.0; 16],
                }],
            )
            .unwrap_err();
        assert!(matches!(
            err,
            IndexError::DimensionMismatch {
                expected: 8,
                got: 16
            }
        ));
    }

    #[test]
    fn removal_excludes_retrieval_immediately() {
        let mut idx = KnowledgeIndex::new(identity());
        idx.add_source(
            Source {
                source_id: "s1".into(),
                title: "Doc".into(),
                content_hash: "h1".into(),
                indexed_at: Utc::now(),
            },
            vec![SourceChunk {
                source_id: "s1".into(),
                chunk_id: "s1-0".into(),
                ordinal: 0,
                text: "The contract value is 5000".into(),
                vector: vec8(3),
            }],
        )
        .unwrap();
        assert!(!idx.search(&vec8(3), 3).is_empty());
        idx.remove_source("s1").unwrap();
        assert!(idx.search(&vec8(3), 3).is_empty());
        assert_eq!(idx.citation_state("s1", "h1"), SourceVersionState::Removed);
    }

    #[test]
    fn changed_source_reports_changed_citation() {
        let mut idx = KnowledgeIndex::new(identity());
        idx.add_source(
            Source {
                source_id: "s1".into(),
                title: "Doc".into(),
                content_hash: "old-hash".into(),
                indexed_at: Utc::now(),
            },
            vec![SourceChunk {
                source_id: "s1".into(),
                chunk_id: "s1-0".into(),
                ordinal: 0,
                text: "v1".into(),
                vector: vec8(1),
            }],
        )
        .unwrap();
        assert_eq!(
            idx.citation_state("s1", "old-hash"),
            SourceVersionState::Current
        );
        idx.mark_source_changed("s1", "new-hash").unwrap();
        assert_eq!(
            idx.citation_state("s1", "old-hash"),
            SourceVersionState::Changed
        );
        assert_eq!(
            idx.citation_state("s1", "new-hash"),
            SourceVersionState::Current
        );
    }

    #[test]
    fn search_ranks_by_similarity() {
        let mut idx = KnowledgeIndex::new(identity());
        for (sid, seed) in [("s-a", 3u32), ("s-b", 9)] {
            idx.add_source(
                Source {
                    source_id: sid.into(),
                    title: sid.into(),
                    content_hash: "h".into(),
                    indexed_at: Utc::now(),
                },
                vec![SourceChunk {
                    source_id: sid.into(),
                    chunk_id: format!("{sid}-0"),
                    ordinal: 0,
                    text: "t".into(),
                    vector: vec8(seed),
                }],
            )
            .unwrap();
        }
        let hits = idx.search(&vec8(3), 2);
        assert_eq!(hits[0].source_id, "s-a");
        let _ = Chunker::chunk(
            "x",
            &ChunkerConfig {
                target_graphemes: 10,
                overlap_graphemes: 0,
                respect_paragraphs: true,
            },
        );
    }
}
