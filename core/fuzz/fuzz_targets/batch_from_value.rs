#![no_main]
//! `harbor.artifact_batch/v3` parsing and application against arbitrary
//! JSON and arbitrary base bytes: the commit path must refuse, never
//! panic, on anything a tampered snapshot could carry.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some(split) = data.iter().position(|b| *b == 0) else { return };
    let (a, bytes) = data.split_at(split);
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(a) else { return };
    if let Ok(batch) = harbor_core::tools::builtin::batch_from_value(&v) {
        let _ = batch.canonical_hash();
        let _ = harbor_core::tools::builtin::apply_batch(&bytes[1..], &batch);
        let _ = harbor_core::tools::builtin::proposal_diff(&bytes[1..], &batch);
    }
    let _ = harbor_core::tools::builtin::detect_kind(&bytes[1..]);
});
