# Decision 0001 — Formula engine pin (HBR-153)

Date: 2026-09-12
Status: Accepted (revision + source pin); integrity hash recorded by tools/pin_engine.py.

## Problem
`22_Formula_Coverage.json` names `Formualizer` as the selected engine family and requires
`production_source_revision` and `production_integrity_hash` to be pinned (HBR-153) before
any formula target can be qualified.

## Decision
- Pin `formualizer = "0.9"` (resolved to 0.9.3 in core/Cargo.lock) via crates.io.
- Sub-crate resolution is locked by Cargo.lock (formualizer-common 3.1.2 and friends).
- Engine identity is recorded in `core/harbor_formula/src/engine.rs` (`ENGINE`).
- `tools/pin_engine.py` computes the integrity hash over the exact packed crate archives
  and rewrites the placeholder; CI re-runs it in `--check` mode.

## License review (required by authority)
Formualizer and its workspace sub-crates are dual-licensed MIT OR Apache-2.0. Approved for
Harbor (Apache-2.0) with attribution in the distribution license inventory.

## Qualification status (honest)
- 100-case corpus across all 71 authority targets: 99 PASS / 1 FAIL.
- Known deviation: `TEXT(0.285,"0.0%")` returns "28%" (truncation) instead of Excel's
  "28.5%" (round-half-away). The `TEXT` target therefore remains REQUIRED_UNQUALIFIED on
  this revision; the corpus documents it and gates any engine upgrade.
- Dates below 1900-03-01 are excluded from the qualified set (Excel 1900 leap-bug zone);
  see the corpus header.

## Consequences
- Any engine bump invalidates `qualification_results` and requires a full corpus re-run.
- Qualification reports stamp engine family/version/source revision/corpus bundle hash.

## Addendum (2026-09-29, session 47): TEXT percent patch

The known TEXT deviation is fixed by a Harbor-maintained patch:
`third_party/formualizer-eval` is the exact 0.9.3 crates.io source with
one change — `format_percent` now parses the decimal count from the
format code and rounds half-away-from-zero with a binary-error correction
(upstream hard-coded `{:.0}%`, which ignored `"0.0%"`'s decimal count and
truncated 0.285 → 28% via float error). Wired through
`[patch.crates-io]` in core/Cargo.toml.

Consequences:
- The engine identity is now `0.9.3+harbor-textfix`; the integrity hash
  covers the patched SOURCE TREE (deterministic file-order hash) plus the
  packed archives of the other pinned crates — `tools/pin_engine.py`
  implements both branches.
- Per this decision's own rule, the full corpus re-ran on every
  available tier after the patch: 139/139 cases, 99/99 targets on macOS,
  iOS Simulator and Android emulator (evidence/formula_evals/, session 47).
- Upstream remains MIT OR Apache-2.0; the vendored tree keeps both
  LICENSE files. If upstream ships the fix, prefer the registry release
  and drop the patch (re-pin + full re-run required either way).
