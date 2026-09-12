//! Embedded fixture bundle and its identity.
//!
//! The corpus is included at compile time; the bundle hash binds
//! qualification results to the exact corpus bytes.

pub const CORPUS_JSON: &str = include_str!("../fixtures/formula_corpus.json");

pub fn bundle_sha256() -> String {
    harbor_canonical::sha256_hex(CORPUS_JSON.as_bytes())
}

pub fn pinned_clock() -> (chrono::DateTime<chrono::Utc>, &'static str) {
    (
        chrono::DateTime::parse_from_rfc3339("2026-09-12T09:00:00Z")
            .expect("pinned clock")
            .with_timezone(&chrono::Utc),
        "Asia/Riyadh",
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundle_is_nonempty_and_hashed() {
        assert!(super::CORPUS_JSON.len() > 10_000);
        assert_eq!(super::bundle_sha256().len(), 64);
    }
}
