# Harbor implementation status — handoff state

Date: 2026-09-12 (session 2). Build identity: workspace `core/Cargo.toml` v0.1.0
(not yet a git repository — initialize git before any promotion claim so evidence
can bind to a commit).

## Verified state this session (all commands reproducible)

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

## New this session

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
