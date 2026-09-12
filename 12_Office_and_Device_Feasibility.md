# Harbor Office and device feasibility authority

## Office engine
Harbor uses a safe OOXML package parser, stable Artifact IR, typed edit batches and a Harbor renderer shared across platforms. Untouched unsupported OOXML parts are preserved byte-for-byte when safe. Macros/OLE/active content are never executed.

### Spreadsheet calculation
The calculation adapter is `harbor_formula`; the selected baseline engine is **Formualizer 0.9.0**, vendored/pinned by exact source revision and integrity hash during HBR-153 before production builds. Harbor owns the compatibility contract and can replace the engine without changing agent/tool schemas. `22_Formula_Coverage.json` is the executable qualification manifest: a function is considered verified only when its entry is `PASS` for the exact engine revision, Harbor adapter revision, workbook fixture corpus and platform qualification set. Entries marked `REQUIRED_UNQUALIFIED`, `PRESERVE_ONLY` or `UNSUPPORTED` cannot support a verified numerical claim. Cached workbook values are provenance-tagged and cannot support a verified numerical claim when any dependency is unsupported or stale. Unsupported formulas are preserved, shown as unverified, and block dependent verified analysis. TODAY/NOW use a run-fixed evaluation clock.

### Rendering
DOCX: paragraphs, runs, lists, tables, images, common section/header/footer/style features are rendered by Harbor; advanced fields, floating layout and complex drawing features are preserve-only until qualified. PPTX: common text, shapes, images, themes, notes and qualified chart types render; animations/transitions/macros are preserve-only. XLSX: grid, merged cells, common number formats/styles and qualified charts render; pivots/slicers/macros/external data are preserve-only. Every import produces a compatibility report: `supported`, `preserved_not_rendered`, `preserved_not_recalculated`, or `rejected`. `21_Office_Feature_Matrix.csv` is the release authority for these classifications; any feature not explicitly marked `SUPPORTED_GA` is not part of the GA fidelity promise.

### Size ceilings
Mobile default ceilings: Office compressed package 100 MB, expanded XML/media 400 MB, PDF 250 MB/750 pages, workbook 750k non-empty cells. Desktop default ceilings: Office compressed 250 MB, expanded 1.5 GB, PDF 750 MB/2500 pages, workbook 3M non-empty cells. Device resource policy may lower a ceiling before opening. Limits are user-visible; no decompression beyond the preflight ceiling.

## Product support floor
The Flutter framework may support wider OS ranges, but Harbor GA qualification is intentionally narrower: iOS/iPadOS 18+ arm64; Android API 29+ arm64-v8a; macOS 13+ Apple Silicon; Windows 11 22H2+ x64/arm64. Custom local-model qualification requires 6 GB physical RAM on mobile and 8 GB on desktop; lower-memory devices may use system-managed models or smaller explicitly qualified packages.

## Resource policy
Model admission must leave an OS/UI reserve: at least 1.5 GB mobile and 2.0 GB desktop, and estimated peak must fit within the runtime safe-memory envelope. Generation, indexing and rendering share one process-wide resource governor. Serious thermal pressure pauses new heavy work; critical pressure cancels/unloads according to platform rules.

## Performance qualification
Do not invent universal speed promises. `15_Performance_Qualification.yaml` defines every metric that must be populated from the lowest qualified device before GA. The only fixed safety SLO is cancellation acknowledgement <=250 ms p95 for the reference generation workload; other GA speed thresholds remain BLOCKED until measured and approved.


## Qualification procedures
26_Qualification_Profiles.json and fixtures/qualification define workload sizes, repetitions, numerical/date/error behavior, fidelity comparison and immutable fixture binding. Fixture source hashes are supplied now; binary workbook/deck render artifacts and vendor/model/device identities must be bound before runtime qualification. Null bindings mean BLOCKED, never an implicit default. Every formula PASS must identify engine source digest, adapter revision, fixture corpus digest, target set and evidence digest. REQUIRED_UNQUALIFIED chart rows cannot be promoted by an unrelated workflow PASS.

For compact viewports below 1024 logical pixels, Office uses viewport-sized controls and structured editing; artifact content may scroll. The 640-pixel minimum applies only to the desktop editor viewport at widths >=1024. At 320 and 390 pixels, approval and conflict actions remain visible at 200% text scale.
