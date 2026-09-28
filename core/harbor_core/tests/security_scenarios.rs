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
