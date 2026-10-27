# Harbor — upgrade plan for the next agents (written 2026-10-28)

Written against `main` @ `95e80c6` (clean tree, 4 commits ahead of `origin/main`).
Companion to `docs/STATUS.md` (what exists), `docs/PRODUCTION_PLAN.md` (phases A–E,
done), `docs/HANDOFF_2026-10-10.md` (ground rules) and `docs/release/closeout_runbook.md`
(the operator-blocked gates). This plan does not repeat those; it says what to do next,
in what order, and how each piece is proven done.

Everything below was checked by reading the code at the named lines, by running the
read-only dossier tools, or by `git` against the branches. Items marked **[verify]** are
strong inferences from the code that still need a runtime check; the agent that takes
the workstream runs that check first and records the result in `docs/STATUS.md`.

---

## 0. How to work this plan

### 0.1 Ground rules (they have cost whole sessions before)

- Rust workspace is `core/`, not the repo root. `export PATH="$HOME/.cargo/bin:$HOME/harbor-tools/flutter/bin:$PATH"` — Homebrew `cargo` shadows rustup and breaks iOS targets.
- `cargo test --workspace` deletes `core/target/debug/libharbor_ffi.dylib`. Run `cargo build -p harbor_ffi` before any Dart test or ~18 app tests fail with layout-looking names.
- Disk: `core/target` grows past 70 GB. Safe to delete: `core/target/aarch64-linux-android`, `core/target/debug/incremental`, `core/target/aarch64-apple-ios-sim`. ENOSPC blocks the tool shell.
- Every commit that touches tracked files needs the dossier re-sealed in the SAME commit: `python3 tools/validate_dossier.py --write`, then commit `19_Structural_Validation.json`, `20_Readiness_Validation_Report.md`, `24_PACKAGE_MANIFEST.json`. CI (`tools/check_dossier_seal.py`) fails otherwise.
- After dependency changes: `python3 tools/generate_sbom.py --write`, `python3 tools/check_supply_chain.py`, and keep `core/deny.toml` and `core/.cargo/audit.toml` in agreement (`tools/check_advisory_ignores.py`).
- Model fixtures are git-ignored (`.gitignore` names source and sha). Tests that need them `SKIP` and still pass; a green run with no fixtures proves little. Say which fixtures were present.
- `evidence/` is git-ignored. Measurements go there; the tracked record is a `docs/STATUS.md` session entry with the commit hash and the command.
- Slice commits so each is bisectable (contracts → core → FFI → app → docs), seal per commit.
- Honesty rule from the dossier: structural PASS never implies product PASS; "not verified on a device" is written down, not omitted.

### 0.2 The gate every workstream must leave green

```bash
cd core && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo check -p harbor_ffi --no-default-features          # the lean Office core
cargo build -p harbor_ffi                                # restore the dylib for Dart
cd ../apps/harbor_app && flutter analyze && flutter test
cd ../harbor_office && flutter analyze && flutter test
cd ../.. && for d in packages/harbor_domain packages/harbor_native packages/harbor_ui; do (cd $d && flutter analyze && flutter test); done
python3 tools/validate_dossier.py && python3 tools/test_contracts.py && python3 tools/check_supply_chain.py
```

Baseline at this commit: dossier PASS, seal fresh, contracts 124/0 (re-run 2026-10-28). Last recorded full workspace run: 522 passed / 0 failed / 19 ignored (2026-10-10); Dart 71/71 then, more tests since.

### 0.3 Definition of done for any item

1. The behaviour is covered by a test that failed before the change (name it in the commit).
2. The gate in 0.2 is green.
3. If a contract, schema, matrix row or surface changed: the dossier file is updated (or a `28_Correction_Log.md` addendum written for frozen files) and resealed.
4. `docs/STATUS.md` has a dated entry with what was verified, where, and what was not.
5. Nothing is marked PASS in any evidence file by hand.

### 0.4 Agent assignment and conflict avoidance

Each workstream names the files it owns. Two agents never edit the same file set at the same time. Lane order inside a phase is a dependency order, not a priority order. `core/harbor_ffi/src/lib.rs` is the contention point: WS-1 (hotfixes) finishes before WS-2/WS-7 start touching it, and WS-7 (the module split) runs alone.

---

## 1. Where we stand (verified 2026-10-28)

| Fact | Evidence |
|---|---|
| `main` @ 95e80c6 is clean, 4 commits ahead of `origin/main` (unpushed: 9b4dff7 01ac9e0 2b348ad 95e80c6) | `git status -sb`, `git rev-list origin/main..main` |
| Last CI on `origin/main` (bd97501) green on all six jobs | `gh run list` |
| Dossier PASS, seal fresh, contracts 124/0 | `tools/validate_dossier.py`, `check_dossier_seal.py`, `test_contracts.py` |
| **The lean Office-only core does not compile on `main`** (`cargo check -p harbor_ffi --no-default-features` → 2 errors); no CI job covers it | run 2026-10-28, see WS-1 C12 |
| Core: 15 crates, ~570 Rust tests (22 ignored live tiers), 4 fuzz targets; `harbor_ffi/src/lib.rs` is 5,534 lines with a 95-arm string dispatch | `grep -c '#\[test\]'`, `wc -l` |
| App: harbor_app ~15.5k Dart LOC + harbor_office ~4.8k + packages ~3.4k (excluding generated l10n); 30 Dart test files, ~180 tests, no goldens, no `integration_test/` | `find`, `grep` |
| Skills: 36 built-in, 17 runnable graphs, 19 declaration-only | `builtin_skills.json`, `src/graphs/` |
| Formula: `22_Formula_Coverage.json` on main = 10 groups / 99 targets; the unmerged branch has 27 groups / 407 targets with physical-device tiers | `git show designed-decks-and-motion:22_Formula_Coverage.json` |
| Catalog epoch 5 signed in the app asset | `assets/catalog/signed_catalog.json` |
| Office matrix: XLSX charts, XLSX conditional formatting, PPTX charts are `REQUIRED_UNQUALIFIED`; PPTX animations `PRESERVE_ONLY` | `21_Office_Feature_Matrix.csv` |
| Platforms: `windows/` runner exists (never executed, WIN-01); no `linux/` runner although Dart code branches on `Platform.isLinux`; harbor_office has no desktop target | `ls apps/*/` |
| Locales: harbor_app en+ar; harbor_office en, ar, fr, de, es, hi | `supportedLocales`, `l10n/` |
| Toolchain: Flutter 3.47.4 pinned (3.47.8 security patch exists), Dart 3.13.3, Rust 1.97.1, edition 2021, no `rust-toolchain.toml` | `flutter --version`, `core/Cargo.toml` |
| Deps behind: `llama-cpp-2 =0.1.156` vendored+patched (0.1.160 released 2026-10-15; 0.1.161 on upstream main); `formualizer 0.9` patched (0.13.0 released 2026-10-25); `ureq 2` (3.x exists); `rand 0.8`; `zip 4` with three accepted advisories fixed in zip ≥ 9; `umya-spreadsheet 3.1` (4.0.0-beta.1); `flutter_lints ^4` (6.x) | crates.io, `Cargo.lock`, `deny.toml` |
| Operator blockers unchanged: Apple Developer identity + notarization, Play Console + upload key, Windows machine, min-spec M1 8 GB Mac, offline catalog root key. **New:** a physical iPhone and a physical Android phone were used on the deck branch (formula device tiers), so IOS-02 / AND-03 device evidence may now be producible | `docs/release_checklist_status.md`, branch commits 95e6b23 a9b8a84 |

### 1.1 Unlanded work on branches (the single largest "upgrade" available)

All three diverged from `553d243` (2026-10-02). `main` has 24 commits since, including the 10→7 navigation consolidation that **deleted `apps/harbor_app/lib/surfaces/ask_surface.dart`**.

| Branch | Ahead / behind | What it holds | Integration cost |
|---|---|---|---|
| `ask-multi-turn-threads` (e26ae64) | 1 / 24 | `harbor_ffi/src/threads.rs` (946 lines): sealed `threads.db`, `thread.*` FFI, `thread_id` on `op.start_generate`/`ask.generate`, bounded history (4 turns / 2000 chars) sent as real user/assistant messages, context-overflow shedding, purge-derived-content, Ask surface restores a thread. 437-line FFI test, plaintext inspection extended. | Core half ports cleanly (13 both-side files, mostly `lib.rs` and `knowledge.rs`). App half targets the deleted Ask surface and must be re-ported onto `HomeSurface` + `WorkCommandBar`. |
| `designed-decks-and-motion` (a9b8a84) | 11 / 24 | **Includes the threads commit.** Plus: `harbor_render::slides` real slide-scene renderer (any PPTX), `harbor_ui` `SlideView`/`PresentView` (+22k lines under `packages/harbor_ui/lib/src/slides/`), 14 deck designs × 20 palettes × 20 purposes, authored motion presets, `chart_xml.rs` rewrite (fixes "every generated chart was invalid"), deck designer sheet in both apps, **formula expansion 99→407 targets / 1,261 cases with Excel as oracle**, engine fidelity patch set (`formualizer-eval 0.9.3+harbor-excel-fidelity`), formula tiers re-run on macOS, iOS sim, Android emulator, **physical iPhone and physical Android (OnePlus CPH2767)**, `tools/qualify_ios`, `docs/operations/{formula-expansion,deck-design-languages,slide-scene-contract}.md`, a decision file numbered **0011 which collides with main's `0011-embeddinggemma2-personal-agents.md`**. | 30 files changed on both sides: `lib.rs` (2,162-line delta), `harbor_service.dart`, `ask_surface.dart` (deleted on main), `knowledge_surface.dart`, `shell_test.dart`, `skills.rs`, `builtin_skills.json`, `tools/authoring.rs`, `tools/builtin.rs`, `pptx.rs`, `workspace.rs`, `knowledge.rs`, `foundation.dart`, `Cargo.lock`, `STATUS.md`. Branch STATUS admits: no visual run of the Flutter screens; the new-presentation sheet duplicated in both apps (~980 lines); Arabic/de/es/fr/hi strings without native review. |
| `moss101-animated-giggle` (b336745) | 9 / 12 | T1–T5 (OCR/ASR registration, capture, vision OCR) — **already on main** in other commits. Branch-only files: `apps/harbor_app/test/vision_ocr_test.dart` (262 lines), `docs/verification_2026-10-11.md`. | Cherry-pick the two files, delete the branch. |
| `enhancements` | 0 / 19 | nothing | delete |

---

## 2. Workstreams

Priority: **P0** = correctness or data-safety defect, or blocks everything after it. **P1** = large user value or unblocks a release gate. **P2** = quality, hygiene, polish. Size: S ≤ 1 day, M = 2–4 days, L = 1–2 weeks, XL > 2 weeks, for one agent.

### WS-0 · Land what exists  (P0, L, machine-only)

Goal: `main` contains every verified piece of work the repo already holds, and `origin/main` equals `main`.

Owns: branches, `core/harbor_ffi/src/{lib,knowledge,threads}.rs`, `core/harbor_render/`, `core/harbor_artifacts/src/{deck,chart_xml}*`, `core/harbor_formula/`, `packages/harbor_ui/lib/src/{slides,deck_designer}/`, `apps/*/lib/surfaces/work/deck_view.dart`, `apps/harbor_app/lib/surfaces/home_surface.dart`, `apps/harbor_app/lib/surfaces/work/command_bar.dart`, `22_Formula_Coverage.json`, `21_Office_Feature_Matrix.csv`, `28_Correction_Log.md`, `docs/decisions/`.

Steps:
1. `git push origin main` (4 commits). Confirm CI green.
2. Cherry-pick `vision_ocr_test.dart` and `docs/verification_2026-10-11.md` from `moss101-animated-giggle` (the test must pass with a fake service or skip when fixtures are absent; check `main`'s `HarborService` API since T4 landed differently). Delete `moss101-animated-giggle` and `enhancements`.
3. Integrate `designed-decks-and-motion` (which carries the threads commit) in **slices**, not one merge:
   a. **Formula expansion** first: `core/harbor_formula/` (corpus, `examples/probe_cases.rs`), `third_party/formualizer-eval` fidelity patches, `tools/formula_batch*.py`, `tools/qualify_ios`, `22_Formula_Coverage.json` (27 groups / 407 targets), `docs/operations/formula-expansion.md`, decision 0001 addenda 2–3. Re-run `cargo test -p harbor_formula full_qualification` and the macOS tier on the merged commit; device tiers are re-run in WS-15 because evidence binds to the commit. Pin the engine (`tools/pin_engine.py`).
   b. **Slide scene renderer + chart writer**: `harbor_render::slides`, `scene*.rs`, `harbor_artifacts/src/chart_xml.rs`, `deck/` module tree, `docs/operations/slide-scene-contract.md`. `pptx.rs` conflicts with main's PPTX slide-edit work (session 56): keep both; the branch's `office_conformance` tests and main's `pptx.edit` FFI tests must both pass.
   c. **Threads core**: `harbor_ffi/src/threads.rs`, `thread.*` ops, `thread_id` on `op.start_generate`, `kcipher.rs` additions, `ask_threads.rs` test, plaintext inspection. Resolve against main's `document` scope in `op.start_generate` (session 61): both arguments are optional and orthogonal.
   d. **Threads app**: re-port the deleted `ask_surface.dart` behaviour onto `HomeSurface` (thread list/switch/rename/delete/new, restore latest) and give `WorkCommandBar` a per-file thread. Knowledge-removal purge preview goes into Library (`knowledge_surface.dart`).
   e. **Deck designer UI + SlideView**: `packages/harbor_ui/lib/src/slides/`, the designer sheet (land it ONCE in `harbor_ui`, not in both apps — the branch says it is duplicated), `deck_view.dart` in both apps, "New presentation" / "Change design" entry points. Present mode honours reduce-motion.
   f. **Dossier**: renumber the branch's decision 0011 to **0021**; `21_Office_Feature_Matrix.csv` rows 24–25 (designed decks, authored motion) and the PPTX animations row move from `PRESERVE_ONLY` to the branch's `REQUIRED_UNQUALIFIED` wording; XLSX/PPTX chart rows reference the new writer; `28_Correction_Log.md` addendum; reseal.
4. Each slice: full gate (0.2) + `docs/STATUS.md` entry. Delete the two branches when the last slice lands.

Done when: `git branch -a` shows only `main`; `22_Formula_Coverage.json` says 407 targets; `flutter test` in both apps includes the deck designer and thread tests; Home restores a conversation across restart on the simulator (screenshot recorded).

Risks: `lib.rs` three-way conflicts (do WS-0 before WS-1 touches `lib.rs`, or rebase WS-1 onto it); branch tests were green on 2026-10-05 against *that* core — expect 1–2 days of re-green work.

### WS-1 · Correctness hotfixes  (P0, M, machine-only)

Goal: every defect found by the 2026-10-28 audit that is a one-site fix is closed with a regression test. All anchors are `main` @ 95e80c6; re-locate after WS-0.

**Core**

| # | Defect | Where | Fix + test |
|---|---|---|---|
| C1 | `op.cancel` does nothing for generation and ingest: a fresh `AtomicBool::new(false)` is passed instead of `entry.progress.cancel` | `core/harbor_ffi/src/lib.rs:4718`, `:4819` (also `:3275`, `:3338` in `run.decide`/`run.commit_proposal`, `:4510` in `ask.generate`) | Pass the op's cancel flag; add `tests/cancel_generate.rs` and `cancel_ingest.rs` modelled on `tools_read_op.rs:57` |
| C2 | Generation stops only on `token_eos()`; models whose turn terminator is EOG-not-EOS run to `max_tokens` | `core/harbor_inference/src/gguf.rs:337,356` | Use `model.is_eog_token(t)` (already used in `vision_ocr.rs:196`); add stop-string support; live test on Qwen + Llama fixtures **[verify per model]** |
| C3 | Re-ingesting an existing source leaves it in `revoked`, so it is unsearchable until restart | `core/harbor_knowledge/src/index.rs:204-230` (`add_source` never clears what `remove_source` inserted) | Clear on add; two-ingest test asserting a hit and `citation_state != Removed` |
| C4 | `memory.add` stores the first 60 chars of private memory text as the plaintext `title` column, and the SHA-256 of the full text | `core/harbor_ffi/src/lib.rs:1428`; schema at `knowledge.rs:114-122` | Seal `title`; make `content_hash` an HMAC under the workspace key (migration for existing rows); change the plaintext-at-rest sentinel to live in the title (`harbor_integration/tests/plaintext_at_rest_inspection.rs:163`) |
| C5 | HF bearer token is attached in `fetch()` but not in `fetch_streaming_to()`: gated weight downloads 401 | `core/harbor_modelhub/src/acquire.rs:111-129` vs `:186+` | Attach on huggingface.co hops in the streaming path; extend `token_rides_hf_hops_and_is_stripped_cross_origin` to streaming |
| C6 | 30 s **overall** request deadline on weight downloads (ureq 2 `.timeout()` bounds body reads); then retried from byte 0 | `core/harbor_net/src/broker.rs:132,393`, `transport.rs:42,91` | Connect timeout + idle-read timeout for streaming; test with a slow fixture server **[verify with a >30 s download first; record whether it ever succeeded]** |
| C7 | `ChatHandle.loaded` inserts the package id BEFORE `provider.load`; a failed load leaves it "loaded" and every later generate says `ModelNotFound` | `core/harbor_ffi/src/knowledge.rs:1278-1290` | Insert after success; don't hold the mutex across the load |
| C8 | `blob.get` panics on a `blob_id` shorter than 2 bytes / non-ASCII; accepts `../` | `core/harbor_store/src/blob.rs:78` | Validate 64-hex; fuzz seed |
| C9 | Zip-bomb check runs after full decompression | `core/harbor_artifacts/src/package.rs:52-58` | `entry.take(stored*200+1)` and check `entry.size()` first; hostile fixture |
| C10 | `models.acquire_catalog` builds a `SignedCatalog`, discards it, never calls `acquire_signed`; persisted trust state in `store.db` is unsigned JSON reloaded without re-verification | `core/harbor_ffi/src/lib.rs:3903-3920`, `:478-537` | Persist the signed envelope and re-verify on load/use (or MAC under the device key); use `verify_strict` (`catalog_signing.rs:112`) |
| C11 | `BlobStateStore` guards the shared `runs/<ws>.index.json` with a per-instance mutex; FFI makes a new instance per call, so concurrent skill runs can lose index updates | `core/harbor_core/src/executor.rs:2340-2401`, `lib.rs:3218,3268` | One shared instance (or move the index into SQLite); two-concurrent-runs test |
| C12 | **CONFIRMED BROKEN 2026-10-28**: `cargo check -p harbor_ffi --no-default-features` fails with 2 errors (E0425/E0433) — `parse_sources` (`lib.rs:5109`) is ungated but calls the gated `parse_sources_unchecked` (`:5127-5128`) and gated types. The Harbor Office Suite core cannot be built from `main`. Duplicated `#[cfg]` at 5096/5100 | `core/harbor_ffi/src/lib.rs:5094-5128` | Gate `parse_sources` (or move both into `knowledge.rs`); add the lean-build CI job the same day so this cannot recur. **Do this first.** |
| C13 | Unbounded inputs on `knowledge.ingest`, `op.start_ingest`, `memory.add` (document scope has caps in `doc_scope.rs:21-33`) | `lib.rs:4325,4819,1390+` | Apply the same cap pattern |
| C14 | `fs::read` of the whole GGUF on `models.install_from_path` (OOM on phones) | `lib.rs:4054` | Stream-hash + rename |
| C15 | `.expect()` reachable from user XLSX in the formula engine adapter | `core/harbor_formula/src/engine.rs:62,71,78,101` via `workbook.rs:479-500` | Return `Result`; test with a sheet name/formula the engine rejects **[verify which inputs Err]** |

**App**

| # | Defect | Where | Fix + test |
|---|---|---|---|
| A1 | `MaterialApp(shortcuts: harborShortcuts())` **replaces** `WidgetsApp.defaultShortcuts` (Tab/Enter/Escape/arrows) instead of merging; the comment at `keyboard.dart:45` says "merged". Tests hide it by nesting `HarborApp` inside another `MaterialApp` | `apps/harbor_app/lib/main.dart:374`, `shell/keyboard.dart:45` | `{...WidgetsApp.defaultShortcuts, ...harborShortcuts()}`; a test that pumps `HarborApp` unwrapped and tabs **[verify at runtime]** |
| A2 | Office `MaterialApp` has no `supportedLocales`, so ar/fr/de/es/hi resolve to en (and LTR) | `apps/harbor_office/lib/main.dart:343-356` | Add the six; unwrapped widget test asserting Arabic directionality **[verify]** |
| A3 | Office prefs keep a saved locale only if `en`/`ar` | `apps/harbor_office/lib/services/preferences.dart:102-104` | Accept the six; test |
| A4 | `RegExp(r'\.(md|pdf)\$')` in a raw string matches a literal `$`: converted files are named `notes.md.docx` | `apps/harbor_app/lib/surfaces/work_surface.dart:98`, `apps/harbor_office/lib/surfaces/work_surface.dart:132` | `r'\.(md|pdf)$'`; unit test |
| A5 | Two handlers on `dev.harbor.office/open`; the later `_importHandedFile` bypasses `OpenIntake` (no sanitising, writes to Documents root, records `.pptx` as `docx`) | `apps/harbor_office/lib/main.dart:168-248` | Delete `_openHandedDocument`; use `OpenIntake` only; test |
| A6 | Production worker never calls `harbor_core_abi_version` (SEC-028); only the test-only sync client does | `packages/harbor_native/lib/harbor_worker.dart:208-247` vs `harbor_ffi.dart:97-104` | Check in `_entry`, fail open with the typed error |
| A7 | Worker-death `_FailAll` is not a `HarborCoreException`, so every `on HarborCoreException` in the service misses it | `harbor_worker.dart:183-188,306` | Subclass; test |
| A8 | Linux library name is `.dylib` | `apps/harbor_app/lib/services/harbor_service.dart:1246` | `.so`; or drop Linux paths (see WS-12) |
| A9 | Workbook actions ignore the selection: highlight `B2:B21`, bold columns 1–12, chart anchor `E2:K18`; one English error string | `apps/harbor_app/lib/surfaces/work/workbook_view.dart:164-232` | Drive from the selected cell/range; localise; widget test |
| A10 | Android back exits from every surface (no `PopScope`) | `apps/harbor_app/lib/shell/` | `PopScope` → Home / previous surface; test |
| A11 | Office recents and intake record `.pptx` as kind `docx` | `work_surface.dart:376-380`, `main.dart:227-232` | Fix + test |
| A12 | Office "Preview only" badge on editable docx/pptx; copied "Open Lens" button opens nothing | `apps/harbor_office/lib/surfaces/work_surface.dart:460-466,514-519` | Remove/fix |
| A13 | Office `editWorkbook`/`editDocumentOps` push history and replace bytes before re-previewing; a preview failure leaves state inconsistent (`editDeckSlide` does it right) | `apps/harbor_office/lib/services/harbor_service.dart:701-709,873-876` | Reorder; test |
| A14 | Office autosave draft written as plaintext under app support | `office harbor_service.dart:950` | Seal through the core (`blob.put`) or document the exception in `13_Network_and_Storage_Policy.md` |
| A15 | `clearPreview` keeps the draft, so a closed document is "recovered" next launch | `office harbor_service.dart:716-725` | Discard on close |
| A16 | `substring(0,12)` on a possibly empty sha; hardcoded `'revision …'`; catalog `FitScoreBadge` without a localised label | `apps/harbor_app/lib/surfaces/models_surface.dart:416,435,445` | Guard + localise |

Done when: each row has a named failing-then-passing test; STATUS entry lists which **[verify]** items were confirmed, which were not reproducible.

### WS-2 · Make the core non-blocking  (P0→P1, L)

Goal: no call that can take more than ~200 ms runs synchronously on the single Dart worker; progress, cancel and partial results are available for all of them.

Owns: `core/harbor_ffi/src/lib.rs` (op section), `core/harbor_modelhub/src/progress.rs`, `packages/harbor_native/lib/harbor_worker.dart`, `apps/*/lib/services/harbor_service.dart` (`_runOp`).

Steps:
1. Inventory is in the audit: synchronous today and slow — `knowledge.open/ingest/search`, `ask.generate`, `models.acquire_hf/acquire_catalog/install_from_path/install_file/search_hf/hf_files/acquire_preflight`, `tools.read`, `run.decide`, `run.commit_proposal`, `skills.suggest` (prewarm + disambiguation), `decision.choose`, `ocr.open`, `memory.add`, `eval.*`, `convert.*`, `workbook.edit`, `docx.edit`, `pptx.edit`, `artifact.preview`. Convert each to `op.start_*` using one `start_op(kind, subject, |progress, cancel| …)` helper (the pattern appears 7 times today).
2. Keep the synchronous names as thin wrappers for one release (the Office app and tests call them), then remove.
3. Op registry: add a concurrency limit per kind (one generation at a time; N≤2 heavy ops) and a queue; today two `op.start_generate` calls each build a full llama context.
4. Progress snapshot gains `partial_text` (ring buffer) so `op.status` can stream tokens; or add `op.read_stream(op_id, cursor)`. UI work is WS-6.
5. Typed errors: `{code, message, retryable}` instead of 340 `HarborError::Other` strings; callers stop string-matching `"cancelled"` (`lib.rs:4574,4774,4823`).
6. Binary payload ABI: `harbor_core_call_bytes(ptr,len)` or path/handle based, so a 50 MB workbook is not base64-copied four times (`lib.rs:1071,1718`; Dart `harbor_service.dart:1122-1188`). Dart side sends `TransferableTypedData`.
7. Worker: document (or enforce with a `Mutex` in the handle) that `&mut *handle` at `lib.rs:5181` is sound only because one isolate serialises calls; the synchronous `HarborCoreClient` on the same handle would be UB.

Done when: `op.list` shows every slow operation; UI never freezes >200 ms in `shell_test` under a 50 MB fixture; cancel of each op kind has a test.

### WS-3 · Model manager and generation quality  (P1, L)

Owns: `core/harbor_inference/src/{gguf,provider}.rs`, `core/harbor_ffi/src/knowledge.rs` (`ChatHandle`), `core/harbor_core/src/executor.rs:2019-2218`, `core/harbor_modelhub/src/{fit,gguf_meta}.rs`.

1. **One process-wide `ModelRegistry`**: refcounted handles, LRU under a memory budget, `chat.release` / `models.release_all(pressure)` FFI ops. Today `KnowledgeService`, `ChatHandle` and `ocr.open` each build their own `GgufLlamaCppProvider` (`knowledge.rs:700,1257`, `lib.rs:2924`); chat models are never unloaded; skill runs load through `executor.rs:797` bypassing the in-use tracking that the SEC-024 uninstall guard reads (`lib.rs:1115-1132`).
2. `generate_cancellable` on the `ModelProvider` trait; executor model nodes use it (`executor.rs:2140`), so a run can be cancelled mid-generation, not only between nodes.
3. Sampling: top-k / top-p / min-p / repeat penalty (all exist in the crate, `sampling.rs`), configurable per skill node with the current greedy/temp defaults preserved for replay cassettes.
4. Set generation `n_threads` (`gguf.rs:280` leaves llama.cpp's default 4); expose flash-attention and KV type; memory-aware `n_ctx` cap (a 128K model on a phone today grows to `n_ctx_train`).
5. Surface the chat-template fallback (`gguf.rs:207-231` silently falls back to an ad-hoc format) as a typed warning in the response.
6. **KV reuse**: a cached context per model with prefix matching via `kv_cache_seq_rm`; structured-output retries then stop re-prefilling (`executor.rs:2126-2205`). This is the prerequisite for fast multi-turn (WS-6).
7. Batch embeddings across sequences (`gguf.rs:432-459` does one decode per text) — ingest of N chunks is N forward passes today.
8. Fit Score: read `block_count`, `head_count_kv`, `embedding_length`, `context_length` from the GGUF header (`gguf_meta.rs` parses only architecture and pooling) instead of `8<<20` per 1k tokens and `weights×1.125`.
9. Generation GPU-correctness canary (the embedding path has one, `knowledge.rs:463-487`; generation only has `HARBOR_GGUF_CPU_ONLY`).

Done when: switching chat models frees the previous one (RSS measured); uninstall guard sees skill-run loads; a 4-turn conversation's second turn prefills only the delta (token count asserted); live tier re-run and recorded (expect changes from C2 and sampling: re-promote cassettes deliberately).

### WS-4 · Downloads that survive the real world  (P1, M)

Owns: `core/harbor_net/src/{broker,transport}.rs`, `core/harbor_modelhub/src/acquire.rs`, `apps/harbor_app/lib/surfaces/models_surface.dart`, `widgets/ops.dart`.

1. After C5/C6: Range-based resume with persisted `.part` files; keep the staging dir across retries (today any failure discards it, `acquire.rs:347-350`); reset `bytes_done` on retry (`:228`) so progress cannot exceed 100 %.
2. Validate `repo_id` as `owner/name` and `revision` before URL interpolation (`acquire.rs:381,455`).
3. Consider `ureq 3` here (all usage is ~140 lines in `transport.rs`; its `timeout_connect`/`timeout_recv_body` model matches the fix). Re-run the network-capture tier on the qualification machine.
4. App: Hugging Face install currently downloads **every** GGUF in a repo (`models_surface.dart:774-777`). Add a quantisation/file picker with size and Fit Score per file; debounce search; page beyond 8 results; derive installed state from `installedModels`, not a local set.
5. Progress card: speed, ETA, pause/resume. Background continuation on mobile (iOS `URLSession` background / Android foreground service) is a platform item — see WS-12 — until then, warn before backgrounding.

Done when: a 1 GB fixture download interrupted at 40 % resumes from 40 % (test with the slow fixture server); gated-repo test passes with a token; the picker shows one row per GGUF.

### WS-5 · Retrieval overhaul  (P1, L)

Owns: `core/harbor_knowledge/src/{index,chunk}.rs`, `core/harbor_ffi/src/{knowledge,doc_scope}.rs`.

1. After C3: normalise vectors at insert and use dot product; `select_nth_unstable` top-k; `chunk_id→idx` map (today linear `find` per hit, `index.rs:347-361`).
2. Hybrid search: reuse the BM25 in `doc_scope.rs` (or SQLite FTS5 over sealed text) with reciprocal-rank fusion; make `top_k` (hard-coded 3 at `lib.rs:4509,4647`) a bounded argument.
3. Chunking: `respect_paragraphs` is a no-op (`chunk.rs:23-26`); merge short paragraphs; make windows token-aware against the embedder's tokenizer.
4. RAG prompt: add a system role; stop discarding the computed citation title (`knowledge.rs:1362-1363`).
5. Footprint: today every chunk is decrypted into RAM on open (`knowledge.rs:859-897`); `load_persisted` is O(sources×chunks). Load vectors only, fetch text per hit; plan int8/f16 vectors and an ANN path (`hnsw_rs` / sqlite-vec) above ~50k chunks. Memory-pressure release should cover the index, not only the weights.
6. Re-run `cargo test -p harbor_knowledge` (832-case corpus, thresholds in `26_Qualification_Profiles.json`) and record NDCG before/after. **Retrieval quality must not regress**; the corpus hash binding test guards the corpus, not the ranking.

Done when: 100k-chunk synthetic index opens in < 2 s and searches in < 50 ms on the reference Mac (recorded), NDCG@10 ≥ current on all three languages, re-ingest test from C3 passes.

### WS-6 · Conversations people can actually have  (P1, L)

Owns: `apps/harbor_app/lib/surfaces/home_surface.dart`, `widgets/{answer,composer}.dart`, `surfaces/work/command_bar.dart`, `services/request_router.dart`; core pieces from WS-0c, WS-2.4, WS-3.6.

1. Threads land in WS-0; here they get: thread list with search, rename, delete, pin; per-file thread in the Work bar; "continue in Home" from a file thread.
2. Streaming display from `partial_text`; markdown rendering (lists, bold, code, tables) replacing raw `SelectableText` (`answer.dart:227`); copy, regenerate, edit-and-resend.
3. Library citations on Home become tappable and open the source (`home_surface.dart:554-560` has no `onCitationTap`).
4. Composer: stop rebuilding the whole Home on every keystroke (`home_surface.dart:128-130`); verify Enter vs IME composition (CJK/Arabic IMEs) **[verify]**.
5. Router: `looksLikeQuestion` and the stopword list are en/ar only (`request_router.dart:169-226`); extend per locale shipped (WS-11).
6. Screen-reader announcement when an answer completes (`SemanticsService.announce`, unused today).

Done when: a 3-turn follow-up ("and in Arabic?") resolves against the previous answer on the simulator with Qwen2.5-1.5B (live test, recorded); tokens appear within 300 ms of generation start; markdown table renders.

### WS-7 · Split the FFI and the tool files  (P2, L, runs alone on `lib.rs`)

Owns: `core/harbor_ffi/src/`, `core/harbor_core/src/tools/`, `core/harbor_core/src/executor.rs`, `core/fuzz/`.

1. **Before any move**: golden request/response tests for the ~30 ops with no Rust test (`run.pause, blob.*, formula.qualify, trust.pulse, model.fit_score, artifact.preview, ocr.*, speech.status, eval.*, run.log_request, hub.*, models.search_hf/hf_files/acquire_*/install_file/uninstall_*, knowledge.release, ask.generate, op.start_acquire/ingest, op.list, runs.list`). Extend `fuzz_targets/ffi_dispatch.rs` from 31 to all 95 methods; add XLSX/DOCX/PPTX/PDF parser fuzz targets.
2. `lib.rs` → `ffi/{ops,runs,memory,goals,diag,models,office,skills,workflow,catalog,install,knowledge,ask}.rs`, each `fn handle(ws, method, args) -> Option<Result<Value>>`; `#[derive(Deserialize)]` arg structs replace 150 `.get("x").and_then(as_str)`; `requires_gguf!()` replaces 16 copies of the feature refusal.
3. `ToolRegistry::builtin()` cached in a `OnceLock` (rebuilt and regexes recompiled per call today, `lib.rs:216,236,3214,3272`); `tool!{}` macro for the ~35 `ToolSpec` boilerplates; split `create.rs`, `builtin.rs`, `authoring.rs` along the section boundaries the audit lists.
4. `executor.rs` → `executor/{types,lifecycle,commit,nodes/*,model_call,state_store}`; add a unit test per node kind before touching `execute_node` (570-line match, 0 in-file tests).
5. Gate `eval.run`/`eval.run_skill` (arbitrary `evals_root` path from the caller, `lib.rs:3618-3640`) behind a `dev-tools` feature.

Done when: no source file in `core/` exceeds 1,500 lines; all golden tests unchanged; fuzz corpus covers 95 methods; `cargo test` count ≥ before.

### WS-8 · App architecture: shared packages and scoped state  (P1, XL)

Owns: new `packages/harbor_client`, new `packages/harbor_work`, `packages/harbor_domain`, `apps/*/lib/services/`, `apps/*/lib/main.dart`, `apps/*/lib/surfaces/work/`.

Facts: `HarborService` is one `ChangeNotifier` with ~75 members (`apps/harbor_app/lib/services/harbor_service.dart`); every `notifyListeners()` (incl. each 250 ms op poll) rebuilds every `HarborServiceProvider.of` dependent, and `ListenableBuilder(listenable: _state)` wraps all of `MaterialApp` (`main.dart:356-419`), so each surface switch rebuilds two `harborThemeData`. The two apps carry forked copies: `workbook_view.dart` 95 % identical, `diagnostics.dart` identical, `harbor_service.dart` ~855 common lines, plus `document/pdf/deck_view`, `work_surface`, `settings_surface`, `preferences`, `file_types`. The Office copy has undo/redo, autosave, multi-sheet, row/column ops, docx/pptx editing, print and PDF export that the main app lacks. `harbor_domain`'s `ModelInfo`, `RunSummary`, `FitBandVM`, `TrailEntry` are unused; ~460 untyped map accesses in app lib.

1. `harbor_client`: worker + typed per-domain APIs (Models, Knowledge, Runs/Ops, WorkDocument, Goals, Diagnostics) over DTOs in `harbor_domain` (`fromMap`, schema-drift tests against FFI goldens from WS-7.1). Base64 and large copies leave the UI isolate (WS-2.6).
2. `harbor_work`: the four viewers + `WorkDocumentSession` (bytes, preview, undo/redo, autosave, save) with capability flags (`editable`, `citationFocus`, `multiSheet`); its own ARB set. Use the Office versions as the base; harbor_app gains Save/undo/autosave; key the command bar by file identity, not bytes (`work_surface.dart:337` resets Q&A state on every cell edit).
3. Per-feature controllers with their own scopes; `ValueListenableBuilder` at leaves; `MaterialApp` listens only to locale/theme. `refresh()` (6–7 round trips, awaited after 17 mutations) becomes per-domain invalidation; `runs.list` paginated.
4. Replace the `pending*` fields on `AppState` (`main.dart:117-129`) with a typed navigation intent consumed once.
5. Delete dead code: unused `HarborCoreClient` helpers, orphaned doc comments (`harbor_service.dart:300-302, 949-953`), 32 unused app ARB keys, 339 unused Office ARB keys.
6. **CI**: add `apps/harbor_office` to the flutter job (format, analyze, test — it is not run today, `ci.yml:146-171`); live-core tests `fail` with a clear message instead of `if (!coreAvailable) return;` unless `HARBOR_ALLOW_NO_CORE=1`.
7. Give Office `HarborService` a `detached()` so its widget tests run without the dylib.

Done when: no file is shared by copy between the two apps (`comm` check in a test); a progress tick rebuilds only the progress card (rebuild-count test); both apps in CI.

### WS-9 · A Work canvas that scales  (P1, L)

Owns: `packages/harbor_work` (after WS-8.2) or `apps/*/lib/surfaces/work/` before it; `core/harbor_render/`.

1. Grid: 2-D virtualisation (`two_dimensional_scrollables` `TableView`) removing the 400×60 cap (`workbook_view.dart:52-53`); keyboard navigation (arrows, Enter-to-edit, Tab); range selection; copy/paste of ranges; row height ≥ 44 px on touch; selection-driven ops (A9).
2. `DocumentView` lazy (today eager `Column` of every paragraph with a GlobalKey each, `document_view.dart:97-119`); zoom (`InteractiveViewer`); find-in-document across all types; cross-block text selection.
3. Slides: real render via WS-0e; presenter mode; add/reorder slides (Office).
4. PDF: rendered pages, not only extracted text (`pdf_view.dart:8-9`). Decision needed: a core rasteriser (keeps the no-new-Dart-deps stance, large) vs `pdfrx`. Record as decision 0022.
5. Large files: whole files are read and base64-encoded on the UI isolate; fixed by WS-2.6/WS-8.1. Each workbook edit encodes→edits→decodes→re-previews the whole file (`harbor_service.dart:1160-1169`): return a preview diff from the core.

Done when: a 50k-row workbook scrolls at 60 fps on the reference Mac and opens on the iPhone simulator; keyboard-only cell edit works; find jumps to the hit in docx/pdf/pptx/xlsx.

### WS-10 · Office suite features  (P1, L; depends on WS-8.2)

Owns: `apps/harbor_office/`, `packages/harbor_work`, `core/harbor_artifacts/src/{docx,workbook,pptx}.rs`, `core/harbor_ffi` office ops.

1. Save semantics: Save New Copy stays the default (file 27: Overwrite is disabled until the adapter passes concurrent-writer and crash-boundary qualification). Add the unsaved-changes prompt on close/open, Ctrl/⌘+Z/Y, Ctrl/⌘+S → save copy; discoverable edit affordance on desktop (today long-press per paragraph).
2. Qualify Overwrite for one adapter (local file replace on macOS via the commit journal) so the office app can save in place: fault-injection tests per `27_Artifact_Commit_Qualification.md`, then flip the matrix.
3. Find/replace across the three formats; basic formatting toolbar (bold/italic/alignment via typed `docx.edit` style ops, number formats, cell styles); insert/delete paragraph; sort/filter + fill handle for sheets; templates beyond blank.
4. Charts: the branch chart writer (WS-0b) → chart UI in Office (series/range picker) → fixtures for the `REQUIRED_UNQUALIFIED` chart rows → qualification (WS-15).
5. Export: docx/xlsx→PDF are text-level today (`harbor_service.dart:809-843`); use the slide-scene approach for a layout-faithful PDF writer (`pdf_out.rs`) as a follow-on.
6. Desktop targets for harbor_office (macOS, Windows): the lean core already builds; add runners, a menu bar, drag-and-drop open.

Done when: open → edit → find/replace → save copy → reopen round trip on iOS sim, Android emulator and macOS, with the Office matrix rows updated honestly.

### WS-11 · Localisation and accessibility  (P1, M)

Owns: `apps/harbor_app/lib/l10n/`, `apps/harbor_office/lib/l10n/`, `packages/harbor_ui/`, `apps/*/test/{l10n_coverage,accessibility_audit}_test.dart`.

1. Six locales for harbor_app (558 keys; port overlaps from the Office ARBs; native review flagged as pending in STATUS). `supportedLocales`, prefs, a locale list instead of the en/ar toggle (`settings_surface.dart:103-105`, `command_palette.dart:120-129`). Devanagari fallback in `HarborType` (`tokens.dart` has Latin/Arabic/mono only).
2. `intl` formatting for dates, sizes and numbers (today `DateTime.toString()` at `goals_view.dart:480`, hand-formatted GB in three places, no Arabic-Indic digits).
3. l10n gate generalised to N locales, checks "used key identical to English" and scans assignments and `harbor_ui` defaults (`FitBand.label`, `HarborErrorState.title`, `RunTrail.emptyLabel` are English constants). Apply the gate to Office (it has none).
4. Accessibility: `meetsGuideline(labeledTapTargetGuideline / androidTapTargetGuideline / textContrastGuideline)` in the audit test; cover chips, `InkWell`, `HarborCard(onTap)`, menus; focus-order test; a high-contrast palette honouring `MediaQuery.highContrast`/`boldText`; VoiceOver/TalkBack pass on devices (operator-adjacent; record in STATUS).
5. Golden tests for `harbor_ui` components in light/dark/RTL/200 %; use `HarborSkeleton` (defined, never used) for loading lists.
6. Decide on bundling Inter / Noto Sans Arabic / JetBrains Mono (deferred since 1.1; fonts are named but not shipped, so look differs per platform). Record as decision.

Done when: `flutter test` enforces six locales in both apps; the audit test uses the Flutter guideline matchers; goldens exist for every `harbor_ui` component.

### WS-12 · Platform integration  (P1/P2, L; several items need devices)

Owns: `apps/*/android`, `apps/*/ios`, `apps/*/macos`, `apps/*/windows`, `native/`, `apps/harbor_app/lib/services/{native_adapters,capture,video_frames}.dart`.

1. Android: `PopScope` (A10); state restoration (`restorationScopeId`, none today); predictive back already enabled.
2. Open-with / share target for harbor_app (HBR-114, P1, M2): VIEW/SEND intent filters, `CFBundleDocumentTypes`, macOS `NSDocument` types — mirror the Office app's intake (after A5). Fix the duplicated `CFBundleDocumentTypes` in `apps/harbor_office/ios/Runner/Info.plist:11,49`.
3. Background work and notifications (HBR-113, P1, M2): scheduled Goals only run on an explicit tap today (`goals_view.dart:29-56`). iOS `BGAppRefreshTask`, Android `WorkManager`, macOS/Windows timers; local notification on due. Background download continuation belongs here (WS-4.5).
4. Desktop: macOS `minSize` and window-state persistence, `PlatformMenuBar`, drag-and-drop file open, Windows runner default size (hardcoded 1280×720, `windows/runner/main.cpp:28-29`), confirm `harbor_ffi.dll` bundling in `windows/CMakeLists.txt` and `release.yml` **[verify]**.
5. **Linux decision**: either add `linux/` runners + a libsecret keystore (today non-Apple/non-Windows targets fall to `FileKeyStore` plaintext root on disk, `harbor_store/src/keys.rs:118-180`), or remove the `Platform.isLinux` branches and say so in `08_Repo_Structure.md`. Record as decision 0023.
6. Device verification of what is simulator-only: record→transcribe happy path (never run on a device), vision-model scan through the UI, Work bar on touch, Android `content://` open-with.
7. OCR/ASR off Apple: Android `SpeechRecognizer` is wired for recordings; Windows has none (UI offers files). Windows OCR via Windows.Media.Ocr is a candidate under HBR-035's bridge pattern (decision 0009).

Done when: back-button test; open-with verified on iOS sim + Android emulator for harbor_app; a Goal fires while the app is backgrounded on a device (recorded); Linux decision written.

### WS-13 · Security hardening round 2  (P1, M)

Owns: `core/harbor_store/`, `core/harbor_net/`, `core/harbor_ffi` (knowledge schema), `core/harbor_core/src/diagnostics.rs`, `core/harbor_integration/tests/`.

1. C4, C8, C9, C10 from WS-1; then: make `Transport::execute` `pub(crate)` to the broker (enforcement is by convention today); HKDF for subkeys (`keys.rs:40-50` uses `SHA256(domain‖key)`); poisoned-mutex policy (every `lock().unwrap()` on the ops registry, knowledge index, OCR/ASR slots is a permanent subsystem death after one panic — `parking_lot` or `into_inner()` recovery).
2. Diagnostics: `record` fsyncs per record and re-reads/decrypts the whole log on every call (`diagnostics.rs:161-170,227-240`); roll by size/count.
3. Android root key crosses Dart as a hex `String` (non-zeroizable); evaluate a native-side handoff (JNI → core) so the key never enters the Dart heap.
4. The vendored llama.cpp `common` feature compiles `download.cpp` + cpp-httplib into the binary (unused, unbrokered HTTP client): disable the feature or document in the egress audit.
5. Re-run `plaintext_at_rest_inspection` and `network_capture` at the next freeze commit; run `/security-review` on WS-1..WS-5 diffs; update decision 0007 → 0024.

Done when: inspection PASS with the new sentinel placement; no `pub` transport outside the broker; security review recorded with none open at "high".

### WS-14 · Dependencies, toolchain, CI  (P2, M; some items gate others)

Owns: `core/Cargo.toml`, `core/Cargo.lock`, `core/deny.toml`, `core/.cargo/audit.toml`, `core/rust-toolchain.toml` (new), `.github/workflows/`, `apps/*/pubspec.yaml`, `packages/*/pubspec.yaml`, `third_party/`.

Ordered by risk:
1. **Now (S):** `rust-toolchain.toml` + `rust-version` (code uses `as_chunks`, stable since 1.88); remove unused `argon2` from workspace deps; `zip` 4 → ≥ 9 (clears RUSTSEC-2026-0367/0368/0369 from `deny.toml:66-68`; Harbor's own usage is read-only in `package.rs`); Flutter pin 3.47.4 → 3.47.8 (CA security patch) in `ci.yml:141` and `~/harbor-tools`; `flutter_lints` ^4 → ^6 and SDK floor to match; regenerate SBOM.
2. **CI (S):** `cargo check -p harbor_ffi --no-default-features` job (C12); Linux `cargo test` job; Android `cargo check --target aarch64-linux-android` (NDK, no Vulkan) job; Office in the flutter job (WS-8.6); a pub-workspace or melos so CI stops looping directories by hand; make `flutter pub outdated` report visible in the summary.
3. **ureq 2 → 3 (M):** with WS-4.
4. **Edition 2021 → 2024 (M):** `cargo fix --edition`; `#[unsafe(no_mangle)]` on the 6 FFI exports; `unsafe extern` blocks in `native_keystore.rs`, `ocr.rs`, `asr.rs`, `system.rs`; `unsafe_op_in_unsafe_fn`. After WS-7 so the split and the edition churn are separate commits.
5. **llama-cpp-2 0.1.156 → 0.1.160/161 (L, highest value, highest risk):** check whether the release vendors llama.cpp ≥ `4fbc76dec5` (gemma-embedding2); if yes retire `third_party/llama-cpp-sys-2` and keep only the two additive `llama-cpp-2` accessors (or upstream them); re-apply the build.rs fixes (cpp-httplib link, mtmd-debug skip) if still needed; re-run the embedding numerics canary, the EmbeddingGemma 2 live tiers, the device-GPU evidence (iPhone Metal, Adreno Vulkan CPU-fallback), and the skill live tier. Decision 0013 addendum. Note HBR-038 (prompt/template cache policy, P1, M1) is still open and is the WS-3.6 KV-cache work.
6. **formualizer 0.9.3 → 0.13 (L, defer until WS-0a + WS-15):** the Excel-fidelity patches live in `third_party/formualizer-eval`; upstream them first (open PRs), then upgrade and re-run the full 407-target qualification on every tier. Not before the Windows tiers exist, or the qualified set is invalidated twice.
7. `umya-spreadsheet` 3.1 → 4.0 when stable (two umya versions and four `zip` versions are in the binary today via formualizer).
8. `rand` 0.8 → 0.9/0.10 waits for `ed25519-dalek` 3 (`rand_core 0.6` pin); or switch nonces to `getrandom`.

Done when: `cargo deny check` has no accepted zip advisories; toolchain pinned; CI has Linux, Android-check, lean-build and Office jobs; the llama.cpp upgrade decision is recorded either way.

### WS-15 · Formula and Office qualification  (P1, M machine + operator)

Owns: `22_Formula_Coverage.json`, `21_Office_Feature_Matrix.csv`, `core/harbor_formula/`, `tools/qualify_ios`, `tools/device_qualification`, `docs/operations/formula-tier-runbook.md`.

1. After WS-0a: re-run the macOS, iOS-simulator, Android-emulator, physical-iPhone and physical-Android tiers at the merged commit (evidence binds to the commit; the branch's evidence is for 59a9269/2d2614c). Update the runbook's "99/99" line.
2. Windows x64 + arm64 tiers remain operator-bound (WIN-01). Nothing becomes PASS before the five-platform rule; say so.
3. Chart and conditional-formatting rows: fixtures + image-diff harness (the slide-scene renderer gives a deterministic raster path for PPTX; XLSX charts need a chart→scene mapping); then move rows from `REQUIRED_UNQUALIFIED` honestly.
4. Rebuild the Word authorities and re-issue `07_Screen_Inventory.csv` as authority **Revision 4** (seven surfaces, decision 0020 mapping, new rows for designer/threads), with 00/01/28 updated and the manifest resealed. Frozen files are only edited in a revision.

### WS-16 · Release track  (P1, operator + machine)

1. Push `main`; tag a throwaway `harbor-v1.1.0-rc0` to exercise the never-run `publish` job of `release.yml` (operator decision: it publishes a draft release).
2. With the physical iPhone and Android phone now in hand: run IOS-02 §4 steps 6–19 and AND-03 §10 from `closeout_runbook.md`, `--measured-class` per device; record device-class evidence. These two gates no longer need procurement.
3. The remaining five operator resources are unchanged; procurement status goes in `docs/release_checklist_status.md` (dated 2026-09-12 today — refresh it; it still describes 71 unqualified formula targets).
4. Bump `pubspec.yaml` to `1.2.0+3` at the WS-0 landing; `CHANGELOG.md` via `tools/generate_changelog.py`.
5. Deferred to 1.3 and later (M4, each with its own activation gate): sync UI (HBR-104–106, 156 — `harbor_sync` is a workspace member nothing depends on), remote OpenAI-compatible provider (HBR-036), system providers activation (decision 0009 bridge exists, off), connectors (HBR-101–103), app lock (HBR-162), Agents surface (hidden), diagnostics upload (HBR-145).

---

## 3. Sequencing

```
Phase 1 (weeks 1–2)  WS-0 land branches ─────────────┐   WS-14.1 toolchain/zip/Flutter pin  │ WS-16.1 push, rc0 tag
                     WS-1 hotfixes (core rows C1-C15; │   WS-14.2 CI jobs                     │ WS-16.2 device gates (operator)
                           app rows A1-A16) ─────────┘   (WS-1 core rows wait for WS-0c/lib.rs)
Phase 2 (weeks 3–5)  WS-2 async core   WS-4 downloads   WS-8 shared packages + CI for Office   WS-11 l10n/a11y
                     WS-3 model mgr    WS-5 retrieval   WS-13 security round 2                 WS-15.1 formula tiers re-run
Phase 3 (weeks 6–8)  WS-6 conversations (needs 2,3)    WS-9 Work canvas (needs 8)             WS-12 platform
                     WS-7 FFI split (alone on lib.rs, after 2)                                  WS-10 Office features (needs 8,9)
Phase 4 (weeks 9–10) WS-14.4-5 edition 2024, llama.cpp upgrade   WS-15.3-4 chart qualification, authority rev 4   WS-16 ring 0
```

Parallel lanes by file ownership:
- Lane R1 (Rust FFI/knowledge): WS-0c → WS-1 core → WS-2 → WS-7.
- Lane R2 (Rust inference/modelhub/net): WS-3, WS-4, WS-5, WS-13 — disjoint crates from R1 except shared `lib.rs` arms, which R1 owns; R2 lands core-crate changes and hands the `lib.rs` wiring to R1 as a small patch.
- Lane F1 (Flutter architecture): WS-1 app → WS-8 → WS-9 → WS-10.
- Lane F2 (Flutter product): WS-0d/e → WS-6 → WS-11 → WS-12.
- Lane Q (qualification/CI/deps): WS-14 → WS-15 → WS-16.

---

## 4. Exit criteria for the whole plan

- `main` holds the deck, threads and formula work; no stale branches.
- All WS-1 rows closed with tests; the **[verify]** items have a recorded outcome.
- No synchronous FFI call over 200 ms; cancel works for every op kind.
- One model registry; chat models unload; RSS after model switch measured.
- Resumable downloads; HF picker; 30 s deadline gone.
- Threads + streaming + markdown on Home and Work bar; live multi-turn test recorded.
- Shared `harbor_client` / `harbor_work`; both apps in CI; Office tests run without the dylib where possible.
- Six locales in both apps with an N-locale gate; guideline-matcher a11y test; goldens.
- `22_Formula_Coverage.json` at 407 targets with tiers re-run at the landed commit; Windows tiers named as the only open ones.
- Toolchain pinned, zip advisories gone, lean/Linux/Android/Office CI jobs, llama.cpp upgrade decided.
- Authority Revision 4 issued; `docs/release_checklist_status.md` refreshed; IOS-02 / AND-03 attempted on the real devices.

---

## 5. Appendix

### 5.1 Verified-today items (2026-10-28, main @ 95e80c6)

Confirmed by reading the source: C1 (`lib.rs:4718,4819` pass `never`), C2 (`gguf.rs:337` `token_eos` only), C3 (`index.rs` `add_source` has no `revoked` handling), C4 (`lib.rs:1428`), C5 (`acquire.rs` authorization only in `fetch`), C6 (`broker.rs:132` 30 s → `transport.rs:42,91` `.timeout`), C7 (`knowledge.rs:1279` insert before load), C12 (`lib.rs:5109` `parse_sources` ungated), A1 (`main.dart:374`, no spread of defaults), A2 (`office main.dart` has `locale:` and delegates but no `supportedLocales`), A3 (`office preferences.dart:102`), A4 (both `work_surface.dart` files), A6 (`abi_version` only in `harbor_ffi.dart:100`), A9 (`workbook_view.dart:197`).

Confirmed by running it: `cargo check -p harbor_ffi --no-default-features --offline` on `main` @ 95e80c6 **fails** (exit 101, 2 errors: E0425 and E0433 at `lib.rs:5109-5128`). The last recorded green lean check was 2026-10-10 (`docs/HANDOFF_2026-10-10.md` §1); the session-61 `document` scope work broke it and CI has no job that would have caught it.

### 5.2 Not verified (needs runtime)

A1 effect on Tab/Escape; A2 locale resolution; C6 whether any >30 s download ever succeeded (the network-capture tier used small files); C2 per model; C15 which formualizer inputs `Err`; Windows DLL bundling; IME/Enter conflict; mobile background download death.

### 5.3 Commands used for the facts in §1

```bash
git rev-list --count origin/main..main
git diff --stat main...designed-decks-and-motion | tail -1
comm -12 <(git diff --name-only 553d243 main | sort) <(git diff --name-only 553d243 designed-decks-and-motion | sort)
git show designed-decks-and-motion:22_Formula_Coverage.json | python3 -c "import json,sys;d=json.load(sys.stdin);print(sum(len(g.get('targets',[])) for g in d['target_groups']))"
python3 tools/validate_dossier.py && python3 tools/check_dossier_seal.py && python3 tools/test_contracts.py
grep -c '#\[test\]' -r core --include='*.rs' | awk -F: '{s+=$2} END {print s}'
```
