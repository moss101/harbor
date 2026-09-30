# Harbor Office Suite

The standalone, downloadable office apps: documents (DOCX), workbooks
(XLSX, editable — formulas, charts, highlights), presentations (PPTX)
and PDF, plus Markdown / PDF → Word conversion. Same Rust core as
Harbor (`core/harbor_ffi`), office-only surfaces — no models, ask,
agents or knowledge.

- iOS bundle id: `dev.harbor.office`
- Android application id: `dev.harbor.office`
- The Xcode build phase cargo-builds and embeds
  `core/target/aarch64-apple-ios-sim/release/libharbor_ffi.dylib` for
  simulator builds (device builds link the static archive, same as the
  main app).
