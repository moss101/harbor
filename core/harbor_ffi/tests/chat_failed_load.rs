//! A failed chat-model load must not leave the package recorded as loaded
//! (upgrade plan C7): the id was inserted before `provider.load`, so after
//! one failure every later generate reported ModelNotFound until restart,
//! and the in-use guard refused to delete a model that was never loaded.
#![cfg(feature = "gguf")]

use std::sync::atomic::AtomicBool;

use harbor_ffi::knowledge::ChatHandle;

#[test]
fn failed_load_is_not_recorded_as_loaded() {
    let models = tempfile::tempdir().unwrap();
    let chat = ChatHandle::new(models.path());
    let never = AtomicBool::new(false);
    for _ in 0..2 {
        let Err(err) = chat.generate_rag_with_history(
            "no-such-package",
            "hi",
            Vec::new(),
            &[],
            harbor_ffi::knowledge::Grounding::Required,
            8,
            &never,
            None,
        ) else {
            panic!("generate succeeded without a model");
        };
        assert!(!err.is_empty());
        assert!(chat.loaded_package_ids().is_empty(), "{err}");
    }
}
