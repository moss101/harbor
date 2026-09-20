#![no_main]
//! `harbor.graph/v1` parsing and structural validation against arbitrary
//! JSON: a manifest that reaches the executor has been through this, so
//! it must never panic or loop.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) else { return };
    if let Ok(g) = harbor_core::graph::Graph::from_value(&v) {
        let _ = g.validate();
        let _ = g.tools();
        let _ = g.model_nodes();
        for n in &g.nodes {
            let _ = n.id();
            let _ = n.kind();
        }
    }
});
