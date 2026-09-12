# Harbor implementation status — handoff state

Date: 2026-09-12 (session 2). Build identity: workspace `core/Cargo.toml` v0.1.0
(not yet a git repository — initialize git before any promotion claim so evidence
can bind to a commit).

## Verified state — latest session (session 8, commits through d931839)

| Suite | Command | Result |
| --- | --- | --- |
| Rust workspace (14 crates) | `cd core && cargo test --workspace` | 149 passed, 0 failed |
| Rust + llama.cpp backend | `cargo test -p harbor_inference --features gguf-backend` | +11 real-model tests (on-device Metal inference) |
| Dossier contract freeze | `python3 tools/validate_dossier.py` | PASS |
| Contract suite | `python3 tools/test_contracts.py` | 124 passed |
| harbor_ui | `flutter test` (packages/harbor_ui) | 7 passed |
| harbor_app (incl. a11y + l10n gates + live-dylib RAG) | `flutter test` (apps/harbor_app) | 18 passed |
| harbor_native / harbor_domain | `dart test` | passed |
| Contrast audit | `python3 tools/check_contrast.py` | 46/46 pairs pass |
| SBOM | `python3 tools/generate_sbom.py --write` | CycloneDX 1.5, 375 components |
| Platform builds (last run) | flutter build macos / ios --simulator / apk --debug | all produced |

## Session 9 additions (in-app acquisition through the Egress Broker)

1. **Real HTTPS transport** (`harbor_net::transport::UreqTransport`, ureq +
   rustls, redirects DISABLED): every hop returns to the broker for
   re-authorization — the transport can never bypass policy.
2. **HfAcquirer** (`harbor_modelhub::acquire`): brokered multi-origin
   download (huggingface.co + CDN origins incl. the new Xet storage
   us.aws.cdn.hf.co / cas-bridge.xethub.hf.co), explicit weight-transfer
   sessions per origin, hash verification per file, staged install commit.
   Without a CDN session the download is refused (logged redirect_blocked).
3. **FFI**: `models.search_hf` (acquisition metadata only),
   `models.acquire_hf` (brokered download + staged install; first-download
   hash becomes the recorded package identity). Dart client + Models →
   Hugging Face tab wired (search → install flow).
4. **Real-network test**: acquires stories260K through the broker across
   the CDN redirect chain; installed bytes hash-match the fixture
   (270cba1b…); audit log shows dispatched + completed entries.
5. Status check added: non-2xx responses are errors, never silently
   hashed (found via a rate-limit response during testing).

## Session 10 additions (signed catalog + streaming acquisition)

1. **Signed catalog drives acquisition**: `parse_catalog_document` +
   `acquire_signed` join the two halves — per-file SHA-256 hashes live ONLY
   inside the signed catalog document (tamper-evident, epoch-protected);
   `acquire_signed` verifies the signature/epoch FIRST, then downloads and
   enforces the signed hashes. A signed-but-wrong hash blocks the install
   (real-network negative test). Entries without a pinned 64-char hash are
   rejected at parse.
2. **Streaming download for multi-GB models**: `Transport::execute_streaming`
   (ureq impl streams 64 KiB chunks; buffered default for other transports)
   + `EgressBroker::dispatch_streaming` (same authorization/hop loop, non-2xx
   is an error). `fetch_streaming_to` writes staged bytes incrementally with
   an incremental SHA-256 — memory use is O(chunk), not O(file).
3. FFI: `catalog.import` (verify + accept a signed document; epoch + trust
   state persist in the handle), `models.acquire_catalog` (acquire from the
   accepted catalog; sessions re-opened per origin per acquisition).
4. Retry-once for 429/503 (server-side refusals; GETs are not protected
   effects) — removes CI flakiness from rate limiting.

## Session 12 additions (release builds, device qualification, composer wiring)

1. **Qualified reference device bound**: real manifest
   fixtures/qualification/reference_device_macos_arm64.json
   (MacBook Pro Mac17,8 / Apple M5 Pro / 24GB / macOS 26.5.1, Metal 4)
   documenting the M1 + inference evidence that ran on THIS hardware;
   `qualified_device_manifest_sha256` = 42f662ae… now bound in
   26_Qualification_Profiles.json (M1/M2 dev qualification).
2. **Release-mode builds**: macOS release harbor_app.app (42.9MB, ad-hoc
   signed) and Android app-release.apk (50.2MB, debug-key signed —
   STORE SIGNING still requires real credentials). Reproducible release
   compilation proven; store distribution remains BLOCKED.
3. **Home composer drives the runtime**: submit creates a durable run and
   logs the request as its first step under lease authority
   (FFI `run.log_request`); the run appears in Activity/Lens. Widget test
   included (19/19 app tests).
4. **Test tiers split**: `cargo test --workspace` is fully offline
   (152 tests); real-network tests are `#[ignore]`-marked and run via
   `cargo test --workspace -- --ignored` (4 tests), which CI now does as a
   separate step.

## Session 15 additions (performance baselines measured)

First execution of the 15_Performance_Qualification.yaml protocol on the
qualified reference device: `cargo run -p harbor_integration --example
perf_baseline --features gguf-backend -- <repo> <models-store>` writes
evidence/perf_baseline.json with RAW samples + derived p50/p95 and full
protocol identity (device manifest, model hash 6a1a2eb6…, runtime
llama.cpp/llama-cpp-sys-2@0.1.156).

Measured (Apple M5 Pro, Metal, Qwen2.5-1.5B Q4_K_M):
- model load: 153ms p50 / 169ms p95
- first-token latency: 53ms p50 / 60ms p95
- generation throughput: 148 tok/s
- artifact open 1ms / recalc 7ms / save 3ms (p50, board_demo fixture)
- RAG indexing: 21,277 docs/min (bge-small 384-dim embeddings)
- cancellation SLOs (250ms ack / 5s unack-pause): protocol-bound, test-covered

These are BASELINE measurements, not the independent qualification run:
GA still requires freezing approved thresholds and minimum-device runs.

## Session 17 additions (harbor_sync E2EE protocol, M4 optional)

`core/harbor_sync` implements the 11_Sync_Protocol.md core, fully tested:
- Record envelope: group, device, device sequence, key epoch, record type,
  object ID, hybrid logical clock, previous-record hash, tombstone flag,
  AEAD ciphertext (ChaCha20-Poly1305, unique per-record nonce, all envelope
  fields bound as AAD). Tampering any field breaks authentication.
- Receiver policy: per-device sequence contiguity + per-device hash chain,
  duplicate rejection, current-epoch-only live uploads. Old-epoch records
  remain readable as history; restored old backups can never roll the
  epoch backward.
- Revocation advances the epoch; revoked devices cannot seal new records.
- LWW policy is type-enforced: only appearance.theme, display.density,
  ui.language may merge by LWW; privacy/capabilities/approvals/routing
  never do.
- Transfer handshake: durable source acknowledgement (stopped + dispatch
  authority revoked + effects settled-or-outcome_unknown), destination
  generation increment, crash retries reuse the transfer ID, completion
  immutable.
- Device horizon 90 days; tombstone retention 120 days (authority consts).
Sync remains DISABLED by default; activation requires ACC-057 (M4 gate).

## Session 18 additions (Office-matrix conformance deepening)

- New conformance suite (harbor_artifacts/tests/office_conformance.rs)
  covering matrix rows: DOCX headings (pStyle) + numbered-list membership
  (numPr) + tables with horizontal merge (gridSpan) — all read in document
  order, and typed edits preserve the remainder of the document; XLSX
  merged cells (A1:B1) survive Harbor's load/edit/recalc/save round-trip
  with the recalculated formula cache persisted. (Matrix rows 2-4, 11,
  23; SUPPORTED_GA.)
- 168 Rust tests green; dossier PASS.

## Session 14 additions (key ceremony, launch verification)

1. **Catalog key ceremony executed** (harbor_modelhub example
   `key_ceremony.rs`): release root key generated (key_id
   5cff12934466e591); the SECRET seed is stored OUTSIDE the repository at
   ~/.harbor-keys/catalog-root.key (0600) and must move to secure/offline
   storage before public release. The signed production catalog
   (fixtures/catalog/signed_catalog.json, epoch 1) binds all three model
   packages with pinned hashes (qwen2.5-1.5b-instruct, bge-small-en-v1.5,
   stories260k test model) plus root_public.hex. A permanent test verifies
   the COMMITTED signature against the COMMITTED root key on every run.
2. **Launch verification**: the macOS RELEASE build
   (harbor_app.app) was launched, confirmed running (process check), and
   quit cleanly — instals-and-launches evidence for item 1 (macOS).

## Session 19 additions (sync snapshots + device expiry)

Completes the "Offline and restore" section of 11_Sync_Protocol.md in
harbor_sync/src/snapshot.rs:
- Group-clock expiry: devices past the 90-day horizon are expired and the
  epoch advances (revocation rules); a device cannot extend its own
  horizon with an untrusted local clock
- Signed snapshots (`harbor.sync_snapshot/v1`): Ed25519 signature binding
  group, epoch, deletion watermark and issue time; tamper detected
- Restore validation: snapshots older than the live epoch are rejected
  (never roll backward); wrong-group snapshots rejected
- Tests: horizon expiry advances epoch + blocks uploads from the expired
  device while compliant devices continue on the new epoch; signature +
  tamper + stale-restore negatives

## Session 20 additions (DOCX formatting preservation, SBOM versioning)

1. **Formatting-preserving DOCX replacement**: same-grapheme-length
   `text.replace` operations now distribute the new text across the
   paragraph's existing RUNS at their original character spans, so per-run
   formatting (bold/italic spans) SURVIVES the edit. Different-length
   replacements keep the documented first-run behavior. Widget-visible
   via the artifact engine; conformance-tested.
2. **SBOM commit versioning**: the CycloneDX document now carries the git
   commit (harbor:git_commit property, version suffix) and build profile;
   deterministic given identical tree + commit.

## Session 21 additions (snapshot bundle transport)

The bulk-record download for re-enrolling devices
(harbor_sync/src/bundle.rs): `seal_bundle` packages the live record tails
(current-epoch only) sealed under the CURRENT group epoch key with a
unique nonce, snapshot metadata signed by the issuing device's identity
key. `open_bundle` rejects stale-epoch bundles before decryption, verifies
the issuer signature, and returns the authenticated envelopes. E2E test
covers the full 90-day expiry flow: device expires -> epoch advances ->
re-enrollment -> bundle download -> fresh uploads on the new epoch.

## Session 22 additions (DOCX TableCellSet typed op)

- New typed op `DocxOp::TableCellSet { table, row, col, new_text }`:
  merge-aware table-cell addressing via
  `docx::table_cell_paragraph_map` (gridSpan-honoring column accounting,
  global paragraph ordinals), precondition hash validated against the
  loaded document state, other paragraphs preserved. Conformance test
  proves targeted-cell edit leaves merged header and all other cells
  untouched. (harbor_artifacts 20 tests.)

## Session 23 additions (XLSX charts conformance, M2 row 14)

- WorkbookDoc::add_bar_chart: bar/column basic-series chart creation via
  the umya backend (matrix row 14, REQUIRED_UNQUALIFIED -> machinery in
  place); WorkbookPreview now reports embedded chart counts
- Conformance test: chart-bearing workbook survives Harbor's full
  load/edit/recalc/save pipeline with the chart part intact and data
  cells unchanged
- DocxOp::TableCellSet: merge-aware table-cell typed edit (row 23 of the
  conformance suite); formatted-run replacement (same-length distribution)
- Verified: 178 Rust tests; dossier PASS

## Session 24 additions (PPTX chart embedding)

The PptxDeck writer embeds real DrawingML chart parts: slides carry a
`chart: Option<ChartSpec>` (kind bar/line, title, categories, series with
cached values); the package gains `ppt/charts/chartN.xml` (c:chartSpace
with strCache/numCache), `ppt/embeddings/chartdataN.xlsx` (minimal
workbook), chart relationships from the slide, and a graphicFrame in the
slide spTree. Read-back via DeckPreview::from_pptx exposes chart parts
through the existing slide-text parse. Conformance-tested: bar chart with
cached values (3600/4000) survives write/read; chart part, rels and
embedded workbook all present. (harbor_artifacts: 20 tests.)

## Session 16 additions (PDF extraction, iOS launch evidence)

1. **PDF text extraction with page mapping** (harbor_render::pdf +
   FFI artifact.preview auto-dispatch): the PDF Research skill's citation
   basis now exists; corrupt-input rejection tested with a real fixture
   (fixtures/office/hello.pdf).
2. **iOS simulator launch verified**: Runner.app installed on iPhone 17 Pro
   simulator, launched (PID 90681), screenshot captured
   (evidence/ios_launch_screenshot.png), clean shutdown.

## Session 8 additions (a11y, SBOM, migration docs)

1. **Accessibility audit is a permanent test gate**
   (apps/harbor_app/test/accessibility_audit_test.dart, 4 tests): semantic
   labels across all 9 surfaces (found + fixed the unlabeled composer send
   button and Ask search IconButton), 200% text scale at compact width with
   zero overflow (fixed Work header flexibility; HarborEmptyState made
   scrollable), 44px minimum targets on compact navigation destinations,
   keyboard traversal reaching composer + quick actions.
2. **CycloneDX 1.5 SBOM**: `tools/generate_sbom.py --write` — deterministic,
   375 components, includes the pinned security-relevant versions.
3. **docs/release/migration_rollback.md**: N-2 schema support policy,
   fail-safe settings downgrade, binary/model rollback procedures,
   catalog rotation-instead-of-rollback, recovery boundary cross-refs.
4. Package-copy ignores extended to all build artifacts; manifest
   regenerated (dossier PASS).

## Earlier session results (all commands reproducible)

Git: repository initialized at commit `4f90f73` and updated through session 2
commits (see `git log`). Evidence can now bind to exact commits.

| Suite | Command | Result |
| --- | --- | --- |
| Rust workspace (13 crates) | `cd core && cargo test --workspace` | 129 passed, 0 failed |
| Rust + llama.cpp backend | `cargo test -p harbor_inference --features gguf-backend` | +3 real-model tests pass (on-device Metal inference) |
| Dossier contract freeze | `python3 tools/validate_dossier.py` | PASS |
| Executable contract suite | `python3 tools/test_contracts.py` | 124 passed, 0 failed |
| Engine pin freshness | `python3 tools/pin_engine.py --check` | ok |
| harbor_ui design system | `flutter test` (packages/harbor_ui) | 7 passed |
| harbor_app adaptive shell | `flutter test` (apps/harbor_app) | 6 passed |
| Dart↔Rust FFI boundary | `dart test` (packages/harbor_native) | passed against real libharbor_ffi.dylib |
| macOS platform build | `flutter build macos --debug` (apps/harbor_app) | harbor_app.app produced |

## New in session 2 (continuation)

1. **Git repository initialized**; first commit `4f90f73` (280 files). CI
   workflow is in place (`.github/workflows/ci.yml`); it runs on push once a
   remote is added.
2. **GGUF provider upgraded**: model-native chat templates (from GGUF metadata
   via `LlamaModel::chat_template`/`apply_chat_template`, documented minimal
   template as fallback) and mean-pooling `embed()` (deterministic, with the
   index-identity obligation documented on the method). Real-model tests:
   greedy decoding is deterministic; embeddings are stable and text-sensitive.
   11/11 inference tests pass with `--features gguf-backend`.
3. **Live FFI wiring in the app**: FFI dispatcher gained `models.installed`,
   `model.fit_score` (device facts in, score computed in core), `runs.list`,
   `run.replay` (verified event trail), `artifact.preview` (OOXML content-type
   sniffing -> harbor_render preview IR). `HarborService` in harbor_app feeds
   Home (Model Dock shows real installed models / honest degraded state),
   Models > Installed (real list + Fit Score band + reasons), Activity (real
   durable runs + replayed Run Trail), Work Canvas (real workbook preview from
   board_demo.xlsx through blob->preview path). 9/9 app tests green, several
   running the real dylib (loaded via `tester.runAsync`).
4. **harbor_render implemented**: `WorkbookPreview` (values + formulas from the
   real engine recalc) and `DeckPreview` (slides/bullets) preview IR with
   tests. Real fixture `fixtures/office/board_demo.xlsx` generated through the
   engine (sha256 1dbd1df1…; generator committed as harbor_render example).
5. **All three desktop/mobile toolchains now compile**:
   - macOS debug: `harbor_app.app`
   - iOS simulator: `Runner.app` (`flutter build ios --simulator --debug`)
   - Android: `app-debug.apk` (152 MB debug APK, real Android SDK found at
     `~/Library/Android/sdk`)

## Session 1 recap

1. **llama.cpp GGUF provider is real**: `harbor_inference` feature `gguf-backend`
   pins `llama-cpp-2 =0.1.156` (the llama.cpp source snapshot is vendored inside
   `llama-cpp-sys-2`, so the crate pin fixes the runtime revision — decision 0002).
   `GgufLlamaCppProvider` loads installed packages from harbor_modelhub, generates
   with greedy decoding (deterministic) with cooperative cancellation, runs on
   Metal. `tests/gguf_provider.rs` installs the real stories260K model
   (GGUF v3, sha256 270cba1b… recorded in 26_Qualification_Profiles.json) through
   the staged-install path and asserts real generation with token accounting.
2. **Flutter SDK 3.47.4 stable** installed at `~/harbor-tools/flutter`.
3. **packages/harbor_ui**: Harbor Current 2 tokens (light/dark/semantic roles),
   Arabic line-height parity, breakpoint contract (§20), and the named product
   patterns: Harbor Rail, Trust Pulse (policy ≠ execution facts), Run Trail,
   Harbor Sheet, Model Dock, Fit Score badge (5 bands + reasons), Artifact Diff
   view, first-class empty/error states. Icon+label+color status everywhere.
4. **apps/harbor_app**: adaptive shell with all 9 surfaces (Home · Ask · Work ·
   Agents · Models · Skills · Knowledge · Activity · Settings), bottom nav <600,
   rail 1024+, persistent Lens 1280+, work-first Home (no chat phrasing), EN/AR
   with true RTL via the app's own Settings switch. Builds for macOS.
5. **core/harbor_ffi + packages/harbor_native**: single JSON-dispatch C ABI over
   harbor_core (runs, blobs, trust pulse, formula qualification); Dart client
   verified against the real dylib: workspace open, run create/state, encrypted
   blob round-trip, typed error envelopes.

## Honest gaps (unchanged or newly scoped)

1. **Chat templates**: GGUF generation uses the documented minimal template;
   model-native chat templates (from GGUF metadata via llama.cpp common) are the
   next provider task. Streaming embeddings path still to wire.
2. **iOS/Android/Windows builds**: project scaffolding exists for all four
   platforms; only macOS has been compiled here (no Android SDK installed;
   iOS/Windows untried). Signing and store packaging are later gates.
3. **harbor_render** (Office preview rendering) and **harbor_sync** (optional,
   M4): empty crates.
4. **Core UI is presentation-only so far**: surfaces render states and tokens;
   wiring them to live FFI calls (real runs, real artifacts, real Fit Scores)
   is the next app milestone.
5. **harbor_domain** (Dart presentation/domain types per 08_Repo_Structure):
   not created; surfaces currently import harbor_ui directly.
6. **Formula target TEXT** remains REQUIRED_UNQUALIFIED (decision 0001).
7. **Release gates**: model_package_sha256 / evaluation_corpus_sha256 /
   qualified_device_manifest_sha256 remain honestly unset until real model,
   evaluation and device qualification (M2/M3).
8. **git**: the repo is not initialized; CI workflow exists but has never run.

## Environment notes

- Disk pressure: keep `core/target` (~7 GB) and `~/harbor-tools/flutter`
  (~3 GB extracted + 2.3 GB zip; the zip can be deleted) in mind.
- Flutter SDK is NOT on PATH; use `~/harbor-tools/flutter/bin/flutter` and
  `~/harbor-tools/flutter/bin/cache/dart-sdk/bin/dart`.

## Next tasks (dependency-ordered)

1. `git init` + push + first CI run.
2. Wire harbor_app surfaces to live FFI calls (Home → run creation → Run Trail;
   Models → modelhub installed list + Fit Score; Work → workbook recalc proof).
3. GGUF provider: model-native chat templates; embed() for the knowledge index.
4. Android SDK install + `flutter build apk --debug`; iOS build with signing.
5. harbor_render: workbook/deck preview IR feeding the Work Canvas.
6. M2 qualification per 18_Reconciled_Release_Matrix.csv.
