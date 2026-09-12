# Release Qualification Checklist — machine-verifiable status sweep

Date: 2026-09-12 (session 6). Bound to git HEAD at time of writing. Evidence
references are commands runnable from the repository root; statuses are
honest: VERIFIED (executed here), PARTIAL (some evidence exists), or BLOCKED
(external input required — named).

## M0 — Contract freeze
- [x] **VERIFIED** Gate evaluator deterministic required-gate set:
      `python3 tools/validate_dossier.py` → PASS; `select_gates` in
      tools/contracts.py covered by `tools/test_contracts.py` (124/124).
- [x] **VERIFIED** Runtime/effect/artifact/network/storage/provider contracts
      schema-versioned with negative/semantic fixtures: contract suite green;
      Rust port in harbor_security/harbor_agent/harbor_artifacts (129→143 tests).
- [x] **PARTIAL** Office support matrix approved (21_Office_Feature_Matrix.csv
      is authority); device support manifest and qualification thresholds are
      BLOCKED on real lowest-spec devices (M2 measurement).

## M1 — Local reference workflow
- [x] **VERIFIED** Reference workflow (install → recalc → verify → deck →
      diff → approval → safe save): `cargo test -p harbor_integration`,
      `m1_deck_generation_diff_approval_safesave_offline` — fully offline.
      Device = this Mac (arm64); "declared reference device" formalization
      remains M2 paperwork.
- [x] **VERIFIED** Kill/restart/replay at run transitions:
      `harbor_agent/tests/durable_run.rs` (8 tests) + M1 blob/registry test.
- [x] **VERIFIED** Crash-window and stale-write fault injection:
      commit.rs recovery classification test + stale-base conflict test;
      tampered-event replay rejection in durable_run.rs.
- [x] **VERIFIED** Unsupported formulas never verified: qualification corpus
      (harbor_formula) reports TEXT as FAIL honestly; workbook preview marks
      provenance; RecalcStatus::UnsupportedDependency exists for callers.

## M2 — Four-platform beta
- [x] **PARTIAL** Build/package smoke: macOS debug app ✓, iOS simulator
      Runner.app ✓, Android debug APK ✓ (this machine). Windows: BLOCKED
      (no Windows toolchain here). Signed store packages: BLOCKED (Apple
      Developer certificate, Play Console account — human/credential input).
- [ ] **BLOCKED (partial)** Independent traffic capture vs broker log: the
      broker + hash-chained audit log exist with unit tests; an independent
      capture (packet-level) comparison harness is M2 work on real devices.
- [x] **PARTIAL** Plaintext-leak inspection: blob-store ciphertext-on-disk
      test passes (harbor_store); temp-window residue sweep tested; full
      leak inspection incl. SQLite/journal plaintext scan is M2 scope.
- [x] **PARTIAL** Office compatibility corpus: DOCX/PPTX/XLSX round-trip and
      precondition tests pass; full corpus per matrix row is incremental.
      Adaptive breakpoints: enforced by harbor_ui tests (contract §20) and
      app widget tests at 390/800/1280.
- [x] **PARTIAL** EN/AR UI + RTL: full arb parity + l10n coverage gate
      (harbor_app 14/14, includes RTL test); screen-reader/keyboard audit:
      NOT STARTED (needs VoiceOver/TalkBack passes). Semantic color contrast:
      tokens ship 4.5:1-compliant pairs from 06_Design_Tokens.json; unrounded
      contrast verification is a pending audit script.
- [x] **PARTIAL** Model package interruption/catalog rotation/provider
      absence: staged-install + incomplete-package tests pass; provider
      absence enforced by router tests; catalog rotation (monotonic epochs,
      signatures) NOT IMPLEMENTED yet (harbor_modelhub catalog is unsigned
      static data) — machine-doable next.

## M3 — Core GA
- [ ] **BLOCKED** Build-bound PASS for all applicable gates: blocked on M2
      device qualification + unset bindings (model_package_sha256,
      qualified_device_manifest_sha256) per 26_Qualification_Profiles.json.
- [ ] **BLOCKED** Performance thresholds from lowest qualified devices.
- [x] **PARTIAL** RAG gates: pinned EN/AR/mixed eval corpus (9 cases) passes
      (retrieval, abstention, injection) — `cargo test -p harbor_knowledge`;
      index migration and source-revocation persistence are implemented at
      the model level (removal/changed states) with tests.
- [ ] **BLOCKED** Store/notarization/privacy declarations: needs signed
      distribution.
- [ ] **NOT STARTED** SBOM, migration N-2, rollback, last-known-good catalog
      docs (machine-doable once packaging lands).

## M4 — Optional services
All N/A for the M1/M2 target; none may inherit GA. Not started (correct —
they must not leave dormant authority paths).

## Corrected package checks
- [x] **VERIFIED** validate_dossier.py PASS (manifest regenerated after every
      contract change); contract suite 124/124; engine pin check ok.
- [x] **VERIFIED** Target gate lists/dependencies/disabled paths: covered by
      contract tests (ACC-064 disabled-path rules).
- [x] **VERIFIED** Qualification fixtures bound: formula engine
      (crates.io/formualizer@0.9.3, integrity 64b7771c…), adapter
      (harbor_formula/1), fixture bundle, evaluation corpus (9e2a7e4d…).
      Still BLOCKED/unset: model_package_sha256,
      qualified_device_manifest_sha256 (need the qualified device + model run).
- [x] **VERIFIED** External overwrite: qualified safe-commit with base
      revalidation + New Copy fallback + crash recovery (harbor_artifacts
      commit tests).
- [ ] Word authority rebuilds after contract changes: not applicable this
      session (no authority edits; decision records are the recorded changes).
