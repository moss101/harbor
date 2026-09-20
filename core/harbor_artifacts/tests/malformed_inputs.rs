//! Malformed office packages found by fuzzing (production plan C4) must be
//! typed load errors, never panics: a user-chosen file reaches these
//! loaders through the tool layer and the executor.

fn regression(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/regressions")
            .join(name),
    )
    .unwrap()
}

#[test]
fn corrupt_deflate_stream_in_a_workbook_is_a_load_error() {
    let bytes = regression("corrupt_deflate.xlsx");
    let err = match harbor_artifacts::WorkbookDoc::load(&bytes) {
        Ok(_) => panic!("corrupt package loaded"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("malformed workbook"), "{err}");
}

#[test]
fn garbage_and_truncated_packages_are_load_errors() {
    for bytes in [
        b"PK\x03\x04garbage".to_vec(),
        Vec::new(),
        vec![0u8; 64],
        b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0".to_vec(),
    ] {
        assert!(harbor_artifacts::WorkbookDoc::load(&bytes).is_err());
        assert!(harbor_artifacts::DocxDocument::load(&bytes).is_err());
        // An empty-but-valid zip is an empty deck, not a crash; anything
        // else is an error.
        if let Ok(deck) = harbor_artifacts::PptxDeck::from_pptx_bytes(&bytes) {
            assert!(deck.slides.is_empty());
        }
    }
}
