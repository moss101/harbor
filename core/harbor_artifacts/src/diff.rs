//! Artifact Diff: version-bound, human + technical change preview.
//! Every entry references a typed target and carries before/after values.

use harbor_canonical::JsonValue;

#[derive(Debug, Clone, PartialEq)]
pub struct DiffEntry {
    /// Machine-readable target id (cell ref, paragraph id, slide+shape id).
    pub target_id: String,
    /// Typed operation kind string.
    pub kind: String,
    /// Human-readable summary in user language.
    pub summary: String,
    pub before: Option<JsonValue>,
    pub after: Option<JsonValue>,
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactDiff {
    pub artifact_id: String,
    pub base_version_id: String,
    pub base_content_hash: String,
    pub proposed_output_hash: String,
    pub entries: Vec<DiffEntry>,
}

impl ArtifactDiff {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Canonical JSON for display/persistence (Harbor Lens / Harbor Sheet).
    pub fn to_canonical_value(&self) -> JsonValue {
        use JsonValue as V;
        let entries: Vec<JsonValue> = self
            .entries
            .iter()
            .map(|e| {
                V::object([
                    ("target_id", V::str(e.target_id.clone())),
                    ("kind", V::str(e.kind.clone())),
                    ("summary", V::str(e.summary.clone())),
                    (
                        "before",
                        e.before.clone().unwrap_or(JsonValue::Null),
                    ),
                    ("after", e.after.clone().unwrap_or(JsonValue::Null)),
                ])
            })
            .collect();
        V::object([
            ("artifact_id", V::str(self.artifact_id.clone())),
            ("base_version_id", V::str(self.base_version_id.clone())),
            ("base_content_hash", V::str(self.base_content_hash.clone())),
            ("proposed_output_hash", V::str(self.proposed_output_hash.clone())),
            ("entries", V::Array(entries)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_canonical_form() {
        let d = ArtifactDiff {
            artifact_id: "art-1".into(),
            base_version_id: "v1".into(),
            base_content_hash: "aa".repeat(32),
            proposed_output_hash: "bb".repeat(32),
            entries: vec![DiffEntry {
                target_id: "Sheet1!B3".into(),
                kind: "cell.set".into(),
                summary: "Set total revenue to 500".into(),
                before: Some(harbor_canonical::parse("450").unwrap()),
                after: Some(harbor_canonical::parse("500").unwrap()),
            }],
        };
        let v = d.to_canonical_value();
        assert_eq!(v.get("artifact_id").unwrap().as_str().unwrap(), "art-1");
        assert_eq!(v.get("entries").unwrap().as_array().unwrap().len(), 1);
    }
}
