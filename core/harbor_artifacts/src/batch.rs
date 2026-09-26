//! Operation batches (`harbor.artifact_batch/v3`).
//!
//! Duplicate op IDs and incompatible operation/precondition targets are
//! rejected; batches are all-or-nothing at publication.

use harbor_canonical::JsonValue;

/// SHA-256 of zero bytes: the immutable empty base every newly created
/// artifact starts from (02 contract, "Artifact batches and safe save").
/// A creation batch binds this as its `base_content_hash`, and every
/// precondition in it names a target that does not exist yet, so each
/// one expects this hash too.
pub const EMPTY_CONTENT_HASH: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// DOCX: replace the text of a paragraph run set identified by
    /// paragraph index.
    TextReplace,
    /// XLSX: set one cell (value or formula).
    CellSet,
    /// PPTX: set the text of a slide placeholder.
    SlideTextSet,
    /// PPTX: append a slide.
    SlideAppend,
    /// XLSX: add a sheet (`sheet_id` names it inside the batch).
    SheetInsert,
    /// PPTX: add a slide at an index.
    SlideInsert,
    /// PPTX: set the typed elements of a slide (title, bullets, notes,
    /// chart) — the slide's whole content, not a patch.
    SlideUpdate,
    /// Any format: set a document property (title) or a presentation
    /// property the IR carries (column number formats).
    MetadataSet,
    /// DOCX: add a styled paragraph (title, heading, body, list item).
    BlockInsert,
}

impl OpKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpKind::TextReplace => "text.replace",
            OpKind::CellSet => "cell.set",
            OpKind::SlideTextSet => "slide.text_set",
            OpKind::SlideAppend => "slide.append",
            OpKind::SheetInsert => "sheet.insert",
            OpKind::SlideInsert => "slide.insert",
            OpKind::SlideUpdate => "slide.update",
            OpKind::MetadataSet => "metadata.set",
            OpKind::BlockInsert => "block.insert",
        }
    }

    pub fn parse(kind: &str) -> Option<OpKind> {
        Some(match kind {
            "text.replace" => OpKind::TextReplace,
            "cell.set" => OpKind::CellSet,
            "slide.text_set" => OpKind::SlideTextSet,
            "slide.append" => OpKind::SlideAppend,
            "sheet.insert" => OpKind::SheetInsert,
            "slide.insert" => OpKind::SlideInsert,
            "slide.update" => OpKind::SlideUpdate,
            "metadata.set" => OpKind::MetadataSet,
            "block.insert" => OpKind::BlockInsert,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Precondition {
    /// Stable target identity (paragraph id, cell ref, slide id).
    pub target_id: String,
    /// Expected content hash of the target before the op applies.
    pub expected_content_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub op_id: String,
    pub kind: OpKind,
    pub precondition: Precondition,
    /// Typed arguments as canonical JSON (validated per kind at apply time).
    pub args: JsonValue,
}

#[derive(Debug, Clone)]
pub struct ArtifactBatch {
    pub batch_id: String,
    pub artifact_id: String,
    pub base_version_id: String,
    pub base_content_hash: String,
    pub operations: Vec<Operation>,
}

#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error("duplicate operation id: {0}")]
    DuplicateOpId(String),
    #[error("invalid hash on op {0}")]
    InvalidHash(String),
    #[error("op {0}: {1}")]
    InvalidArgs(String, String),
    #[error("empty batch")]
    Empty,
}

impl ArtifactBatch {
    /// Validate structural batch invariants.
    pub fn validate(&self) -> Result<(), BatchError> {
        if self.operations.is_empty() {
            return Err(BatchError::Empty);
        }
        let mut seen = std::collections::BTreeSet::new();
        for op in &self.operations {
            if !seen.insert(op.op_id.as_str()) {
                return Err(BatchError::DuplicateOpId(op.op_id.clone()));
            }
            if harbor_canonical::sha256_hex(b"") != op.precondition.expected_content_hash
                && op.precondition.expected_content_hash.len() != 64
            {
                return Err(BatchError::InvalidHash(op.op_id.clone()));
            }
        }
        Ok(())
    }

    /// Serialize to the schema-shaped canonical JSON value.
    pub fn to_canonical_value(&self) -> JsonValue {
        use JsonValue as V;
        let ops: Vec<JsonValue> = self
            .operations
            .iter()
            .map(|op| {
                V::object([
                    ("op_id", V::str(op.op_id.clone())),
                    ("kind", V::str(op.kind.as_str())),
                    (
                        "precondition",
                        V::object([
                            ("target_id", V::str(op.precondition.target_id.clone())),
                            (
                                "expected_content_hash",
                                V::str(op.precondition.expected_content_hash.clone()),
                            ),
                        ]),
                    ),
                    ("args", op.args.clone()),
                ])
            })
            .collect();
        V::object([
            ("schema", V::str("harbor.artifact_batch/v3")),
            ("batch_id", V::str(self.batch_id.clone())),
            ("artifact_id", V::str(self.artifact_id.clone())),
            ("base_version_id", V::str(self.base_version_id.clone())),
            ("base_content_hash", V::str(self.base_content_hash.clone())),
            ("operations", V::Array(ops)),
        ])
    }

    pub fn canonical_hash(&self) -> String {
        self.to_canonical_value()
            .canonical_sha256()
            .unwrap_or_else(|_| String::new())
    }

    /// A batch that creates a new artifact: it is bound to the empty base.
    pub fn is_creation(&self) -> bool {
        self.base_content_hash == EMPTY_CONTENT_HASH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: &str, target: &str) -> Operation {
        Operation {
            op_id: id.into(),
            kind: OpKind::CellSet,
            precondition: Precondition {
                target_id: target.into(),
                expected_content_hash: "ab".repeat(32),
            },
            args: harbor_canonical::parse(r#"{"sheet":"Sheet1","row":1,"col":1,"value":{"n":42}}"#)
                .unwrap(),
        }
    }

    #[test]
    fn duplicate_op_ids_rejected() {
        let b = ArtifactBatch {
            batch_id: "batch-1".into(),
            artifact_id: "art-1".into(),
            base_version_id: "v1".into(),
            base_content_hash: "cd".repeat(32),
            operations: vec![op("op-1", "Sheet1!A1"), op("op-1", "Sheet1!A2")],
        };
        assert!(matches!(b.validate(), Err(BatchError::DuplicateOpId(id)) if id == "op-1"));
    }

    #[test]
    fn valid_batch_passes_and_hashes() {
        let b = ArtifactBatch {
            batch_id: "batch-1".into(),
            artifact_id: "art-1".into(),
            base_version_id: "v1".into(),
            base_content_hash: "cd".repeat(32),
            operations: vec![op("op-1", "Sheet1!A1"), op("op-2", "Sheet1!A2")],
        };
        b.validate().unwrap();
        assert_eq!(b.canonical_hash().len(), 64);
        assert_eq!(
            b.to_canonical_value()
                .get("schema")
                .unwrap()
                .as_str()
                .unwrap(),
            "harbor.artifact_batch/v3"
        );
    }

    #[test]
    fn empty_content_hash_is_the_hash_of_zero_bytes() {
        assert_eq!(EMPTY_CONTENT_HASH, harbor_canonical::sha256_hex(b""));
        for kind in [
            OpKind::TextReplace,
            OpKind::CellSet,
            OpKind::SheetInsert,
            OpKind::SlideInsert,
            OpKind::SlideUpdate,
            OpKind::MetadataSet,
            OpKind::BlockInsert,
        ] {
            assert_eq!(OpKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(OpKind::parse("file.delete"), None);
    }

    #[test]
    fn empty_batch_rejected() {
        let b = ArtifactBatch {
            batch_id: "batch-1".into(),
            artifact_id: "art-1".into(),
            base_version_id: "v1".into(),
            base_content_hash: "cd".repeat(32),
            operations: vec![],
        };
        assert!(matches!(b.validate(), Err(BatchError::Empty)));
    }
}
