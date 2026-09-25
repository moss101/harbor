//! Reproducer for the `batch_from_value` fuzz finding
//! (crash-92b3025ecb55a42824f0ca23637d20c4bd4f9879, CI run 36195333990).
//!
//! The input is a well-formed `harbor.artifact_batch/v3` whose `cell.set`
//! carries the address "Dxxxxxxxxxxxxxxxxxx2" — 18 'x' characters — applied
//! against a real XLSX. The commit path must REFUSE it, never panic: a
//! tampered snapshot is untrusted input, and "always answers" is the
//! property the fuzz targets exist to hold.
//!
//! The file is the exact bytes libFuzzer minimised, kept verbatim so this
//! test fails for the original reason and not a paraphrase of it.

#[test]
fn a_batch_with_an_absurd_cell_address_is_refused_not_panicked_on() {
    let data = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/batch_crash_92b3025e.bin"),
    )
    .expect("fuzz reproducer fixture");
    let split = data.iter().position(|b| *b == 0).expect("nul separator");
    let (json, rest) = data.split_at(split);
    let bytes = &rest[1..];
    let v: serde_json::Value = serde_json::from_slice(json).expect("valid JSON");

    // Exactly the fuzz target's call sequence.
    if let Ok(batch) = harbor_core::tools::builtin::batch_from_value(&v) {
        let _ = batch.canonical_hash();
        let _ = harbor_core::tools::builtin::apply_batch(bytes, &batch);
        let _ = harbor_core::tools::builtin::proposal_diff(bytes, &batch);
    }
    let _ = harbor_core::tools::builtin::detect_kind(bytes);
}
