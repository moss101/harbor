//! Contract of the pinned llama.cpp JSON-Schema → grammar converter that
//! `harbor_core::graph` enforces at validation time so structured nodes
//! never fail at run time: patterns must be fully anchored and may not be
//! combined with length keywords.
fn ok(schema: &str) -> bool {
    llama_cpp_2::json_schema_to_grammar(schema).is_ok()
}

#[test]
fn converter_accepts_the_constructs_graphs_rely_on() {
    for schema in [
        r#"{"type":"object","properties":{"kind":{"const":"cell.set"}},"required":["kind"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^[A-Z]{1,3}[0-9]{1,7}$"}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^[a-f0-9]{64}$"}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^=[ -~]{1,400}$"}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":["string","null"],"maxLength":10}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"array","maxItems":50,"items":{"type":"string"}}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"string","minLength":1,"maxLength":600}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"integer","minimum":0}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"enum":["en","ar","fr"]}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"boolean"}},"required":["a"]}"#,
    ] {
        assert!(ok(schema), "converter rejected {schema}");
    }
}

#[test]
fn converter_rejects_unanchored_patterns_and_pattern_with_length() {
    for schema in [
        r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^=.+"}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^x","maxLength":300}},"required":["a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^=[ -~]+"}},"required":["a"]}"#,
    ] {
        assert!(!ok(schema), "converter unexpectedly accepted {schema}");
    }
}

#[test]
fn every_builtin_structured_node_schema_converts() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../harbor_core/src/graphs");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let g: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for node in g["nodes"].as_array().unwrap() {
            if node["kind"] == "model.structured" {
                let schema = serde_json::to_string(&node["output_schema"]).unwrap();
                assert!(
                    ok(&schema),
                    "{}: node {} schema is not grammar-convertible",
                    path.display(),
                    node["id"]
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 3,
        "expected structured nodes across the built-in graphs, checked {checked}"
    );
}
