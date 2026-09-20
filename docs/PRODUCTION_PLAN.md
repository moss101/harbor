# Harbor — enhancement and production push plan

Written 2026-09-19 against working tree at `603b149` + uncommitted session-32
work. Companion to `docs/STATUS.md` (what exists), `10_Release_Checklist.md`
(what qualifies a release) and `docs/release/closeout_runbook.md` (the eight
operator-blocked gates).

## 0. Where we stand (verified, not assumed)

| Fact | Evidence |
| --- | --- |
| Release candidate `1.0.0-rc2` exists; all machine suites green at rc2 | `evidence/releases/1.0.0-rc2`, `evidence/gate_results.json` |
| Eight gates blocked only on operator resources (Apple identity, Play key, devices, Windows host, min-spec Mac) | `docs/release/closeout_runbook.md` |
| Session 32 (skill graphs, tool layer, executor, eval harness) is **uncommitted**: 38 modified + 19 untracked files | `git status` |
| Local `main` is 1 commit ahead of `origin/main` (session-30 UI overhaul, unpushed) | `git status -sb` |
| Last CI run on `origin/main` **failed** the `dossier` job (manifest seal stale after the runbook commit); local validator now PASS | run `34879836465`; `tools/validate_dossier.py` |
| Skills: 30 built-in, 4 runnable graphs, 26 declarations only | `builtin_skills.json`, decision 0006 |
| Live-tier skill evals 6/11 on Qwen2.5-1.5B (replay 11/11) | `evidence/skill_evals/` |
| Skill proposals cannot yet be applied to a document (no safe-commit through FFI/UI, no proposal diff in Work) | decision 0006 "Not done" |
| Agents surface disabled; sync, remote inference, connectors, diagnostics upload disabled (M4) | feature registry, `25_Feature_Registry.json` |
| No dependency-vulnerability gate (`cargo audit` / `cargo deny`) and no tag-driven release workflow in CI | `.github/workflows/ci.yml` |
| No integration tests on real emulator/simulator in CI; device evidence is manual | `apps/harbor_app/test`, CI |

Strategy in one line: **stabilise and land what exists (week 1), make the
skills product actually useful (weeks 2–4), harden for strangers using it
(weeks 4–6), then close the operator gates and ship in rings (weeks 6–9).**
Nothing in phases A–C depends on the blocked gates, so they run in parallel
with procuring the resources in phase D.

Two parallel tracks from day 1:

- **Engineering track** (this machine, machine-verifiable): phases A–C.
- **Operator track** (needs a human with accounts/hardware): phase D
  prerequisites. Start procurement on day 1 because lead times (Apple
  Developer approval, Play Console verification, device purchase) are days
  to weeks.

---

## Phase A — Land and stabilise (days 1–3)

Goal: `origin/main` green, session 32 committed, one clean baseline every
later phase measures against.

### A1. Commit session 32 in reviewable slices
```bash
cd core && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd ../apps/harbor_app && ~/harbor-tools/flutter/bin/flutter analyze && ~/harbor-tools/flutter/bin/flutter test
cd ../.. && python3 tools/validate_dossier.py
```
Then commit in this order so each slice is bisectable:
1. `schemas/graph.schema.json`, `schemas/run_event.schema.json`, dossier
   derived files (19/20/24/28) — "contracts: harbor.graph/v1 + run-event step branches".
2. `core/harbor_core/src/{graph,jsonschema,pointer,tools,executor,harness}.rs`
   + tests — "core: graph executor, tool registry, eval harness".
3. `core/harbor_inference` cassette + grammar probe — "inference: cassette provider + structured output".
4. `core/harbor_core/src/graphs/`, `builtin_skills.json`, `evals/skills/`,
   fixtures, `tools/make_skill_eval_fixtures.py` — "skills: four graphs + eval suites".
5. FFI + Flutter (`harbor_ffi/src/lib.rs`, `skill_run.dart`, `skills_surface.dart`,
   l10n) — "app: skill run sheet".
6. `docs/decisions/0005`, `0006`, `docs/STATUS.md`.

Exit: `git status` clean, `git push`, CI run green on all four jobs.

### A2. Fix the dossier CI failure permanently
The seal broke because a docs commit did not regenerate derived files. Add a
CI step that runs `python3 tools/validate_dossier.py --write` and fails on
`git diff --exit-code` for files 19/20/24, so a stale seal is reported as
"regenerate", not as an opaque FAIL. (~1 h)

### A3. Add the missing hygiene gates to CI (~half day)
- `cargo audit` (advisory DB) and `cargo deny check licenses bans` with a
  `core/deny.toml` that encodes the notices policy already in
  `docs/release/THIRD_PARTY_NOTICES.md`.
- `flutter pub outdated --mode=null-safety` report (non-blocking) and
  `dart pub audit` if available in the pinned SDK.
- Cache `target/` and pub caches; current jobs take ~5 min, keep it there.

### A4. Executor kill/restart test (decision 0006 open item, ~half day)
`Executor::resume` exists but only its wrong-state refusal is tested. Add a
test in `core/harbor_core/tests/` that starts a graph run, drops the
executor mid-node (simulated crash), reopens the log, resumes, and asserts
the node trail + io hashes match the uninterrupted run. This is an M1
checklist item ("kill/restart/replay at every run transition") now that
skills are runnable.

Exit criteria for phase A: CI green on pushed `main`; `cargo audit` clean or
every advisory recorded with a decision; resume test in the workspace suite.

**Status (2026-09-19):** A1 landed as six slices (order: contracts, inference,
core, skills, app, docs — inference before core because the executor tests
replay through the cassette provider). A2 `tools/check_dossier_seal.py`,
A3 `supply-chain` job + `core/deny.toml` + `core/.cargo/audit.toml`, A4
`core/harbor_core/tests/executor_resume.rs` — all in commit history; see
`docs/STATUS.md` session 33 for the gate results.

---

## Phase B — Make skills the product (weeks 2–4)

Goal: a user can run a skill against their own workbook or document, see
the proposed change as a diff, approve it, and get a safely committed file.
Today the loop stops at "proposal". This is the largest user-facing gap and
the reason to ship 1.1 rather than 1.0 to the public.

### B1. Safe-commit through FFI and UI (week 2)
- Core: `harbor_artifacts::commit::SafeCommitter` already exists. Expose
  `run.commit_proposal { run_id, batch_id, target: SaveNewCopy | Overwrite }`
  over FFI under the same lease/approval authority as `run.decide`.
- Default to **Save New Copy** (checklist: "verify external overwrite
  adapters or keep Save New Copy"). Overwrite stays behind the existing
  journal recovery classification.
- App: `HarborService.commitProposal`, worker-isolate plumbing (same pattern
  as `op.start_*`), and a commit step in `skill_run.dart` after approval.
- Tests: FFI test in `core/harbor_ffi/tests/` (commit → file on disk →
  journal clean → run event `ArtifactCommitted`); Flutter shell test on the
  live core using `fixtures/office/formula_errors.xlsx`.

### B2. Proposal diff in the Work surface (week 2–3)
- Reuse `harbor_ui` diff components (they exist from session 30) to render
  the proposal batch: cell-level for XLSX, run-level for DOCX, slide-level
  for PPTX.
- Entry points: Run detail → "Review changes"; Skill run sheet → after the
  approval node.
- Accessibility gate must pass on the diff view at 200 % text on 320 px.

**Status (2026-09-20):** B1 and B2 done — `Executor::decide_and_commit`,
FFI `run.commit_proposal`, `HarborService.commitProposal`, Run sheet with
`ArtifactDiffView` and Save new copy / Overwrite original… / Reject; full
journey in `shell_test.dart` on the live core. See `docs/STATUS.md` session 34.
The diff is cell-level for XLSX and paragraph-level for DOCX (the batch
model has no run-level DOCX ops); PPTX slide ops carry target + after only.

### B3. Decompose the highest-value remaining skills into graphs (week 3–4)
Not all 26. Pick by user value and fixture availability:
1. Document Co-Authoring
2. Deck Review & QA
3. Financial Model Review
4. Team Update
5. Document Style Review

Each: graph in `core/harbor_core/src/graphs/`, eval suite under
`evals/skills/`, replay tier green in CI, live tier recorded in
`evidence/skill_evals/`. Keep the remaining 21 as honest "Declaration only"
entries; the Skills surface already labels them.

**Status (2026-09-20):** B3 done — nine runnable graphs (`deck-review`,
`financial-model-review`, `document-style-review`, `team-update`,
`doc-coauthoring` added), replay tier 23/23 in CI, live tier recorded
(`evidence/skill_evals/live-6a1a2eb6d156.json`). See STATUS session 35.

### B4. Raise the live-tier pass rate (ongoing in weeks 2–4)
Current 6/11 on Qwen2.5-1.5B, every failure a pinned model fact contained
by a deterministic node. Options, in cost order:
1. Tighten structured-output schemas and node prompts (free).
2. Move classification/extraction work from the model into deterministic
   tool nodes where the eval shows the model is the weak link (cheap).
3. Add a second catalog tier (e.g. Qwen2.5-3B or 7B Q4) under decision
   0004's "catalog-upgradable" clause, gated by Fit Score on the device.
   Requires a new `model_package_sha256` binding, perf re-qualification
   and a signed catalog epoch bump.
Target: ≥ 9/11 on the reference device before 1.1 freeze; record the
number, do not promise it.

**Status (2026-09-20):** options 1 and 2 applied to the new graphs (schemas
sized against `max_tokens`, prompts copy-not-describe, fix/review decisions
and formula attachment moved into deterministic tools, guard cases marked
replay-only). Live tier on Qwen2.5-1.5B: **13/18** across nine skills (72 %,
up from 6/11 = 55 % across four). The five remaining failures are pinned
model facts (decision 0006 addendum). Option 3 (a larger catalog tier) is
not taken in this phase; it stays the documented next lever.

### B5. SKILL.md frontmatter importer (HBR-072, week 4, optional)
Import community `SKILL.md` files as `harbor.skill/v1` declarations, never
as executable graphs (closed tool catalog stays closed). Only if B1–B3 are
done; otherwise defer to 1.2.

Exit criteria for phase B: full journey on the live core in `shell_test.dart`
(open fixture → run Formula Audit & Repair → review diff → approve → Save
New Copy → reopen and verify); nine runnable graphs; replay tier 100 %.

---

## Phase C — Production hardening (weeks 4–6, overlaps B)

Goal: the app behaves well in the hands of people who are not the author.

### C1. Diagnostics without telemetry
Harbor's contract is local-only. Production support still needs a way for a
user to hand over evidence:
- Local rolling crash/error log (Rust `panic` hook + Dart `FlutterError` +
  `PlatformDispatcher.onError`) written under the data root, encrypted with
  the workspace key like run events.
- Settings → "Export diagnostics": produces a zip with logs, device profile,
  installed model ids, and gate-relevant versions. **No document content,
  no chunks, no prompts.** Add it to the plaintext-at-rest inspection so the
  export path is byte-scanned like everything else.
- "Diagnostics upload" remains N/A_DISABLED (M4).

**Status (2026-09-20):** C1 done — `harbor_core::diagnostics`, FFI
`diag.*`, `DiagnosticsSink`, Settings → Export diagnostics, inspection
extended (STATUS session 36).

### C2. First-run and empty states
- First launch: model install is the gate to everything. Make the Home
  empty state drive the user to Models → Recommended with size + Fit Score,
  and show Local Only status clearly.
- Recovery UX for the known failure modes: missing native symbol (the
  degraded banner exists), keychain Allow prompt after rebuild (signed
  builds fix this; document for TestFlight testers), low disk during
  acquisition (verify the brokered download aborts cleanly and the staged
  install is removed).

### C3. Performance and memory on the floor device
- Run `tools/run_performance_qualification.py` on the min-spec Mac as soon
  as PERF-01 hardware arrives (phase D); until then run it on the reference
  device under `memory_pressure` simulation and record the p95, not the p50.
- iOS: 1.5B Q4_K_M plus KV cache on a 6 GB iPhone is tight. Add a Fit
  Score refusal (not a warning) below the measured floor.

### C4. Security review before GA
- Run `/security-review` on the whole branch and the `harbor_ffi` boundary
  specifically (JSON dispatch is the attack surface).
- Fuzz the FFI dispatcher and `jsonschema.rs` (`cargo fuzz`, 1 h each
  target, corpus committed under `core/fuzz/`).
- Re-run `plaintext_at_rest_inspection` and `network_capture` at the freeze
  commit.
- Independent review: if budget allows, a short external pen test focused
  on keystore handling and the egress broker. Record the report under
  `evidence/security/`.

### C5. Release automation
- `.github/workflows/release.yml` triggered on `harbor-v*` tags: builds
  macOS app (unsigned artifact), Android APK/AAB (unsigned), runs
  `assemble_release_evidence.py`, uploads the bundle as a release asset.
  Signing stays on the operator machine per the credential boundary
  scripts.
- `CHANGELOG.md` generated from conventional commit prefixes already in
  use (`core:`, `app+ui:`, `docs:`, `ci:`).
- Bump `apps/harbor_app/pubspec.yaml` to `1.1.0+2` at phase-B freeze.

Exit criteria for phase C: diagnostics export in the plaintext inspection;
fuzz targets in CI (short run); release workflow produces a bundle from a
tag; security review findings triaged with none open at "high".

---

## Phase D — Close the operator gates and ship in rings (weeks 6–9)

Everything here is in `docs/release/closeout_runbook.md`; this section only
adds ordering, ring structure and go/no-go rules.

### D0. Procurement (start day 1, in parallel)
| Resource | Unblocks | Lead time |
| --- | --- | --- |
| Apple Developer Program (individual or org) | MAC-02, IOS-03 | 1–7 days |
| Play Console account + operator-generated upload key | AND-04 | 1–3 days |
| One iPhone (A15 or newer, 6 GB) and one iPad | IOS-02 | purchase |
| Two Android handsets (one 6 GB mid-range, one flagship) | AND-03 | purchase |
| Windows 10/11 x64 host (a VM on Apple silicon is acceptable for build smoke, physical preferred) | WIN-01 | days |
| M1 8 GB Mac | PERF-01, MAC-03 | purchase/borrow |

### D1. Ring 0 — internal (week 6)
- Sign macOS with Developer ID, notarize, staple (MAC-02).
- iOS device build and the §4 steps 6–19 checklist on the physical iPhone
  (IOS-02); TestFlight internal group (IOS-03).
- Android release APK on physical handsets, §10 checklist (AND-03);
  store-signed AAB to Play internal testing (AND-04).
- Windows build smoke on the host (WIN-01).
- Performance protocol on the M1 8 GB (PERF-01) → freeze per-class
  thresholds in `15_Performance_Qualification.yaml`, no TBD left.
- Regenerate all evidence at the freeze commit, assemble `1.1.0-rc1`.

Go/no-go for ring 1: gate report shows 0 FAIL, 0 BLOCKED_EXTERNAL,
0 BLOCKED_DEVICE_EVIDENCE for the platforms being shipped; every remaining
N/A is N/A_DISABLED or N/A_PLATFORM.

### D2. Ring 1 — closed beta (weeks 7–8)
- 20–50 testers across iOS (TestFlight external), Android (closed track),
  macOS (notarized DMG link). Windows only if WIN-01 passed.
- Ask each tester for the diagnostics export (C1) after their first skill
  run. Track: install success, first model install success, first skill
  commit success, crashes per session.
- Fix-only window; no new graphs. Re-tag `rc2`, `rc3` as needed; every tag
  gets a full evidence bundle.

Go/no-go for GA: two consecutive beta builds with zero crash reports
attributable to Harbor code, and every M3 checklist item checked with
build-bound evidence.

### D3. Ring 2 — GA (week 9)
- App Store review submission with the privacy manifests already in the
  repo; Play production rollout at 10 % → 50 % → 100 % over a week;
  macOS DMG on the release page.
- Tag `harbor-v1.1.0`, assemble the evidence bundle, update `docs/STATUS.md`
  authoritative snapshot, archive rc bundles.
- Post-GA: rollback plan is `docs/release/migration_rollback.md`; the
  catalog stays at epoch 1 unless a model tier was added in B4 (then epoch 2
  is signed with the offline root key before GA, never after).

---

## Deferred to 1.2 (explicitly not in this plan)
- Agents surface enablement (needs its own contract work beyond skills).
- E2EE sync activation (ACC-057), remote inference, connectors,
  diagnostics upload — M4 optional services, each gated independently.
- Deep links / share targets (HBR-114, P1).
- Bundled fonts decision (fallback chains cover every platform today).
- Remaining 21 skill decompositions.

## Sequencing summary

```
Week 1   A1 commit+push  A2 dossier CI  A3 audit gates  A4 resume test     | D0 procurement starts
Week 2   B1 safe-commit FFI/UI                    B4 live-tier tuning       | D0
Week 3   B2 proposal diff        B3 graphs 1-3    B4                        | D0
Week 4   B3 graphs 4-5   B5(opt)  C1 diagnostics   C5 release workflow      | D0 hardware arrives
Week 5   C2 first-run    C3 perf floor  C4 security review + fuzz           | WIN-01 smoke
Week 6   freeze 1.1.0-rc1, D1 ring 0 on real devices, evidence regenerated  |
Week 7-8 D2 closed beta, fix-only                                           |
Week 9   D3 GA
```

Every item is either machine-verifiable on this Mac (A, B, C) or has a
named operator prerequisite (D). A phase does not start until the previous
phase's exit criteria are recorded in `docs/STATUS.md` with the commit hash.
