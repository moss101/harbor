//! Index identity: the exact contract for when vectors may share an index.
//! Identity mismatch is a hard error — never a silent merge.

use harbor_canonical::JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Normalization {
    None,
    Nfc,
}

/// Identifier of the embedding function (model + version + dims).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedModelIdentity {
    /// e.g. "bge-m3" or "harbor-test/hash-v1".
    pub model: String,
    pub revision: String,
    pub dimension: u32,
}

impl EmbedModelIdentity {
    pub fn identity_string(&self) -> String {
        format!("{}/{}#dim{}", self.model, self.revision, self.dimension)
    }
}

pub fn embed_model_identity(model: &str, revision: &str, dimension: u32) -> EmbedModelIdentity {
    EmbedModelIdentity {
        model: model.into(),
        revision: revision.into(),
        dimension,
    }
}

/// An embedding is storable only when every component is a finite
/// number. NaN/Inf components poison cosine ranking silently; a
/// broken runtime must surface as an error at the embed boundary.
pub fn require_finite_vector(vector: &[f32]) -> Result<(), String> {
    if vector.is_empty() {
        return Err("empty embedding vector".into());
    }
    for (i, v) in vector.iter().enumerate() {
        if !v.is_finite() {
            return Err(format!("embedding component {i} is not finite"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkerConfig {
    /// Target chunk size in unicode graphemes.
    pub target_graphemes: usize,
    /// Overlap in graphemes.
    pub overlap_graphemes: usize,
    /// Split on paragraph boundaries first.
    pub respect_paragraphs: bool,
}

/// The complete identity of a logical index. Two identities are compatible
/// only when ALL fields are equal; the canonical hash is the index key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexIdentity {
    pub embedding: EmbedModelIdentity,
    pub chunker: String,
    pub chunker_config: ChunkerConfig,
    /// Tokenizer identity used for length accounting.
    pub tokenizer: String,
    pub normalization: Normalization,
    /// e.g. "en,ar,mixed" — language policy affects preprocessing.
    pub language_policy: String,
    /// Instruction-policy identity (`instructions::InstructionPolicy`);
    /// the query/document anchoring a model was trained for is
    /// preprocessing, so a change re-embeds instead of mixing.
    pub instruction: String,
    /// Encryption scope: workspace id (private) or "public".
    pub encryption_scope: String,
}

impl IndexIdentity {
    pub fn canonical_hash(&self) -> String {
        let v = JsonValue::object([
            (
                "embedding",
                JsonValue::str(self.embedding.identity_string()),
            ),
            ("chunker", JsonValue::str(self.chunker.clone())),
            (
                "chunker_config",
                JsonValue::object([
                    (
                        "target_graphemes",
                        JsonValue::int(self.chunker_config.target_graphemes as i64)
                            .unwrap_or(JsonValue::Null),
                    ),
                    (
                        "overlap_graphemes",
                        JsonValue::int(self.chunker_config.overlap_graphemes as i64)
                            .unwrap_or(JsonValue::Null),
                    ),
                    (
                        "respect_paragraphs",
                        JsonValue::Bool(self.chunker_config.respect_paragraphs),
                    ),
                ]),
            ),
            ("tokenizer", JsonValue::str(self.tokenizer.clone())),
            (
                "normalization",
                JsonValue::str(format!("{:?}", self.normalization)),
            ),
            (
                "language_policy",
                JsonValue::str(self.language_policy.clone()),
            ),
            ("instruction", JsonValue::str(self.instruction.clone())),
            (
                "encryption_scope",
                JsonValue::str(self.encryption_scope.clone()),
            ),
        ]);
        v.canonical_sha256().unwrap_or_default()
    }

    /// Hard compatibility check used before merging/attaching vectors.
    pub fn require_compatible(&self, other: &IndexIdentity) -> Result<(), String> {
        if self == other {
            Ok(())
        } else {
            Err(format!(
                "index identity mismatch: {} != {}",
                self.canonical_hash(),
                other.canonical_hash()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> IndexIdentity {
        IndexIdentity {
            embedding: embed_model_identity("test-hash", "v1", 8),
            chunker: "paragraph-window/1".into(),
            chunker_config: ChunkerConfig {
                target_graphemes: 800,
                overlap_graphemes: 80,
                respect_paragraphs: true,
            },
            tokenizer: "grapheme/1".into(),
            normalization: Normalization::Nfc,
            language_policy: "en,ar,mixed".into(),
            instruction: "none/1".into(),
            encryption_scope: "ws-1".into(),
        }
    }

    #[test]
    fn identical_identities_are_compatible() {
        assert!(identity().require_compatible(&identity()).is_ok());
    }

    #[test]
    fn any_component_change_breaks_compatibility() {
        let mut other = identity();
        other.embedding.dimension = 16;
        assert!(identity().require_compatible(&other).is_err());
        let mut other = identity();
        other.language_policy = "en".into();
        assert!(identity().require_compatible(&other).is_err());
        let mut other = identity();
        other.instruction = "e5/1".into();
        assert!(identity().require_compatible(&other).is_err());
        let mut other = identity();
        other.chunker_config.overlap_graphemes = 0;
        assert!(identity().require_compatible(&other).is_err());
        let mut other = identity();
        other.encryption_scope = "ws-2".into();
        assert!(identity().require_compatible(&other).is_err());
    }

    #[test]
    fn hash_is_stable() {
        let a = identity().canonical_hash();
        let b = identity().canonical_hash();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
