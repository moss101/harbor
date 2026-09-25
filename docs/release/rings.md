# Harbor 1.1 — ring release plan (production plan phase D)

Three rings, each with a go/no-go rule that reads the **gate report**
produced by `tools/assemble_release_evidence.py` (`evidence/releases/<version>/acceptance_results.json`,
goal §20 taxonomy: `PASS`, `FAIL`, `FAIL_NO_EVIDENCE`, `BLOCKED_EXTERNAL`,
`BLOCKED_DEVICE_EVIDENCE`, `N/A_DISABLED`, `N/A_PLATFORM`). Nothing in a
ring is a judgement call: a ring opens when its rule holds for the platforms
being shipped, and closes when the next ring's rule holds. The engineering
prerequisites (phases A–C) are done and recorded in `docs/STATUS.md`; the
operator prerequisites are the eight resources in `closeout_runbook.md`
(procurement should already be under way — lead times are days to weeks).

The rings ship **1.1.0** (the skills product: nine runnable graphs, safe
commit, proposal diff, diagnostics export), not 1.0.0: the 1.0.0-rc2 bundle
is the baseline every ring measures against.

## Ring 0 — internal (week 6)

Purpose: close the operator gates on real hardware and freeze the
performance thresholds. Everything runs from the tagged commit
`harbor-v1.1.0-rc1`.

1. Tag and let CI produce the evidence bundle and the unsigned artifacts
   (`.github/workflows/release.yml`); do **not** release the draft. That
   bundle is **partial** (`bundle_completeness: "partial"`): a GitHub
   runner cannot produce the qualification-machine-local evidence, so
   X-07 (network capture) and X-08 (performance) read `FAIL_NO_EVIDENCE`
   in it. It is a build-bound archive of everything CI *can* regenerate,
   never the input to a go/no-go rule.

   If that job fails on the app suite, read the failing suite's
   `output_tail` in `gate_results.json` (uploaded even when the job
   fails) before doing anything else, and re-run nothing until you have.
   The one intermittent failure this suite has had — a live-core widget
   test that stalled on a loaded runner — was a fake-async timer in the
   test's own wait loop, fixed in session 40, and it took three
   diagnoses to get right because each earlier one was inferred from the
   UI rather than read off the evidence. The timeout now prints both the
   tree AND the core's own op state, which is what distinguishes a stuck
   core from a test that stopped asking.
2. Per gate, follow `closeout_runbook.md`:
   - MAC-02 / IOS-03: sign, notarize, staple; TestFlight internal group.
   - IOS-02: physical iPhone, §4 steps 6–19; record device-tier evidence.
   - AND-03 / AND-04: release APK on both handsets, §10 checklist;
     store-signed AAB to Play internal testing.
   - WIN-01: build smoke on the Windows host.
   - PERF-01: on the M1 8 GB machine,
     `tools/run_performance_qualification.py --write --measured-class
     minimum_spec_macos_arm64`. The `--measured-class` flag is what closes
     the gate: it records `evidence/devices/<class>.json` with this host's
     model, CPU and memory, and the class stays blocked until that file
     exists. Run it ON the hardware — the recorded host facts are the
     audit trail, and a class attested from the wrong machine is visible
     in the evidence. Then replace every `TBD` in
     `15_Performance_Qualification.yaml` (status `BLOCKED_UNTIL_MEASURED`)
     with the measured p95 per class — thresholds are frozen from here.
   - IOS-02 / AND-03 / WIN-01 close the same way, each with its own
     `--measured-class` on the machine that did the measuring.
3. Regenerate all evidence at the frozen commit and re-assemble the bundle
   on the qualification machine, **without** `--partial`:
   `python3 tools/assemble_release_evidence.py --version 1.1.0-rc1 --write`.
   This is the authoritative bundle; it refuses to assemble cleanly while
   any gate is `FAIL` (evidence exists and says the check did not pass) or
   `FAIL_NO_EVIDENCE` (there is no evidence to read).

**Go/no-go for ring 1** — run the rule, do not read it:

```
python3 tools/check_ring_gate.py \
    --report evidence/releases/1.1.0-rc1/release_gate_report.json \
    --ring 1 --platforms mac,ios,android
```

It prints each clause below with the gate ids that violate it and exits
non-zero on NO-GO. The clauses are:

| Field | Rule |
| --- | --- |
| `FAIL` | 0 |
| `FAIL_NO_EVIDENCE` | 0 |
| `BLOCKED_EXTERNAL` | 0 for every platform in the ring-1 set |
| `BLOCKED_DEVICE_EVIDENCE` | 0 for every platform in the ring-1 set |
| remaining non-`PASS` | only `N/A_DISABLED` (M4 services) or `N/A_PLATFORM` |
| `bundle_completeness` | `complete` (a `partial` bundle never decides a ring) |

A platform whose gates are still blocked is dropped from the ring-1 set
(Windows is the expected drop if WIN-01 slips); it does not block the
others — that is what `--platforms` names. Cross-platform gates (`X-*`,
`PERF-01`, `SYNC-01`, `OPT-01`) count against every set and cannot be
dropped.

## Ring 1 — closed beta (weeks 7–8)

Purpose: 20–50 strangers use the skills product on their own documents.

- Channels: TestFlight external group (iOS), Play closed track (Android),
  notarized DMG link (macOS); Windows only if WIN-01 passed in ring 0.
- Tester brief (one page): install a model from **Models → Recommended**
  (the first-run card leads there), run one skill on a document of their
  own, review the diff, **Save new copy**, then **Settings → Diagnostics →
  Export diagnostics** and send the zip. The export contains redacted
  error records and build facts and never document content
  (`docs/PRODUCTION_PLAN.md` §C1; `plaintext_at_rest_inspection` proves it).
- Tracked per tester from the export's `diagnostics.json` and
  `records.jsonl`: install success, first model install success
  (`runs_by_state`, `installed_models`), first skill commit success
  (`run.effect_resolved` counts in the records), crashes per session
  (`levels.panic`).
- **Fix-only rule**: no new graphs, no schema changes, no catalog epoch
  bump. Every fix re-tags (`rc2`, `rc3`, …) and every tag gets a full
  evidence bundle from the release workflow; the replay tier (23 cases)
  and the live tier number are re-recorded on the qualification machine
  before each re-tag.
- Known tester notes to include: after a rebuilt debug/TestFlight build,
  macOS may show a Keychain "Allow" prompt for the device root key once
  (signed builds fix this); low disk during model install aborts cleanly
  and removes the staged download (verified in
  `harbor_modelhub::acquire::staging_cleanup_tests`).

**Go/no-go for GA** — `--ring ga` over the last rc bundle, plus the
exports. The tool decides the gate-report clauses and prints the three
human ones as `MAN` items with the artifact to read. It never returns GO
while one is unconfirmed; the operator confirms each by name once it has
actually been checked, which puts the assertion in the shell history and
the runbook instead of in someone's head:

```
python3 tools/check_ring_gate.py \
    --report evidence/releases/1.1.0/release_gate_report.json \
    --ring ga --platforms mac,ios,android \
    --confirmed crashes,checklist,evals
```


| Check | Rule |
| --- | --- |
| Crash reports attributable to Harbor code | 0 across two consecutive beta builds (`levels.panic == 0` in every export; `core`/`ffi` records reviewed) |
| M3 checklist (`10_Release_Checklist.md`) | every item checked with build-bound evidence in the bundle |
| Gate report | same rule as ring 0 for the GA platform set |
| Live-tier skill evals | recorded for the rc (number stated, not promised) |
| `release_declared` | `true`. The assembler computes this: no `FAIL*`, no `BLOCKED_*`, `bundle_completeness: complete`. A platform Harbor is not shipping must therefore be recorded `N/A_PLATFORM` by its gate — dropping it from a ring set is not the same as declaring it out of scope. |

## Ring 2 — GA (week 9)

- Tag `harbor-v1.1.0`; the release workflow assembles the final bundle;
  `docs/STATUS.md` authoritative snapshot updated with the commit hash;
  rc bundles archived under `evidence/releases/`.
- App Store review submission with the privacy manifests already in the
  repository; Play production rollout **10 % → 50 % → 100 %** over a week,
  halting at the first Harbor-attributable crash report; macOS DMG on the
  release page.
- Rollback: `docs/release/migration_rollback.md`. The catalog stays at
  epoch 1 (no model tier was added in B4); an epoch bump, if one is ever
  needed, is signed with the offline root key **before** GA, never after.

## What the operator must bring (unchanged)

Apple Developer Program membership, Play Console account + upload key, one
iPhone (A15+, 6 GB) and one iPad, two Android handsets, a Windows 10/11 x64
host, an M1 8 GB Mac. None of these can be substituted by the engineering
track; the gate report will say `BLOCKED_EXTERNAL` /
`BLOCKED_DEVICE_EVIDENCE` until they exist.
