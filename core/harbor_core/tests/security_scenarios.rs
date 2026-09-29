//! Security scenario SEC-018 (09_Security_Test_Matrix.csv) as an
//! executable control: crash-log exfiltration.
//! `security.sec_018` — diagnostics are content-free by default: document
//! content (long quoted strings) and user file paths never land in a
//! crash or error message; payload previews exist only behind explicit
//! opt-in, which redact() does not perform.

#[test]
fn sec_018_crash_logs_are_content_free_by_default() {
    // A crash-style message carrying document content in quotes and a
    // user file path.
    let message = "panic at node 'draft': model returned {\
        \"summary\": \"ACME Ltd will migrate the archive to local storage in Q4 \
        for 42000 USD, and the contract value is 5000 USD and ends 2026-12-31\"} \
        while processing /Users/amina/Documents/Q3 plan.docx";
    let redacted = harbor_core::diagnostics::redact(message);
    assert!(
        !redacted.contains("ACME"),
        "document content must not survive redaction: {redacted}"
    );
    assert!(
        !redacted.contains("42000"),
        "figures from user content must not survive: {redacted}"
    );
    assert!(
        !redacted.contains("Q3 plan.docx"),
        "user file names must not survive: {redacted}"
    );
    assert!(
        !redacted.contains("/Users/amina"),
        "user home paths must not survive: {redacted}"
    );
    // Build facts stay (they are not user data).
    assert!(
        redacted.contains("node 'draft'"),
        "non-content context stays"
    );
}

/// SEC-007 (tool argument injection): tool arguments are schema-validated
/// BEFORE the executor runs anything — injected extra fields, wrong
/// types and enum smuggles produce violations the executor treats as
/// refusals, never as best-effort execution.
#[test]
fn sec_007_injected_tool_arguments_fail_schema_validation() {
    let schema: serde_json::Value = serde_json::json!({
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "mode": {"enum": ["preview", "commit"]}
        },
        "required": ["path", "mode"],
        "additionalProperties": false
    });
    // Benign call validates.
    let ok = serde_json::json!({"path": "report.docx", "mode": "preview"});
    assert!(harbor_core::jsonschema::validate(&schema, &ok).is_empty());

    // Injected extra argument ("just a flag") is refused.
    let injected = serde_json::json!({
        "path": "report.docx", "mode": "preview",
        "overwrite": true, "skip_approval": true
    });
    assert!(
        !harbor_core::jsonschema::validate(&schema, &injected).is_empty(),
        "injected arguments must fail validation"
    );

    // Type confusion (mode as a number) is refused.
    let confused = serde_json::json!({"path": "report.docx", "mode": 7});
    assert!(!harbor_core::jsonschema::validate(&schema, &confused).is_empty());

    // Enum smuggle (a mode the tool never declared) is refused.
    let smuggled = serde_json::json!({"path": "report.docx", "mode": "force"});
    assert!(!harbor_core::jsonschema::validate(&schema, &smuggled).is_empty());
}

/// SEC-005 (skill prompt injection): the skill manifest's tool allowlist
/// is deny-by-default and enforced BELOW the model layer — a graph that
/// uses a tool outside its declared allowlist is refused at load, and a
/// prompt cannot widen it.
#[test]
fn sec_005_skill_allowlist_enforced_below_the_model() {
    use harbor_core::skills::{CapabilityCatalog, SkillManifest};
    // A REAL built-in skill (table-cleanup), loaded from its graph file.
    let graph_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/graphs/table-cleanup.json");
    let graph: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&graph_path).unwrap()).unwrap();
    let skills: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtin_skills.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let entry = skills
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "table-cleanup")
        .unwrap()
        .clone();
    // The real manifest, with the graph INLINED and the allowlist
    // narrowed to one tool the graph does not exclusively use.
    let mut manifest_json = entry.clone();
    manifest_json["graph"] = graph;
    manifest_json["tools"] = serde_json::json!(["table.inspect"]);
    let skill: SkillManifest = serde_json::from_value(manifest_json).unwrap();
    let catalog = CapabilityCatalog::new().with_tools(&[
        "artifact.read",
        "table.inspect",
        "workbook.build_operations",
        "workbook.verify_spec",
    ]);
    let err = skill.validate(&catalog).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("differ") || msg.contains("declared tools"),
        "a graph tool outside the declared allowlist must be refused: {msg}"
    );
    // And with the allowlist matching the graph, the same skill loads.
    let skill_tools: Vec<String> = skill.graph.as_ref().unwrap().tools().into_iter().collect();
    let mut ok_json = serde_json::to_value(&skill).unwrap();
    ok_json["tools"] = serde_json::json!(skill_tools);
    let ok: SkillManifest = serde_json::from_value(ok_json).unwrap();
    ok.validate(&catalog).unwrap();
}

/// SEC-020 (clipboard surprise): the clipboard tool reads ONLY text the
/// host explicitly attached to the run after a visible user action — a
/// read with no host attachment is a typed refusal.
#[test]
fn sec_020_clipboard_requires_visible_attachment() {
    use harbor_core::tools::{MemoryArtifacts, ToolContext, ToolRegistry};
    use std::collections::BTreeSet;
    use std::sync::atomic::AtomicBool;
    let registry = ToolRegistry::builtin();
    let arts = MemoryArtifacts::new();
    let cancel = AtomicBool::new(false);
    let allow: BTreeSet<String> = ["clipboard.read"].iter().map(|s| s.to_string()).collect();

    // No host-attached clipboard: refused.
    let empty = serde_json::json!({});
    let ctx = ToolContext::new(&arts, &empty, &cancel);
    let err = registry
        .call(&ctx, "clipboard.read", &serde_json::json!({}), &allow)
        .unwrap_err();
    assert!(
        matches!(err, harbor_core::tools::ToolError::Unavailable(_, _)),
        "unattached clipboard read must be refused: {err}"
    );

    // Attached (the visible action happened host-side): the text returns.
    let host = serde_json::json!({"clipboard": "user-pasted text"});
    let ctx = ToolContext::new(&arts, &host, &cancel);
    let out = registry
        .call(&ctx, "clipboard.read", &serde_json::json!({}), &allow)
        .unwrap();
    assert_eq!(out.output["text"], "user-pasted text");
}
