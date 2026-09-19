//! RFC 6901 JSON pointers over the run blackboard (`serde_json::Value`).
//!
//! Graph nodes read state through pointers (`{"$state": "/input/x"}`,
//! `context[].from`, predicates) and write through their `out` pointer.
//! Reads support one `*` segment that fans out over an array (used by
//! eval assertions such as "every owner at /minutes/actions/*/owner").
//! Writes create intermediate objects and reject anything that would
//! replace the reserved read-only roots (`/input`, `/host`).

use serde_json::Value;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PointerError {
    #[error("pointer must be empty or start with '/': {0}")]
    Malformed(String),
    #[error("pointer {0} writes into a read-only root")]
    ReadOnly(String),
    #[error("pointer {0} cannot be written: {1}")]
    Unwritable(String, String),
}

pub const READ_ONLY_ROOTS: [&str; 2] = ["input", "host"];

fn unescape(seg: &str) -> String {
    seg.replace("~1", "/").replace("~0", "~")
}

/// Split a pointer into unescaped segments. `""` is the root (no segments).
pub fn segments(pointer: &str) -> Result<Vec<String>, PointerError> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(PointerError::Malformed(pointer.into()));
    }
    Ok(pointer[1..].split('/').map(unescape).collect())
}

/// Resolve a pointer to at most one value (no wildcard fan-out).
pub fn get<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    let segs = segments(pointer).ok()?;
    let mut cur = root;
    for s in segs {
        cur = match cur {
            Value::Object(m) => m.get(&s)?,
            Value::Array(a) => a.get(s.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Resolve a pointer, fanning out over arrays at every `*` segment.
/// A pointer without `*` yields zero or one value.
pub fn get_all<'a>(root: &'a Value, pointer: &str) -> Vec<&'a Value> {
    let Ok(segs) = segments(pointer) else {
        return Vec::new();
    };
    let mut frontier: Vec<&Value> = vec![root];
    for s in segs {
        let mut next = Vec::new();
        for v in frontier {
            match (v, s.as_str()) {
                (Value::Array(a), "*") => next.extend(a.iter()),
                (Value::Object(m), key) => {
                    if let Some(x) = m.get(key) {
                        next.push(x);
                    }
                }
                (Value::Array(a), idx) => {
                    if let Some(x) = idx.parse::<usize>().ok().and_then(|i| a.get(i)) {
                        next.push(x);
                    }
                }
                _ => {}
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    frontier
}

/// Write `value` at `pointer`, creating intermediate objects. Array
/// targets accept an existing index or `-` (append). The root and the
/// reserved read-only roots cannot be written by graph nodes.
pub fn set(root: &mut Value, pointer: &str, value: Value) -> Result<(), PointerError> {
    let segs = segments(pointer)?;
    if segs.is_empty() {
        return Err(PointerError::Unwritable(
            pointer.into(),
            "root is not writable".into(),
        ));
    }
    if READ_ONLY_ROOTS.contains(&segs[0].as_str()) {
        return Err(PointerError::ReadOnly(pointer.into()));
    }
    set_unchecked(root, pointer, value)
}

/// Like [`set`] but without the read-only root guard (executor-internal
/// binding of `/input` and `/host`).
pub fn set_unchecked(root: &mut Value, pointer: &str, value: Value) -> Result<(), PointerError> {
    let segs = segments(pointer)?;
    if segs.is_empty() {
        *root = value;
        return Ok(());
    }
    if !root.is_object() {
        *root = Value::Object(Default::default());
    }
    let mut cur = root;
    for (i, s) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        match cur {
            Value::Object(m) => {
                if last {
                    m.insert(s.clone(), value);
                    return Ok(());
                }
                cur = m
                    .entry(s.clone())
                    .or_insert_with(|| Value::Object(Default::default()));
                if !cur.is_object() && !cur.is_array() {
                    *cur = Value::Object(Default::default());
                }
            }
            Value::Array(a) => {
                let idx = if s == "-" {
                    a.len()
                } else {
                    s.parse::<usize>().map_err(|_| {
                        PointerError::Unwritable(
                            pointer.into(),
                            format!("'{s}' is not an array index"),
                        )
                    })?
                };
                if idx > a.len() {
                    return Err(PointerError::Unwritable(
                        pointer.into(),
                        format!("index {idx} out of range"),
                    ));
                }
                if last {
                    if idx == a.len() {
                        a.push(value);
                    } else {
                        a[idx] = value;
                    }
                    return Ok(());
                }
                if idx == a.len() {
                    a.push(Value::Object(Default::default()));
                }
                cur = &mut a[idx];
            }
            _ => {
                return Err(PointerError::Unwritable(
                    pointer.into(),
                    "path crosses a scalar".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Deterministic hash of a JSON value: SHA-256 over a sorted-key
/// serialization. Used for blackboard/state hashes and node io hashes;
/// unlike the cross-tool canonical form it accepts floats, which cell
/// values need.
pub fn stable_hash(value: &Value) -> String {
    let mut buf = Vec::new();
    write_sorted(value, &mut buf);
    harbor_canonical::sha256_hex(&buf)
}

fn write_sorted(v: &Value, buf: &mut Vec<u8>) {
    match v {
        Value::Object(m) => {
            buf.push(b'{');
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                buf.extend_from_slice(serde_json::to_string(k).unwrap_or_default().as_bytes());
                buf.push(b':');
                write_sorted(&m[*k], buf);
            }
            buf.push(b'}');
        }
        Value::Array(a) => {
            buf.push(b'[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                write_sorted(x, buf);
            }
            buf.push(b']');
        }
        other => buf.extend_from_slice(serde_json::to_string(other).unwrap_or_default().as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_set_and_wildcards() {
        let mut v = json!({"input": {"a": 1}, "items": [{"owner": "x"}, {"owner": null}]});
        assert_eq!(get(&v, "/input/a"), Some(&json!(1)));
        assert_eq!(get(&v, "/items/1/owner"), Some(&json!(null)));
        assert_eq!(get(&v, ""), Some(&v));
        let owners: Vec<&Value> = get_all(&v, "/items/*/owner");
        assert_eq!(owners, vec![&json!("x"), &json!(null)]);
        set(&mut v, "/out/nested/x", json!(2)).unwrap();
        assert_eq!(get(&v, "/out/nested/x"), Some(&json!(2)));
        set(&mut v, "/items/-", json!({"owner": "z"})).unwrap();
        assert_eq!(get_all(&v, "/items/*/owner").len(), 3);
        assert_eq!(
            set(&mut v, "/input/a", json!(9)),
            Err(PointerError::ReadOnly("/input/a".into()))
        );
        assert!(matches!(
            set(&mut v, "", json!(1)),
            Err(PointerError::Unwritable(_, _))
        ));
        assert!(matches!(
            set(&mut v, "bad", json!(1)),
            Err(PointerError::Malformed(_))
        ));
        assert!(segments("/a~1b/c~0d").unwrap() == vec!["a/b", "c~d"]);
    }

    #[test]
    fn stable_hash_is_key_order_independent() {
        let a = json!({"b": [1, 2.5, {"z": null, "y": "s"}], "a": true});
        let b = json!({"a": true, "b": [1, 2.5, {"y": "s", "z": null}]});
        assert_eq!(stable_hash(&a), stable_hash(&b));
        assert_ne!(stable_hash(&a), stable_hash(&json!({"a": false})));
    }
}
