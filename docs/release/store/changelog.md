# Harbor — Changelog

All notable changes are recorded per commit in the repository
(`git log`); this file summarizes user-visible milestones.

## 1.0.0-rc1 (2026-09-13)

Release candidate freeze. Product-complete implementation qualified for
release readiness; store-distribution signing and hardware-bound
qualification tiers pending external resources.

- Office Feature Matrix: all 23 rows carry executable conformance
  fixtures; compatibility classifier exposed through artifact preview.
- Independent network-capture qualification (broker audit vs. wire
  capture, 1:1) for offline, redirect-chain, blocked-origin, and strict
  Local Only scenarios; real-network acquisition proof through the HF →
  CDN chain.
- Run-event payloads AEAD-sealed at rest; full plaintext-at-rest byte
  inspection PASS.
- Performance thresholds v2 frozen (cold/warm split) after release-profile
  recalibration; qualification tooling gates fresh runs against the frozen
  file.
- macOS app bundle ships the live native core (dylib in Contents/Frameworks).
- iOS: simulator app runs the live native core; production device strategy
  implemented (static archive force-loaded into the Runner binary,
  sdk-conditional build phase) and symbol-link-verified.
- Android: release APK runs the live native core on arm64 (NDK
  cross-compile, llama.cpp included).
- Evaluation corpus expanded to 464 cases (116 EN / 116 AR / 232 mixed)
  with corpus hash rebound.
- TODAY/NOW formula determinism fix (timezone pinned in the qualification
  harness).
- Apple privacy manifests (app-level, symbol-evidence-based) and
  export-compliance declaration added to iOS/macOS bundles.
- Third-party notices generated from the locked dependency graph.

## Earlier (implementation milestones, summarized)

- M0 contract freeze; M1 local reference workflow (model install →
  workbook recalculation → analysis → deck → approval → safe save; replay
  at every transition); formula engine qualification (71 targets, pinned
  clock).
- Rust core (15 crates) + Flutter shell (EN/AR, accessibility gates,
  Harbor Current 2 design system) verified over a single JSON-dispatch FFI
  boundary.
- Signed catalog + key ceremony (root key outside the repository);
  brokered egress with hash-chained audit; staged installs with
  interruption safety.
- Durable run journal with replay; artifact commit semantics
  (deterministic ops, effect uncertainty, stale-write protection).
- harbor_sync protocol core complete and tested; feature remains disabled
  pending activation gates (ACC-022/023/057).
