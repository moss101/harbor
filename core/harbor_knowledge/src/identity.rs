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
