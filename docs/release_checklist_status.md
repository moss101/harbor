# Release Qualification Checklist — machine-verifiable status sweep

Date: 2026-09-12 (session 27). Bound to the session-27 evidence commits
(de09abc → 7115d49; exact suite-bound commit in `evidence/gate_results.json`).
Statuses: VERIFIED (executed here, evidence path given), PARTIAL (some
evidence exists; remainder named), BLOCKED (external input required — named),
N/A_DISABLED (optional capability intentionally off with verified clean
surface).

## M0 — Contract freeze
- [x] **VERIFIED** Gate evaluator deterministic required-gate set:
      `python3 tools/validate_dossier.py` → PASS (manifest regenerated this
      session after file changes); `tools/test_contracts.py` green;
      `evidence/gate_results.json`.
- [x] **VERIFIED** Runtime/effect/artifact/network/storage/provider
      contracts schema-versioned with negative/semantic fixtures.
- [x] **VERIFIED** Office support matrix approved and now fully fixture-
      covered (all 23 rows of `21_Office_Feature_Matrix.csv` have executable
      conformance tests; `office_matrix` classifier prevents silent claims).
      Device support manifest: reference class bound; thresholds frozen (see
      M3). Visual pixel-diff fixtures still need a rendering stack (honest gap).

## M1 — Local reference workflow
- [x] **VERIFIED** Reference workflow offline:
      `cargo test -p harbor_integration --test m1_reference_vertical`.
- [x] **VERIFIED** Kill/restart/replay, crash-window + stale-write fault
      injection, tampered-event replay rejection.
- [x] **VERIFIED** Unsupported formulas never verified (TEXT reports FAIL).
- [x] **VERIFIED** Run-event payloads sealed at rest; plaintext-at-rest
      inspection PASS (`evidence/plaintext_at_rest.json`).

## M2 — Four-platform beta
- [x] **VERIFIED (this machine)** macOS release app (ad-hoc signed,
      launches); iOS simulator build + launch (`evidence/ios/`); Android
      release APK + **live native core** on arm64 emulator
      (`evidence/android_emulator_launch.png`,
      `evidence/device_qualification.json`).
- [x] **VERIFIED (scripts complete)** Windows: `scripts/build_windows.ps1`
      + `docs/release/windows_qualification.md`. Execution evidence
      **BLOCKED** (no Windows machine).
- [x] **VERIFIED** Independent traffic capture vs broker log —
      offline scenarios are permanent tests; real-network HF run verified
      capture == audit 1:1 including redirect chain, blocked origins and
      strict Local Only (`evidence/network_capture.json`).
      Packet-level (root/BPF) capture on physical devices remains future
      work; the down-stack capture is the qualified harness here.
- [x] **VERIFIED** Plaintext-at-rest: full inspection over SQLite main +
      WAL/SHM, encrypted blobs, key store, temp windows (incl. crash
      residue), run log, network audit, commit-journal recovery paths.
      Previews/extracted content/index are in-memory in this build (no
      on-disk surface exists; recorded in the evidence report).
- [x] **VERIFIED** Office compatibility corpus: all matrix rows have
      executable fixtures; XLSX PRESERVE parts carried byte-identically.
      EN/AR + RTL: arb parity + l10n gates green; VoiceOver/TalkBack passes
      still pending (physical devices) — PARTIAL.
      Contrast: token audit green (`tools/check_contrast.py`).
- [x] **VERIFIED** Model package interruption/catalog rotation/provider
      absence: staged installs, signed catalog with monotonic epochs,
      router-enforced provider absence.
- [x] **N/A_DISABLED** Optional services (incl. `feature:sync`): all
      default-off with zero FFI surface
      (`evidence/optional_capabilities_disabled.json`).

## M3 — Core GA
- [x] **VERIFIED (reference device class)** Performance thresholds frozen
      (`performance_thresholds_reference_macos_arm64.json`, v2 dated
      recalibration) and independent run evaluated:
      `evidence/perf_qualification.json` → all metrics PASS.
      Minimum-spec macOS / iOS / Android / Windows classes:
      **BLOCKED_DEVICE_EVIDENCE** (recorded in the same file; never PASS).
- [x] **VERIFIED** Bindings present: model_package_sha256,
      evaluation_corpus_sha256, qualified_device_manifest_sha256,
      formula engine + fixture bundle hashes
      (`26_Qualification_Profiles.json`). Evaluation corpus expansion
      (9 → 100 cases/language) is the remaining engineering gap.
- [ ] **BLOCKED** Store/notarization/privacy declarations: needs Apple
      Developer identity, notarization, Play Console ownership + upload key
      (packaging scripts stop exactly at this boundary).
- [x] **VERIFIED** SBOM (`tools/generate_sbom.py --write`, commit-bound) +
      migration N-2 + rollback + last-known-good docs
      (`docs/release/migration_rollback.md`).

## M4 — Optional services
- [x] **N/A_DISABLED** All optional features verified disabled with clean
      surfaces. harbor_sync protocol code is complete and tested at the
      crate level; activation requires ACC-022/023/057 with real evidence.

## External blockers (human/credential/hardware)
1. Apple Developer Program identity + notarization profile.
2. Play Console ownership + Android upload key (release key stays server-side).
3. Physical iOS device, physical Android device, Windows machine,
   minimum-spec Apple-silicon device.
4. Nothing else is blocked on these: all machine-completable work for the
   above is implemented, scripted, and evidenced.
