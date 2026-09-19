//! Minimal JSON Schema validator over `serde_json::Value`.
//!
//! Harbor validates tool arguments, graph inputs and model outputs against
//! schemas that authors write inline in skill graphs. Only the subset the
//! graph contract promises is enforced (`schemas/graph.schema.json`
//! `jsonSchemaObject`): `type`, `properties`, `required`,
//! `additionalProperties`, `items`, `enum`, `const`, `minimum`, `maximum`,
//! `minLength`, `maxLength`, `minItems`, `maxItems`, `pattern`, `anyOf`,
//! `oneOf`, `allOf`, `not` and `$ref` to a local `#/$defs/...` entry. Unknown
//! keywords are ignored, matching JSON Schema semantics. The validator
//! never panics on malformed schemas; a malformed keyword is reported as a
//! schema error at the offending path.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// JSON pointer to the offending instance location.
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            if self.path.is_empty() {
                "/"
            } else {
                &self.path
            },
            self.message
        )
    }
}

/// Validate `instance` against `schema`. Returns every violation found
/// (never short-circuits) so callers can show authors the complete list.
pub fn validate(schema: &Value, instance: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    validate_at(schema, schema, instance, "", &mut out, 0);
    out
}

pub fn is_valid(schema: &Value, instance: &Value) -> bool {
    validate(schema, instance).is_empty()
}

const MAX_DEPTH: usize = 64;

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn type_matches(want: &str, v: &Value) -> bool {
    match want {
        "number" => matches!(v, Value::Number(_)),
        "integer" => match v {
            Value::Number(n) => {
                n.is_i64() || n.is_u64() || n.as_f64().map(|f| f.fract() == 0.0).unwrap_or(false)
            }
            _ => false,
        },
        other => type_name(v) == other,
    }
}

fn resolve_ref<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let path = reference.strip_prefix('#')?;
    let mut cur = root;
    for seg in path.split('/').skip(1) {
        let seg = seg.replace("~1", "/").replace("~0", "~");
        cur = cur.get(&seg)?;
    }
    Some(cur)
}

fn validate_at(
    root: &Value,
    schema: &Value,
    inst: &Value,
    path: &str,
    out: &mut Vec<Violation>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        out.push(Violation {
            path: path.into(),
            message: "schema nesting too deep".into(),
        });
        return;
    }
    let schema = match schema {
        Value::Bool(true) => return,
        Value::Bool(false) => {
            out.push(Violation {
                path: path.into(),
                message: "schema forbids any value".into(),
            });
            return;
        }
        Value::Object(_) => schema,
        _ => {
            out.push(Violation {
                path: path.into(),
                message: "malformed schema (not an object)".into(),
            });
            return;
        }
    };
    if let Some(Value::String(r)) = schema.get("$ref") {
        match resolve_ref(root, r) {
            Some(target) => validate_at(root, target, inst, path, out, depth + 1),
            None => out.push(Violation {
                path: path.into(),
                message: format!("unresolvable $ref {r}"),
            }),
        }
        return;
    }
    // type
    if let Some(t) = schema.get("type") {
        let ok = match t {
            Value::String(s) => type_matches(s, inst),
            Value::Array(ts) => ts
                .iter()
                .any(|t| t.as_str().map(|s| type_matches(s, inst)).unwrap_or(false)),
            _ => false,
        };
        if !ok {
            out.push(Violation {
                path: path.into(),
                message: format!("expected type {}, got {}", t, type_name(inst)),
            });
            // Further keyword checks on a wrong type are noise.
            return;
        }
    }
    if let Some(c) = schema.get("const") {
        if c != inst {
            out.push(Violation {
                path: path.into(),
                message: format!("must equal {c}"),
            });
        }
    }
    if let Some(Value::Array(vals)) = schema.get("enum") {
        if !vals.iter().any(|v| v == inst) {
            out.push(Violation {
                path: path.into(),
                message: "value not in enum".into(),
            });
        }
    }
    match inst {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
                if f < min {
                    out.push(Violation {
                        path: path.into(),
                        message: format!("{f} < minimum {min}"),
                    });
                }
            }
            if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
                if f > max {
                    out.push(Violation {
                        path: path.into(),
                        message: format!("{f} > maximum {max}"),
                    });
                }
            }
        }
        Value::String(s) => {
            let len = s.chars().count();
            if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
                if (len as u64) < min {
                    out.push(Violation {
                        path: path.into(),
                        message: format!("length {len} < minLength {min}"),
                    });
                }
            }
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
                if (len as u64) > max {
                    out.push(Violation {
                        path: path.into(),
                        message: format!("length {len} > maxLength {max}"),
                    });
                }
            }
            if let Some(Value::String(p)) = schema.get("pattern") {
                match regex::Regex::new(p) {
                    Ok(re) => {
                        if !re.is_match(s) {
                            out.push(Violation {
                                path: path.into(),
                                message: format!("does not match pattern {p}"),
                            });
                        }
                    }
                    Err(e) => out.push(Violation {
                        path: path.into(),
                        message: format!("malformed pattern: {e}"),
                    }),
                }
            }
        }
        Value::Array(items) => {
            if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
                if (items.len() as u64) < min {
                    out.push(Violation {
                        path: path.into(),
                        message: format!("{} items < minItems {min}", items.len()),
                    });
                }
            }
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
                if (items.len() as u64) > max {
                    out.push(Violation {
                        path: path.into(),
                        message: format!("{} items > maxItems {max}", items.len()),
                    });
                }
            }
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    validate_at(
                        root,
                        item_schema,
                        item,
                        &format!("{path}/{i}"),
                        out,
                        depth + 1,
                    );
                }
            }
        }
        Value::Object(map) => {
            let props = schema.get("properties").and_then(Value::as_object);
            if let Some(Value::Array(req)) = schema.get("required") {
                for r in req {
                    if let Some(name) = r.as_str() {
                        if !map.contains_key(name) {
                            out.push(Violation {
                                path: path.into(),
                                message: format!("missing required property '{name}'"),
                            });
                        }
                    }
                }
            }
            let additional = schema.get("additionalProperties");
            for (k, v) in map {
                let child = format!("{path}/{}", k.replace('~', "~0").replace('/', "~1"));
                match props.and_then(|p| p.get(k)) {
                    Some(ps) => validate_at(root, ps, v, &child, out, depth + 1),
                    None => match additional {
                        Some(Value::Bool(false)) => out.push(Violation {
                            path: child,
                            message: format!("unexpected property '{k}'"),
                        }),
                        Some(s @ Value::Object(_)) => {
                            validate_at(root, s, v, &child, out, depth + 1)
                        }
                        _ => {}
                    },
                }
            }
        }
        _ => {}
    }
    if let Some(Value::Array(all)) = schema.get("allOf") {
        for s in all {
            validate_at(root, s, inst, path, out, depth + 1);
        }
    }
    if let Some(Value::Array(any)) = schema.get("anyOf") {
        let ok = any.iter().any(|s| {
            let mut tmp = Vec::new();
            validate_at(root, s, inst, path, &mut tmp, depth + 1);
            tmp.is_empty()
        });
        if !ok {
            out.push(Violation {
                path: path.into(),
                message: "matches none of anyOf".into(),
            });
        }
    }
    if let Some(Value::Array(one)) = schema.get("oneOf") {
        let n = one
            .iter()
            .filter(|s| {
                let mut tmp = Vec::new();
                validate_at(root, s, inst, path, &mut tmp, depth + 1);
                tmp.is_empty()
            })
            .count();
        if n != 1 {
            out.push(Violation {
                path: path.into(),
                message: format!("matches {n} of oneOf, expected exactly 1"),
            });
        }
    }
    if let Some(not) = schema.get("not") {
        let mut tmp = Vec::new();
        validate_at(root, not, inst, path, &mut tmp, depth + 1);
        if tmp.is_empty() {
            out.push(Violation {
                path: path.into(),
                message: "matches forbidden schema (not)".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_rules() {
        let s = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1},
                "n": {"type": "integer", "minimum": 0, "maximum": 3},
                "tags": {"type": "array", "items": {"type": "string"}, "maxItems": 2},
                "kind": {"enum": ["a", "b"]}
            },
            "required": ["name", "kind"],
            "additionalProperties": false
        });
        assert!(is_valid(
            &s,
            &json!({"name": "x", "kind": "a", "n": 3, "tags": ["t"]})
        ));
        let v = validate(
            &s,
            &json!({"name": "", "kind": "c", "n": 4, "tags": ["a", "b", "c"], "extra": 1}),
        );
        let msgs: Vec<String> = v.iter().map(|x| x.to_string()).collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("/name") && m.contains("minLength")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("/kind") && m.contains("enum")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("/n") && m.contains("maximum")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("/tags") && m.contains("maxItems")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("/extra") && m.contains("unexpected")),
            "{msgs:?}"
        );
    }

    #[test]
    fn nullable_types_refs_and_combinators() {
        let s = json!({
            "$defs": {"owner": {"type": ["string", "null"], "maxLength": 5}},
            "type": "object",
            "properties": {
                "owner": {"$ref": "#/$defs/owner"},
                "x": {"oneOf": [{"type": "integer"}, {"type": "string", "pattern": "^[a-z]+$"}]}
            }
        });
        assert!(is_valid(&s, &json!({"owner": null, "x": 1})));
        assert!(is_valid(&s, &json!({"owner": "bob", "x": "abc"})));
        assert!(!is_valid(&s, &json!({"owner": "toolong", "x": "ABC"})));
        assert!(!is_valid(&json!({"not": {"type": "null"}}), &json!(null)));
        assert!(!is_valid(&json!({"$ref": "#/$defs/missing"}), &json!(1)));
    }

    #[test]
    fn integer_vs_number() {
        assert!(is_valid(&json!({"type": "integer"}), &json!(2)));
        assert!(!is_valid(&json!({"type": "integer"}), &json!(2.5)));
        assert!(is_valid(&json!({"type": "number"}), &json!(2.5)));
        assert!(is_valid(&json!({"const": "x"}), &json!("x")));
        assert!(!is_valid(&json!({"const": "x"}), &json!("y")));
    }
}
