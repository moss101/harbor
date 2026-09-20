#![no_main]
//! The JSON Schema validator below the model (`harbor_core::jsonschema`)
//! against arbitrary (schema, instance) pairs: never panics, never hangs
//! (bounded recursion), and a schema that validates an instance is
//! idempotent.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some(split) = data.iter().position(|b| *b == 0) else { return };
    let (a, b) = data.split_at(split);
    let Ok(schema) = serde_json::from_slice::<serde_json::Value>(a) else { return };
    let instance = serde_json::from_slice::<serde_json::Value>(&b[1..])
        .unwrap_or(serde_json::Value::String(String::from_utf8_lossy(&b[1..]).into_owned()));
    let v1 = harbor_core::jsonschema::validate(&schema, &instance);
    let v2 = harbor_core::jsonschema::validate(&schema, &instance);
    assert_eq!(v1.len(), v2.len());
    for v in &v1 {
        let _ = v.to_string();
    }
});
