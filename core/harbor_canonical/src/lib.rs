//! harbor.canonical_json/v1 — Harbor's bounded canonical JSON format.
//!
//! Semantics are normative per `02_Runtime_Effect_and_Artifact_Contracts.md`
//! and must remain byte-compatible with the reference encoder
//! `tools/contracts.py::canonical`:
//!
//! - UTF-8 JSON, object keys sorted by Unicode scalar value.
//! - No insignificant whitespace; lowercase JSON literals.
//! - No Unicode normalization; string values preserved as-is.
//! - Integers restricted to the exact signed 53-bit range.
//! - Floating-point JSON values are forbidden; decimals travel as strings.
//! - Duplicate keys, non-finite values and unpaired surrogates are rejected
//!   on parse.
//! - Strings escape `"` and `\` and control characters only; all other
//!   Unicode is emitted directly.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Largest magnitude accepted for canonical integers (2^53 - 1).
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error("integer outside exact signed 53-bit range: {0}")]
    IntegerOutOfRange(i64),
    #[error("floating-point values are prohibited in canonical JSON")]
    FloatProhibited,
    #[error("duplicate JSON key: {0}")]
    DuplicateKey(String),
    #[error("invalid JSON: {0}")]
    Syntax(#[from] serde_json::Error),
    #[error("non-string object key")]
    NonStringKey,
}

/// A JSON value that can only hold canonical-representable data.
///
/// Object keys are held in a `BTreeMap`, which orders `String`s by their
/// bytes — identical to ordering by Unicode scalar values for Rust's
/// UTF-8 strings — matching Python `sort_keys=True` byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    /// Construct an integer, rejecting values outside ±(2^53-1).
    pub fn int(v: i64) -> Result<Self, CanonicalError> {
        if v.abs() > MAX_SAFE_INTEGER {
            Err(CanonicalError::IntegerOutOfRange(v))
        } else {
            Ok(JsonValue::Int(v))
        }
    }

    pub fn str(v: impl Into<String>) -> Self {
        JsonValue::Str(v.into())
    }

    pub fn object(pairs: impl IntoIterator<Item = (impl Into<String>, JsonValue)>) -> Self {
        JsonValue::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            JsonValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, JsonValue::Null)
    }

    /// Encode to canonical bytes.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        let mut out = Vec::new();
        self.write_canonical(&mut out)?;
        Ok(out)
    }

    /// SHA-256 of the canonical encoding, lowercase hex.
    pub fn canonical_sha256(&self) -> Result<String, CanonicalError> {
        Ok(hex_hex(Sha256::digest(
            self.to_canonical_bytes()?.as_slice(),
        ).as_slice()))
    }

    fn check(&self) -> Result<(), CanonicalError> {
        match self {
            JsonValue::Int(v) => {
                if v.abs() > MAX_SAFE_INTEGER {
                    Err(CanonicalError::IntegerOutOfRange(*v))
                } else {
                    Ok(())
                }
            }
            JsonValue::Array(items) => items.iter().try_for_each(|v| v.check()),
            JsonValue::Object(map) => map.values().try_for_each(|v| v.check()),
            _ => Ok(()),
        }
    }

    fn write_canonical(&self, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
        self.check()?;
        match self {
            JsonValue::Null => out.extend_from_slice(b"null"),
            JsonValue::Bool(true) => out.extend_from_slice(b"true"),
            JsonValue::Bool(false) => out.extend_from_slice(b"false"),
            JsonValue::Int(v) => out.extend_from_slice(v.to_string().as_bytes()),
            JsonValue::Str(s) => write_escaped_string(out, s),
            JsonValue::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    item.write_canonical(out)?;
                }
                out.push(b']');
            }
            JsonValue::Object(map) => {
                out.push(b'{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    write_escaped_string(out, k);
                    out.push(b':');
                    v.write_canonical(out)?;
                }
                out.push(b'}');
            }
        }
        Ok(())
    }
}

/// Escape a string exactly like Python `json.dumps(..., ensure_ascii=False)`:
/// `"` -> `\"`, `\` -> `\\`, control chars via `\b \t \n \f \r` or
/// `\u00xx` with lowercase hex; all other Unicode passes through as UTF-8.
fn write_escaped_string(out: &mut Vec<u8>, s: &str) {
    out.push(b'"');
    for c in s.chars() {
        match c {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\u{08}' => out.extend_from_slice(b"\\b"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\u{0c}' => out.extend_from_slice(b"\\f"),
            '\r' => out.extend_from_slice(b"\\r"),
            c if (c as u32) < 0x20 => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let n = c as u32;
                out.extend_from_slice(b"\\u00");
                out.push(HEX[((n >> 4) & 0xf) as usize]);
                out.push(HEX[(n & 0xf) as usize]);
            }
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

fn hex_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// Parse JSON text into a canonical [`JsonValue`], rejecting duplicate keys,
/// floats, non-finite literals and out-of-range integers at the deserializer
/// boundary. This is the Rust gate equivalent of `contracts.py::load`.
pub fn parse(text: &str) -> Result<JsonValue, CanonicalError> {
    serde_json::from_str(text).map_err(CanonicalError::from)
}

impl<'de> serde::Deserialize<'de> for JsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, SeqAccess, Visitor};
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = JsonValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a canonical JSON value (no floats, no duplicate keys)")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<JsonValue, E> {
                Ok(JsonValue::Bool(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<JsonValue, E> {
                JsonValue::int(v).map_err(de::Error::custom)
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<JsonValue, E> {
                if v <= MAX_SAFE_INTEGER as u64 {
                    Ok(JsonValue::Int(v as i64))
                } else {
                    Err(de::Error::custom(CanonicalError::IntegerOutOfRange(i64::MAX)))
                }
            }
            fn visit_f64<E: de::Error>(self, _v: f64) -> Result<JsonValue, E> {
                Err(de::Error::custom(CanonicalError::FloatProhibited))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<JsonValue, E> {
                Ok(JsonValue::Str(v.to_owned()))
            }
            fn visit_unit<E>(self) -> Result<JsonValue, E> {
                Ok(JsonValue::Null)
            }
            fn visit_none<E>(self) -> Result<JsonValue, E> {
                Ok(JsonValue::Null)
            }
            fn visit_some<D2: serde::Deserializer<'de>>(
                self,
                d: D2,
            ) -> Result<JsonValue, D2::Error> {
                d.deserialize_any(JsonVisitor)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<JsonValue, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = seq.next_element::<JsonValue>()? {
                    out.push(v);
                }
                Ok(JsonValue::Array(out))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JsonValue, A::Error> {
                let mut out = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(de::Error::custom(CanonicalError::DuplicateKey(key)));
                    }
                    let value = map.next_value::<JsonValue>()?;
                    out.insert(key, value);
                }
                Ok(JsonValue::Object(out))
            }
        }
        deserializer.deserialize_any(JsonVisitor)
    }
}

impl serde::Serialize for JsonValue {
    /// Serializes in canonical form (sorted keys, no floats). This makes
    /// `serde_json::to_value(json_value)` produce equivalent JSON.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        use serde::ser::SerializeSeq as _;
        match self {
            JsonValue::Null => serializer.serialize_none(),
            JsonValue::Bool(b) => serializer.serialize_bool(*b),
            JsonValue::Int(v) => serializer.serialize_i64(*v),
            JsonValue::Str(s) => serializer.serialize_str(s),
            JsonValue::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            JsonValue::Object(map) => {
                let mut m = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map {
                    m.serialize_entry(k, v)?;
                }
                m.end()
            }
        }
    }
}

/// Convert an already-parsed `serde_json::Value` into a canonical value.
///
/// Note: duplicate keys are unrepresentable at this type, so duplicate
/// detection only happens in [`parse`]. Lexical `-0` is deserialized by
/// serde_json as a float and is therefore rejected (the reference encoder
/// would emit `0`); this is a strictly-saved subset of the reference.
pub fn convert(raw: serde_json::Value) -> Result<JsonValue, CanonicalError> {
    match raw {
        serde_json::Value::Null => Ok(JsonValue::Null),
        serde_json::Value::Bool(b) => Ok(JsonValue::Bool(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                JsonValue::int(i)
            } else {
                Err(CanonicalError::FloatProhibited)
            }
        }
        serde_json::Value::String(s) => Ok(JsonValue::Str(s)),
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(convert(item)?);
            }
            Ok(JsonValue::Array(out))
        }
        serde_json::Value::Object(map) => {
            let mut out = BTreeMap::new();
            for (k, v) in map {
                out.insert(k, convert(v)?);
            }
            Ok(JsonValue::Object(out))
        }
    }
}

/// Convert a `serde_json::Value` (e.g. from an untrusted source) into a
/// canonical value, rejecting anything the canonical format forbids.

/// Canonical JSON encoder over [`serde_json::Value`] input, for callers that
/// hold dynamic JSON. Equivalent to `contracts.py::canonical` including its
/// rejection rules. Returns the canonical bytes.
pub fn canonical_bytes(raw: &serde_json::Value) -> Result<Vec<u8>, CanonicalError> {
    convert(raw.clone())?.to_canonical_bytes()
}

/// SHA-256 of `canonical_bytes(raw)`, lowercase hex.
pub fn canonical_sha256(raw: &serde_json::Value) -> Result<String, CanonicalError> {
    Ok(hex_hex(Sha256::digest(canonical_bytes(raw)?.as_slice()).as_slice()))
}

/// Lowercase SHA-256 hex of raw bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_hex(Sha256::digest(bytes).as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference vectors produced with tools/contracts.py::canonical.
    fn py_canonical(v: &str) -> String {
        String::from_utf8(parse(v).unwrap().to_canonical_bytes().unwrap()).unwrap()
    }

    #[test]
    fn key_sorting_matches_python() {
        // Python: json.dumps({"b":1,"a":2,"\u00e9":3,"Z":4}, sort_keys=True, ensure_ascii=False)
        assert_eq!(py_canonical(r#"{"b":1,"a":2,"é":3,"Z":4}"#), r#"{"Z":4,"a":2,"b":1,"é":3}"#);
    }

    #[test]
    fn escapes_match_python() {
        let canon = |s: &str| {
            String::from_utf8(JsonValue::Str(s.to_string()).to_canonical_bytes().unwrap()).unwrap()
        };
        // Reference: json.dumps('a"b\\c' + chr(9) + 'd' + chr(10), ensure_ascii=False)
        assert_eq!(canon("a\"b\\c\td\n"), "\"a\\\"b\\\\c\\td\\n\"");
        // DEL (U+007F) is NOT escaped by Python with ensure_ascii=False.
        assert_eq!(canon("\u{7f}"), "\"\u{7f}\"");
        assert_eq!(canon("\u{1}"), "\"\\u0001\"");
        assert_eq!(canon("\u{1f}"), "\"\\u001f\"");
    }

    #[test]
    fn literals_and_ints() {
        assert_eq!(py_canonical("[true,false,null,0]"), "[true,false,null,0]");
        // Lexical -0 is rejected by the Rust deserializer (serde_json
        // classifies it as a float); the reference would emit 0 — a strict
        // subset of the reference grammar.
        assert!(parse("[-0]").is_err());
        assert_eq!(JsonValue::int(MAX_SAFE_INTEGER).unwrap().to_canonical_bytes().unwrap(), b"9007199254740991");
        assert!(JsonValue::int(MAX_SAFE_INTEGER + 1).is_err());
        assert!(JsonValue::int(-MAX_SAFE_INTEGER - 1).is_err());
    }

    #[test]
    fn rejects_floats_and_duplicates() {
        assert!(parse(r#"{"a":1.5}"#).is_err());
        assert!(parse(r#"{"a":1e3}"#).is_err());
        assert!(parse(r#"{"a":NaN}"#).is_err());
        assert!(parse(r#"{"a":Infinity}"#).is_err());
        // Duplicate keys are rejected; the custom visitor error surfaces as
        // a serde syntax error, which is the same rejection the reference
        // loader performs.
        assert!(parse(r#"{"a":1,"a":2}"#).is_err());
    }

    #[test]
    fn hash_chain_vector() {
        // Reference: hashlib.sha256(json.dumps({"a":1,"b":2},sort_keys=True,
        //   separators=(',',':')).encode()).hexdigest()
        let v = JsonValue::object([("a", JsonValue::Int(1)), ("b", JsonValue::Int(2))]);
        let bytes = v.to_canonical_bytes().unwrap();
        assert_eq!(bytes, b"{\"a\":1,\"b\":2}");
        assert_eq!(sha256_hex(&bytes), "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777");
        assert_eq!(v.canonical_sha256().unwrap(), "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777");
    }
}
