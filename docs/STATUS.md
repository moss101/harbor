# Harbor — project status

This file has ONE authoritative snapshot (below) followed by historical
session notes. The snapshot is rewritten each session; history is append-only
context. Evidence files under `evidence/` bind to the exact git commit they
ran at.

---

## Authoritative snapshot — session 28 (2026-09-13; RELEASE CANDIDATE
## `harbor-v1.0.0-rc1`; commits this session: 1415e79 + the STATUS commit on
## top — run `git log --oneline -3`; all machine evidence binds to 1415e79,
## later commits touch docs/evidence only)

### Release-candidate state

Harbor v1.0.0-rc1 is **frozen**: every machine-completable qualification task
is done and the remaining non-PASS gates are exactly the external-dependency
gates recorded in `evidence/releases/1.0.0-rc1/release_gate_report.json`
(11 PASS / 4 BLOCKED_EXTERNAL / 4 BLOCKED_DEVICE_EVIDENCE / 2 N/A_DISABLED /
0 FAIL). `release_declared: false` — HARBOR v1 PRODUCTION RELEASE COMPLETE is
NOT declared until the external gates below close.

App identity: `version: 1.0.0+1` (Android versionName 1.0.0 / versionCode 1;
iOS CFBundleShortVersionString 1.0.0).

### Completed this session (release goal workstreams)

1. **iOS production-device embedding (goal §4 steps 1–5, machine-complete).**
   `libharbor_ffi.a` (aarch64-apple-ios, llama.cpp included) is force-loaded
   into the Runner binary: `OTHER_LDFLAGS[sdk=iphoneos*]` with `-force_load`
   + `-u _harbor_core_open/_call/_close/_string_free` retention roots (Dart
   FFI resolves at runtime, so without `-u` dead-strip removes the exports).
   Deterministic `Stage Harbor Native Archive` build phase (pre-link,
   iphoneos-only) builds the archive on demand and stages it; the Dart side
   (`HarborCoreClient.open`) falls back to `DynamicLibrary.process()` symbol
   lookup when the named library is absent. Accelerate (+Metal/MetalKit)
   frameworks linked (llama.cpp vDSP usage). Verified: release device build
   (codesign-free) links a 25.5 MB Runner containing all four FFI symbols.
   NOTE: xcodebuild upfront-validates the force_load input — on a fresh
   machine run `scripts/package_apple.sh` (or the cargo rustc command in the
   phase's error message) BEFORE the first Xcode build.
2. **Android release AAB (goal §9, machine part).** `app-release.aab`
   v1.0.0(1), arm64-v8a `libharbor_ffi.so` + `libc++_shared.so` + native
   debug symbols in BUNDLE-METADATA. INTERNET added as the app's ONLY
   permission (without it the authorized HF acquisition is impossible in
   release builds); rationale documented in AndroidManifest.xml.
3. **Apple privacy manifests + export compliance (goal §8).**
   `PrivacyInfo.xcprivacy` bundled on both platforms via the packaging path —
   declarations are symbol-evidence-based (FileTimestamp CA92.1 for the stat
   family, DiskSpace E174.1 for statfs; no tracking, no collected data; the
   Flutter engine ships its own manifest).
   `ITSAppUsesNonExemptEncryption=false` with the recorded analysis in
   `docs/release/store/apple_export_compliance.md` (operator confirms at
   submission).
4. **Post-packaging requalification (goal §10 analog).** macOS release bundle
   (dylib + privacy manifest, ad-hoc): codesign verify ok, dylib mapped,
   fresh temp workspace opened (agent.db+WAL/SHM+network_audit.db observed
   via lsof), clean quit. iOS simulator rebuilt through the changed phases
   and relaunched live: core mapped (lsof=1), LOCAL ONLY badge with real
   model state → `evidence/ios/live_core_screenshot_rc.png`. Both recorded in
   `evidence/device_qualification.json`
   (`post_packaging_requalification`). Simulator builds now use `--debug`
   (current Flutter rejects release/profile for simulators).
5. **Store & release collateral (goal §17).** `docs/release/store/`:
   product description, privacy statement, local-first explainer, device
   matrix, model compatibility (no "every HF model" claims), release notes +
   changelog, FAQ, security contact (GitHub advisories until an operator
   email is bound), model licenses, export-compliance analysis — EN+AR where
   stores require. `docs/release/THIRD_PARTY_NOTICES.md` generated from the
   locked dependency graph (405 entries; `tools/generate_third_party_notices.py
   --check` gates staleness).
6. **Sealed release evidence bundle (goal §18) + gate report (§20).**
   `evidence/releases/1.0.0-rc1/`: build_identity, git_commit, sbom (413
   components), qualification profiles, acceptance/security/performance/
   devices/network/office/accessibility/evaluation sections,
   signed_artifact_hashes + store_package_hashes (AAB, APK, macOS app zip,
   iOS device app zip — all honestly labeled ad-hoc/debug, NOT
   store-distributable), release_gate_report (25 gates; PASS emitted only
   when backing evidence exists at the same commit; assembler:
   `tools/assemble_release_evidence.py`).

### Test results (all regenerated at commit 1415e79)

| Suite | Result |
| --- | --- |
| `cargo test --workspace` (in gate bundle) | **PASS** (10/10 suites overall, `evidence/gate_results.json`, commit 1415e79, all_suites_ok=true) |
| gguf-backend / dossier / contracts / pin / contrast / flutter (19) / dart | PASS (components of the gate bundle) |
| Office conformance (23 matrix rows) | PASS (component of the gate bundle) |
| Plaintext-at-rest inspection | PASS (rerun at 1415e79 → `evidence/plaintext_at_rest.json`) |
| Optional capabilities disabled | N/A_DISABLED, zero violations |
| Performance qualification | PASS_WITH_BLOCKED_CLASSES (reference metrics all PASS: TTFT p95 ≤ 100, ≥ 100 tok/s, artifact open/recalc/save within thresholds, RAG 25 777 docs/min; min-device/Windows classes BLOCKED_DEVICE_EVIDENCE) |
| Network capture (offline + real-HF) | PASS evidence from session 27 (commit-identified inside `evidence/network_capture.json`; real-network rerun deliberately not repeated — HF egress behavior unchanged this session) |

### RC freeze rules now in force (goal §19)

Only release-blocker fixes, qualification fixes, packaging/signing fixes and
release documentation may land on top of the RC tag. Every such change
invalidates affected evidence and requires rerunning the relevant subset
(`docs/STATUS.md` "Reproduce the evidence" block) plus
`python3 tools/assemble_release_evidence.py --version 1.0.0-rc1 --write`.

### Remaining external blockers (nothing machine-completable left)

Each is a gate in `evidence/releases/1.0.0-rc1/release_gate_report.json`
with its `resume_with` path:

- **Apple Developer identity + notarization** (MAC-02, IOS-03): operator sets
  `HARBOR_APPLE_SIGNING_IDENTITY` and runs `scripts/package_apple.sh`;
  notarize via operator notarytool profile; TestFlight upload.
- **Physical iPhone/iPad** (IOS-02): attach device, `flutter run --release`
  (development signing acceptable), then execute the §4 steps 6–19 checklist
  (inference, artifact preview, durable run replay, background/foreground,
  Local Only, Arabic/RTL, VoiceOver, thermal) into a new device-tier record.
- **Physical Android device(s)** (AND-03): install the release APK/AAB via
  bundletool/adb, rerun launch + native core + inference + lifecycle +
  TalkBack + network capture on hardware.
- **Play Console + upload key** (AND-04): operator supplies
  `HARBOR_ANDROID_KEYSTORE*` env vars → `scripts/package_android.sh`
  produces the store-signed AAB; upload to internal testing; re-qualify the
  store-delivered artifact (§10).
- **Windows machine** (WIN-01): run `scripts/build_windows.ps1` +
  `docs/release/windows_qualification.md` runbook.
- **Minimum-spec Apple-silicon device** (PERF-01): run the performance
  protocol, freeze thresholds per class from measurement.

### Qualified right now

macOS Apple-silicon release (ad-hoc; launch/workspace/inference/perf bound),
iOS simulator live-core tier, Android arm64 emulator live-core tier, all
cross-platform machine gates (office/privacy/storage/evaluation/perf-reference/a11y-test-level),
production iOS linkage machine-verified. Physical-device tiers remain
BLOCKED_DEVICE_EVIDENCE — never promoted from simulator/emulator evidence.

### Reproduce the evidence

```bash
cd core && cargo test --workspace
cargo test -p harbor_inference --features gguf-backend
cargo test -p harbor_artifacts --test office_conformance
cargo test -p harbor_net --test net_capture_qualification
cargo test -p harbor_integration --test plaintext_at_rest_inspection
cargo run --release -p harbor_integration --example perf_baseline --features gguf-backend -- .. ../fixtures/models-store
cd ..
python3 tools/run_performance_qualification.py --write
python3 tools/generate_gate_evidence.py --write
python3 tools/validate_dossier.py --write   # after any file change
python3 tools/check_optional_disabled.py --write
python3 tools/generate_sbom.py --write
python3 tools/generate_third_party_notices.py --check
python3 tools/assemble_release_evidence.py --version 1.0.0-rc1 --write
# real-network (ignorable, unchanged since session 27):
cargo test -p harbor_modelhub --lib -- --ignored real_hf_capture --nocapture
```

### Environment notes

- Flutter SDK is NOT on PATH: use `~/harbor-tools/flutter/bin/flutter` and
  `~/harbor-tools/flutter/bin/cache/dart-sdk/bin/dart`.
- `rustc`/`cargo` on PATH are Homebrew builds (host target only). Use
  rustup shims (`PATH="$HOME/.cargo/bin:$PATH"`) for cross targets; NDK r27
  env for Android (see `evidence/device_qualification.json`).
- iOS device archive: `cargo rustc --crate-type staticlib` is required — a
  plain `cargo build` also builds the cdylib, whose bare link fails on
  `___chkstk_darwin` (compiler-rt); the app link provides it.
- `evidence/` is gitignored by policy; evidence files are filesystem state
  whose contents record their own commit bindings. The RC tag binds code;
  `docs/` binds the process.

---

## Historical session notes

*(ordered oldest → newest; the authoritative snapshot above supersedes
anything below)*

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
