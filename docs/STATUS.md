# Harbor — project status

This file has ONE authoritative snapshot (below) followed by historical
session notes. The snapshot is rewritten each session; history is append-only
context. Evidence files under `evidence/` bind to the exact git commit they
ran at.

---

## Authoritative snapshot — session 29 (2026-09-14; post-RC hardening
## round; HEAD binds machine evidence unless stated otherwise — run
## `git log --oneline -3`)

### Session 29: the ten-item post-RC work order (user-directed)

The user issued a ten-item recommended order of work (persistent storage,
knowledge encryption, OS keystores, full journey, no-op removal, FFI off
the UI isolate, knowledge lifecycle, evidence repair, CI gates, external
qualification). All machine-completable items are DONE and requalified at
the session-29 commit; the external item remains blocked on hardware /
operator credentials, unchanged from rc1.

1. **Persistent app storage, service disposal, unique run IDs — FIXED.**
   - Storage: the app data root moved from a per-launch temp dir to the
     platform app-support dir (`path_provider`):
     `~/Library/Application Support/dev.harbor.harborApp/harbor-data`
     (macOS) / `/data/data/<pkg>/files/harbor-data` (Android). Verified in
     the PACKAGED macOS app (agent.db + network_audit.db + store.db created
     there; dylib mapped) and on the Android emulator (below). Durable
     device identity persists in `store.db device_meta` and is stable
     across restarts on both platforms.
   - Disposal: `HarborService.close()` actually existed but was never
     called; the app now closes the native core on widget dispose AND on
     `AppLifecycleState.detached` (WidgetsBindingObserver).
   - Run IDs: `submitRequest` used the LITERAL string
     `run-\${DateTime.now()...}` (an escaped `$` — a constant id!), so the
     second submission in a workspace always failed with RunExists and was
     silently swallowed. IDs are now 96-bit random hex (`Random.secure`)
     Dart-side, and `run.create` mints `HarborId::generate("run")` core-side
     when the caller omits one.
2. **Knowledge database encryption — DONE + qualified.** Chunks and
   embedding vectors are sealed with ChaCha20-Poly1305 under a
   workspace-derived key (`harbor.knowledge.chunk/v1`), AAD-bound to each
   chunk identity; at rest `knowledge.db` holds no text and no vectors.
   Legacy plaintext DBs (dev-era format) are migrated by re-sealing.
   The plaintext-at-rest inspection (`plaintext_at_rest_inspection.rs`)
   now drives the REAL persistence layer (`harbor_ffi::knowledge::
   KnowledgeStore`) with a knowledge sentinel and byte-scans the whole
   data root — PASS at the session commit (surfaces list includes
   knowledge_db). Unit tests cover round-trip, tamper, AAD-binding and
   key-separation; the flutter e2e knowledge/RAG tests cover the live
   sealed path.
3. **Real OS keystore adapters — DONE.** `harbor_store::native_keystore`:
   - macOS/iOS: `KeychainKeyStore` (Security framework generic-password
     items; verified live in the packaged macOS app — item
     `dev.harbor.core/harbor.device` created and reused).
   - Windows: `DpapiKeyStore` (CryptProtectData, user scope) —
     compile-gated by the new `windows-core` CI job (cargo check + clippy
     for the whole workspace on windows-latest).
   - Android: the NDK side of a dlopened lib has no JavaVM, so the
     embedding is the adapter: Kotlin `MainActivity` seals a random
     32-byte root under a non-exportable AndroidKeyStore AES-256-GCM key
     (`files/harbor-device-root.bin`, 60-byte nonce||ct blob) and injects
     the root at open via the new `harbor_core_open_ex(..., root_hex)`.
   - Legacy file-root data roots are rotated to the native keystore at
     open (`rotate_file_root_to_native`, tested incl. data survival).
   - Caveat recorded: ad-hoc rebuilds change the binary hash; the
     keychain item ACL follows the creating binary, so a rebuild needs one
     user Allow (or a reset). Signed releases have a stable identity.
4. **Complete user journey — WIRED and TESTED.** Models install (HF
   acquisition or local GGUF import) -> Knowledge ingestion (file picker
   via `file_selector`, or pasted text; docx/pdf text extracted through
   the qualified preview paths; everything else honestly refused) ->
   Ask (grounded generation with a selected chat model; answer card with
   citations, executed-on and token usage; INSUFFICIENT_EVIDENCE renders
   the abstention note) -> durable activity (`op.start_generate` logs the
   question as StepStarted and the answer as StepCompleted into the run;
   Activity lists the run and Run Trail replay shows request + answer
   summaries). Covered by `shell_test.dart` knowledge + RAG journey tests
   against the real core (install via staged path, ingest, generate,
   replay).
5. **No-op / misleading actions — REPLACED with real behavior or honest
   disabled states.** Home quick chips prefill the composer; the attach
   button routes files into the knowledge index; ModelDock navigates to
   Models; HF rows run the REAL brokered acquisition (the fake
   `setState(_installedId)` button is gone) with live progress + cancel;
   Models > Installed empty state navigates to Recommended; Work > Open
   file opens a real picker; Ask no longer has a permanently-disabled
   search (retrieval-only search + generate actions); Agents surface
   states plainly that agent orchestration is not enabled in this
   release (no fake button); Knowledge surface is a full management UI
   (add files/paste text/remove/identity); Settings shows the durable
   device identity. `models.search_hf` envelope fixed (was a bare JSON
   array the Dart client could never parse — every HF search crashed).
6. **FFI off the UI isolate + progress/cancellation — DONE.**
   `harbor_native/harbor_worker.dart`: a long-lived background isolate
   owns the native handle; every core call crosses it (request/reply
   protocol over ports), so the UI isolate never blocks on FFI.
   Long-running work (acquisition, ingestion, generation) additionally
   runs on native threads behind an op registry: `op.start_acquire` /
   `op.start_generate` / `op.start_ingest` / `op.status` / `op.cancel` /
   `op.list`, with real progress (bytes via the brokered download loop,
   chunks during ingest, tokens during generation — `AcquireProgress`
   plumbed through `HfAcquirer` and `generate_cancellable`) and
   cooperative cancellation at chunk/token/file boundaries. The service
   polls status, exposes kind-tracked snapshots, and the UI renders
   progress bars + cancel buttons.
7. **Knowledge replacement, removal, identity — FIXED.** Ingesting an
   existing source id now REPLACES it wholesale (transactional delete +
   reinsert; the live index is replaced too — stale higher-ordinal
   chunks of a longer previous version can no longer survive; regression
   tested). `knowledge.remove_source` + `knowledge.sources` exposed over
   FFI and managed in the UI (removed sources stay revoked so past
   citations report Removed). Device identity persists (store.db);
   `identity.get` exposes device + workspace identity.
8. **Release evidence — REGENERATED at the session-29 commit.** All
   machine suites re-run: `cargo test --workspace` 214 passed / 0 failed;
   gguf-backend suite; plaintext-at-rest inspection (now incl.
   knowledge.db) PASS; flutter suites (shell 13, a11y 5, l10n 2, plus
   harbor_ui/domain/native packages) PASS; `dart format` + `flutter
   analyze` clean in all four Dart packages; `cargo fmt --check` and
   `cargo clippy --workspace --all-targets -- -D warnings` clean.
   Performance qualification re-run (reference device). Artifacts
   repackaged (APK v1.0.0(1), AAB, macOS app, iOS static-archive path
   verified at link level with Security.framework). Evidence bundle
   reassembled as `1.0.0-rc2` (tag `harbor-v1.0.0-rc2`).
9. **CI gates — ADDED and GREEN.** `.github/workflows/ci.yml` now gates:
   rustfmt, clippy `-D warnings` (plus a windows-latest job for the
   DPAPI/cdylib paths), dart format, flutter analyze, package tests, and
   the app suite against the LIVE core (the job builds `libharbor_ffi`
   first). Fully green on the session-29 HEAD (`run 34877254045`: rust ✓
   flutter ✓ dossier ✓ windows-core ✓). Two CI findings fixed along the
   way: the dossier manifest now seals exactly the git-tracked file set
   (gitignored generated files previously made the seal irreproducible),
   and the real-network test tier is qualification-machine-local — HF's
   edge resets shared runner IPs, so runner runs are not meaningful
   evidence; the tier passes on this machine and is recorded
   commit-bound in evidence/.
10. **External qualification — UNCHANGED blockers, machine part done.**
    Android emulator tier requalified on the NEW release APK (see below);
    physical iPhone/Android hardware, Apple identity + notarization, Play
    Console upload key, a Windows machine and the min-spec Apple-silicon
    device remain BLOCKED_EXTERNAL / BLOCKED_DEVICE_EVIDENCE exactly as in
    rc1 (`resume_with` paths unchanged in the gate report).

### Requalification record (evidence/device_qualification.json →
### session_29_persistence_and_keystore_requalification, commit-bound)

- **macOS packaged app** (ad-hoc, release): launch live, dylib mapped;
  persistent data root in Application Support; keychain-held device root
  (no file keys); device identity stable across quit + relaunch.
- **Android emulator tier** (cvbase_test API 34 arm64, release APK
  v1.0.0(1)): AndroidKeyStore-sealed root (60-byte blob) injected via
  `harbor_core_open_ex`; persistent workspace (agent.db + WAL/SHM +
  network_audit.db + store.db); device identity
  `device-00d53bcb64dab31b41a8c5b3` stable across `am force-stop` +
  relaunch. extractNativeLibs=false caveat as before.

### Test results (regenerated at the session-29 commit)

| Suite | Result |
| --- | --- |
| `cargo test --workspace` | **214 passed / 0 failed** |
| gguf-backend inference suite | PASS (component of the workspace run) |
| Plaintext-at-rest inspection | PASS — now covers knowledge.db (sealed chunk sentinel) |
| Flutter: app shell (13), a11y (5), l10n (2), packages (9) | PASS; `flutter analyze` + `dart format` clean |
| Clippy / rustfmt | `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean |
| Performance qualification | re-run on the reference device (see evidence/perf_qualification.json) |
| Office / network / optional-disabled / SBOM / notices | PASS (regenerated; notices 408 entries) |

### External-prerequisite audit (2026-09-14)

The absence of every operator-supplied prerequisite was VERIFIED on this
machine, not assumed — `security find-identity -p codesigning -v` → 0
identities; `xcrun devicectl list devices` → none; `adb devices` →
emulator only; `~/.harbor-keys/` holds just the catalog root key and no
`HARBOR_ANDROID_KEYSTORE*` env exists; no Windows VM is installed;
notarytool has no stored profile. The full audit is recorded
commit-bound in `evidence/device_qualification.json`
(`session_29_external_prerequisite_audit`). Closing the eight blocked
gates requires a human to supply the resources and run the resume_with
paths; nothing machine-completable remains.

### Remaining external blockers (unchanged from rc1)

- Apple Developer identity + notarization (MAC-02, IOS-03)
- Physical iPhone/iPad (IOS-02), physical Android device(s) (AND-03)
- Play Console + upload key (AND-04)
- Windows machine (WIN-01) — compile-gated in CI in the meantime
- Minimum-spec Apple-silicon device (PERF-01)

### Reproduce the evidence

```bash
cd core && cargo test --workspace
cargo test -p harbor_inference --features gguf-backend
cargo test -p harbor_artifacts --test office_conformance
cargo test -p harbor_integration --test plaintext_at_rest_inspection
cargo run --release -p harbor_integration --example perf_baseline --features gguf-backend -- .. ../fixtures/models-store
cd ..
python3 tools/run_performance_qualification.py --write
python3 tools/generate_gate_evidence.py --write
python3 tools/validate_dossier.py --write
python3 tools/check_optional_disabled.py --write
python3 tools/generate_sbom.py --write
python3 tools/generate_third_party_notices.py --check
python3 tools/assemble_release_evidence.py --version 1.0.0-rc2 --write
cd apps/harbor_app && ~/harbor-tools/flutter/bin/flutter test
# real-network (unchanged since session 27):
cargo test -p harbor_modelhub --lib -- --ignored real_hf_capture --nocapture
```

### Environment notes (additions)

- The Flutter app now depends on `path_provider` + `file_selector`
  (pulled in via pub; plugins compile into the bundles automatically).
- Android cross-compile: stage the rebuilt
  `target/aarch64-linux-android/release/libharbor_ffi.so` into
  `apps/harbor_app/android/app/src/main/jniLibs/arm64-v8a/` BEFORE
  `flutter build` (recipe in the session-27 notes below).
- `dart`/`flutter` are NOT on PATH: use `~/harbor-tools/flutter/bin/...`.

---

## Historical session notes

*(ordered oldest → newest; the authoritative snapshot above supersedes
anything below)*

### Session 28 (2026-09-13; RELEASE CANDIDATE harbor-v1.0.0-rc1)

Superseded by the session-29 snapshot above; the rc1 state was: frozen RC
tag `harbor-v1.0.0-rc1` = 3b50001, gate report 11 PASS / 4
BLOCKED_EXTERNAL / 4 BLOCKED_DEVICE_EVIDENCE / 2 N/A_DISABLED / 0 FAIL,
iOS production-device static linkage, Android AAB + INTERNET permission,
Apple privacy manifests, store collateral, sealed evidence bundle,
Android §10 reduced suite + EN/AR screenshots, network capture rebound.
Only external blockers remained.

### Sessions 1–8 (2026-09-12, early)

- llama.cpp GGUF provider is real: `harbor_inference` feature `gguf-backend`
  pins `llama-cpp-2 =0.1.156` (vendored llama.cpp snapshot; decision 0002).
  Greedy deterministic decoding, cooperative cancellation, Metal execution.
- Flutter 3.47.4 at `~/harbor-tools/flutter`. `packages/harbor_ui` (Harbor
  Current 2 tokens, RTL parity, breakpoints §20, Harbor Rail/Trust Pulse/
  Run Trail/Harbor Sheet/Model Dock/Fit Score patterns); `apps/harbor_app`
  (9 surfaces, adaptive shell, EN/AR); `core/harbor_ffi` +
  `packages/harbor_native` single JSON-dispatch C ABI verified against the
  real dylib.
- Git initialized at `4f90f73`; CI workflow in place.
- Accessibility audit became a permanent test gate (semantic labels, 200%
  text scale, 44px targets, keyboard traversal); CycloneDX 1.5 SBOM tool;
  migration/rollback docs.
- Real-network acquisition through the Egress Broker (ureq+rustls, redirects
  DISABLED — every hop re-authorized): HfAcquirer with staged installs,
  hash verification, first-download identity; `models.search_hf` /
  `models.acquire_hf` FFI; signed catalog drives acquisition
  (epoch-protected per-file hashes; signed-but-wrong hash blocks install);
  streaming downloads with incremental SHA-256; retry-once for 429/503.

### Sessions 9–12 (2026-09-12)

- Qualified reference device bound:
  `fixtures/qualification/reference_device_macos_arm64.json` (Mac17,8 /
  M5 Pro / 24 GB / macOS 26.5.1, Metal 4);
  `qualified_device_manifest_sha256` bound in 26_Qualification_Profiles.
- Release-mode builds: macOS release app (ad-hoc signed) launched and quit
  cleanly; Android release APK (debug-key signed — store signing still
  requires credentials). Home composer drives the runtime
  (`run.log_request` under lease authority).
- Test tiers split: `cargo test --workspace` fully offline; real-network
  tests `#[ignore]`-gated.
- Key ceremony executed: release root key seed stored OUTSIDE the repo at
  `~/.harbor-keys/catalog-root.key` (0600); signed production catalog
  (epoch 1) binds qwen2.5-1.5b-instruct, bge-small-en-v1.5, stories260k
  with pinned hashes; permanent test verifies the committed signature.
- Performance baselines measured on the reference device (M5 Pro, Metal,
  Qwen2.5-1.5B Q4_K_M): load 153 ms p50, TTFT 53 ms p50, 148 tok/s, RAG
  ~21k docs/min; thermal 10-min sustained generation with no throttling.

### Sessions 13–16 (2026-09-12)

- harbor_sync: full 11_Sync_Protocol core (record envelopes, AEAD, hash
  chains, per-device sequence contiguity, epoch/revocation, type-enforced
  LWW, transfer handshake, 90-day horizon, signed snapshots, bundle
  transport with latest-per-object dedupe + tombstone propagation). Sync
  stays DISABLED; activation requires ACC-057.
- Office conformance deepening: DOCX headings/numbered lists/tables with
  gridSpan; XLSX merged-cell round trips; bar-chart machinery; DOCX
  TableCellSet merge-aware typed op; formatting-preserving same-length
  replacement across runs; XLSX chart round trip; PPTX DrawingML chart
  embedding with cached values + embedded workbook.
- SBOM commit versioning (git commit + build profile properties).
- PDF text extraction with page mapping (corrupt-input rejection tested);
  iOS simulator launch evidence (session 16).

### Session 26 (commit d3cdd61)

- `artifact.preview` DOCX dispatch fixed (was misrouted into the workbook
  branch); DocxPreview IR + Work Canvas rendering branches for docx/pdf;
  Dart FFI e2e test previews the real structured.docx fixture.

### Session 27 (commits de09abc → a8447cb)

- Reconciled stale dossier state (derived reports regenerated; manifest
  PASS restored at de09abc).
- Eight release-gap workstreams: Office matrix full fixture coverage;
  network-capture qualification; AEAD-sealed run events + plaintext
  inspection; performance thresholds v2 (cold/warm split); Windows build
  script + docs; packaging-to-credential-boundary scripts; optional-
  capability zero-surface proof. Office/eval clock fix (14367ae), corpus
  expansion to 464 cases (5e5a6bd).
- App bundles carry the live native core: macOS dylib in
  Contents/Frameworks (5468ca0), iOS simulator embedding build phase with
  live verified launch (d323bde).

### Session 28 (2026-09-13; this session)

- Release-candidate freeze work: iOS production-device static-archive
  embedding (symbol-verified), Android AAB + INTERNET permission review,
  Apple privacy manifests + export-compliance analysis, store collateral
  + third-party notices, §18 sealed evidence bundle + §20 gate report,
  post-packaging requalification (macOS live workspace, iOS simulator
  live core), all machine evidence regenerated at 1415e79. RC tagged
  `harbor-v1.0.0-rc1`. See the authoritative snapshot for full detail.
