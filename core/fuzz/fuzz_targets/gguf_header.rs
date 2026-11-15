#![no_main]
//! GGUF pre-load validation (decision 0022, phase 2) against arbitrary
//! bytes: every model file passes through `gguf_check` before llama.cpp
//! sees it, so the checker must answer `Ok`/`Err` on any input — never
//! panic, never loop, never allocate in proportion to a declared count.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = harbor_inference::gguf_check::check_bytes(data, data.len() as u64);
    // A header slice of a larger file exercises the data-bounds checks.
    let _ = harbor_inference::gguf_check::check_bytes(data, u64::MAX);
});
