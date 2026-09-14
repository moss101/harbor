//! Harbor Knowledge: local-first indexing and citation-aware retrieval.
//!
//! Authority: `16_Index_and_Evaluation_Contract.md`, goal §12,
//! `03_Architecture_Contracts.md` §11.
//!
//! - Index identity incorporates embedding model identity, dimension,
//!   chunker, chunker configuration, tokenizer, normalization, language
//!   policy and encryption scope. Incompatible vectors are never combined
//!   into one logical index.
//! - Every answer preserves source identity and version; a citation
//!   indicates whether its source is current, changed or removed.
//! - Retrieval quality, citation support, insufficient-evidence
//!   abstention, contradictory evidence and prompt injection are
//!   evaluation-gated (EN / AR / mixed).

pub mod chunk;
pub mod corpus;
pub mod eval;
pub mod identity;
pub mod index;

pub use chunk::{Chunker, ChunkerConfig};
pub use corpus::{evaluation_corpus_sha256, run_pinned_evals};
pub use eval::{run_eval, EvalCase, EvalReport};
pub use identity::{embed_model_identity, IndexIdentity, Normalization};
pub use index::{Citation, KnowledgeIndex, Source, SourceChunk, SourceVersionState};
