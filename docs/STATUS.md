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
