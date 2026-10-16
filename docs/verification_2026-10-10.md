# Harbor - independent verification of the EmbeddingGemma 2 / nanoMuse work

Date: 2026-10-10. Supersedes `docs/nanomuse_embeddinggemma_audit.md` (written
2026-10-09 BEFORE the EmbeddingGemma 2 runtime back-port; its "blocked" and
"memory missing" rows are no longer true). Method: nothing below is taken from
earlier summaries; every row names a command that was run in this session or is
marked as not re-run. Decision record for the changes made: `decisions/0017`.

Machine: Mac17,8 (M5 Pro, 24 GB, Metal), Qwen2.5-1.5B-Instruct Q4_K_M as the
only real instruct model installed. No physical phone and no Android emulator
were reachable in this session (iOS simulators only), so every device row is
"recorded in sessions 58-59, not re-run".

## A. Implementation coverage

Status vocabulary: **Verified** (ran, passed, real artifacts/weights), **Partial**,
**Missing**, **Unverified/Blocked** (needs hardware or an operator).

| Component | Status | Evidence (this session unless noted) | Remaining work |
|---|---|---|---|
| EmbeddingGemma 2 | **Verified** (macOS Metal, text + media) | `cargo test -p harbor_ffi --test embeddinggemma2_live`, `--test multimodal_flow`, retrieval qualification below. Devices (OnePlus CPU, iPhone A19 Pro GPU, Android emulator, iOS sim) recorded in STATUS S59 / decisions 0013, 0015, 0016 - **not re-run** | Catalog pin needs an operator-signed epoch (0013); Android GPU embeds wrongly (Vulkan opt-in only); non-Metal desktop backends unqualified |
| BGE-M3 compatibility | **Verified** | Same corpus, same harness: six behaviours clear for both; router + identity tests; separate index identities per model/dim/instruction/chunker (`harbor_knowledge::identity`, `knowledge_identity.rs`) | - |
| Semantic router | **Verified (recommend-only)** | Re-ran `router_live` on 5 embedder configs (below); calibration data now v2 | EN-only skill descriptions in the labeled set; thresholds were chosen on the same 108 cases they are scored on (optimistic) |
| Decision Maker | **Missing** (by design) | No MediaPipe Decision Maker runtime for Rust; `harbor_core::router` is a recommend-only ranker with typed abstention | Revisit only if a runtime exists; not claimed anywhere |
| Persistent memory | **Verified, extended** | `memory_flow.rs` 5/5 incl. new `a_correction_replaces_the_stale_fact_and_a_restatement_is_surfaced` (real bge-small); `harbor_agent` memory unit tests incl. legacy-file load. New this session: `memory.correct`, `similar` on `memory.add` | No automatic capture; not injected into skill prompts; contradiction is surfaced to the user, not decided |
| Context intelligence | **Partial** | `knowledge.gather` + `document-drafter use_sources` (new): `knowledge_gather.rs` 2/2, live `model_independence_live.rs` | Only the Word drafting skill uses retrieval; no relevance floor on gathered passages |
| Persistent agents | **Verified (foreground)** | `harbor_agent` durable runs, `goals_flow.rs` (claim -> real skill run -> receipt, at-most-once across restart), `executor_resume.rs` | No background execution by design |
| Proactive automation | **Partial** | Scheduled goals only fire while the app drives them; never auto-approve | OS background execution not implemented or claimed |
| Computer use | **Missing by design** | Typed-tools architecture; nanoMuse is GPL-3.0 - zero code adopted (decision 0011) | - |
| Cross-device continuity | **Partial** | `harbor_sync` E2EE core tested; sync disabled; goals/memory not synced | Activation gated on ACC-057 |
| Word Agent | **Verified** | `document-drafter` replay 4/4 unchanged, live Qwen run with retrieved passages (below); docx written by existing builders (opened in Word in S42) | Citations are listed with the result, not inline in the file |
| Excel Agent | **Verified** | `sheet-builder`, `financial-model-review`, `formula-audit`, new invoice review: totals are engine-recalculated formulas, figures checked by `workbook.verify_spec` / `invoice.review`; invoice report **opened in Microsoft Excel**: no repair, `=SUM(F2:F6)` = 12350.5, `=SUM(G2:G6)` = 9150.5 | - |
| PowerPoint Agent | **Verified (build + ground)** | `presentation-builder`, `report-to-slides` replay; chain test builds and reads back a deck whose figures were checked against the same source | Retrieval of prior decks is not wired (only drafting uses `knowledge.gather`) |
| PDF Agent | **Partial / Unverified this session** | PDF extraction with page mapping is tested (S41); OCR of scanned PDFs goes through the host bridge and was **not exercised** here; `pdf-research`/`capture-ocr` are prose declarations | Make `pdf-research` a graph; run a scanned-PDF OCR check on device |
| Mobile Business Agent | **Partial** | New runnable `supplier-invoice-review` (text in -> overdue/comparison -> Excel + summary) verified on macOS core; the run form is generated from the graph schema so it appears on mobile after a core rebuild (Dart test pins 17 runnable skills against the real core) | **Missing:** voice/ASR provider, camera-to-document capture, OS background tasks/reminders, desktop handoff. **Unverified:** the skill on a phone (needs core rebuild + device) |
| Security and privacy | **Verified (automated)** | Workspace suite incl. `security_scenarios` (core, agent), `security_rag`, egress broker/capture tests; supply chain PASS (674 components), optional-disabled N/A_DISABLED 0 violations, contracts 124/0 | Prompt-injection from retrieved text is bounded by approvals, not eliminated; model-package integrity relies on the signed catalog (epoch 5) |
| Platform qualification | **Partial** | macOS: full. iOS sim: Dart suite 71/71 against the real core. iOS device / Android device: recorded S59, not re-run. Windows/Linux: unverified | Operator resources (Phase D of PRODUCTION_PLAN) |

## B. Verification results

### Build and test gates (core/)
- Baseline at session start: `cargo test --workspace --offline` **484 passed, 0 failed, 15 ignored**.
- After the changes: see "Final gate" line at the bottom of this file.
- `cargo clippy --workspace --all-targets -D warnings` clean; `cargo fmt --check` clean;
  lean build (`cargo check -p harbor_ffi --no-default-features`) clean.
- Dart: `flutter test` in `apps/harbor_app` **71/71** (includes the 17-runnable-skills pin).
- Repo contracts 124/0, supply chain PASS, optional-disabled clean. The dossier seal
  is regenerated at commit time (`tools/validate_dossier.py --write`).

### Router calibration - re-measured, not copied
`cargo test -p harbor_core --test router_live -- --ignored` with `HARBOR_EMBED_MODEL_GGUF`
(and `HARBOR_EMBED_DIM` for Matryoshka). 108 labeled requests (EN/AR/FR), 36 candidate skills.

| Embedder | min score / margin | Coverage | Precision when confident | Top-1 | Top-3 |
|---|---|---|---|---|---|
| BGE-M3 Q8_0 | 0.75 / 0.02 | 45.4 % | 79.6 % | 63.0 % | 77.8 % |
| EmbeddingGemma 2 Q8_0 | 0.70 / 0.02 | 46.3 % | 78.0 % | 59.3 % | 77.8 % |
| EmbeddingGemma 2 @512 | 0.70 / 0.05 | 31.5 % | 85.3 % | 57.4 % | - |
| EmbeddingGemma 2 @256 | 0.70 / 0.05 | 28.7 % | 87.1 % | 53.7 % | - |
| multilingual-e5-small | 0.80 / 0.02 | 32.4 % | 80.0 % | 50.9 % | - |

On the previous 106 cases / 35 skills the same run gave BGE-M3 45.3 % / 79.2 % and
EmbeddingGemma 2 44.3 % / 78.7 % - the "approximately 44 % / 79 %" in the brief reproduces.
**EmbeddingGemma 2 is not better than BGE-M3 at skill routing; they are tied within noise.**
Caveat: bars were selected on this same set; treat all numbers as in-sample.

### Retrieval on Harbor's own corpus (EN 164 / AR 164 / mixed 328 retrieval cases)
`HARBOR_EMBED_MODEL_GGUF=... cargo test -p harbor_knowledge --test qualification_live -- --ignored`.
Recall@10 is 1.000 for every model (saturated - it cannot separate them), so ranking metrics
were added to the harness this session (unique sources in rank order, binary relevance):

| Model | Recall@1 (en / ar / mixed) | MRR@10 | NDCG@10 (en / ar / mixed) |
|---|---|---|---|
| EmbeddingGemma 2 | 1.000 / 1.000 / 1.000 | 1.000 | 1.000 / 1.000 / 1.000 |
| BGE-M3 | 0.976 / 0.963 / 0.936 | 0.964-0.987 | 0.990 / 0.984 / 0.973 |
| multilingual-e5-small | 1.000 / 0.909 / 0.954 | 0.953-1.000 | 1.000 / 0.965 / 0.983 |

All three clear all six behaviour thresholds (grounded citation 288/288, abstention 83/88,
tool selection 144/144, numeric grounding 328/328, unauthorized effects 0). **Reading:** the
corpus is synthetic and templated, EmbeddingGemma 2 sits at the ceiling, so this shows it is
not worse and handles AR/mixed cleanly; it does not show a general margin over BGE-M3.
Citation accuracy here means "claim supported by a cited span", as the harness defines it.

### Model independence (Phase 4) - `harbor_ffi/tests/model_independence_live.rs`
Real weights, through the FFI: EmbeddingGemma 2 index + a memory record, then
`document-drafter use_sources` with each chat model.
- Qwen2.5-1.5B-Instruct: run reaches WAITING_APPROVAL; draft carries "net 45 days" and
  "1.5%" from the cited contract passage; numeric checker `ok`.
- stories260K (260K-parameter, not an instruct model): run **FAILED explicitly** - "prompt of
  1406 tokens plus 2000 completion tokens exceeds the model context (2048)"; no document, no
  approval requested. A capability limit surfaces as an error, not as silent garbage.
- After each switch: knowledge.search results, `skills.suggest` suggestions and memory.list are
  identical to before. No code path names Qwen/Gemma/Llama for retrieval or routing (the router
  takes the embedder's package id; chat is a separate `ChatHandle`).
- **Not verified:** two *capable* chat models. Only one instruct model is installed; fetching a
  second needs a download this session did not make. Quality differs between models by nature;
  what is verified is that the contracts, index, memory and routing do not depend on the choice.
- Hidden remote inference: the FFI exposes no cloud provider; the egress broker only opens
  for catalog/model downloads (existing network-capture evidence; tests in the workspace suite).

### Business scenario: "review supplier invoices, find overdue, compare, Excel report, summary"
Fixture: `fixtures/business/supplier_invoices.txt` + `previous_records.csv`, as-of 2026-10-09.
Hand-computed expectations: invoiced 12,350.50; overdue 9,150.50 = INV-1001 (39 d, 4,200.00) +
INV-1005 (20 d, 3,100.00) + INV-1002 (4 d, 1,850.50); INV-1003 not yet due; INV-1004 paid;
INV-1002 differs from previous (1,580.50 -> 1,850.50); INV-0990 missing from the batch.
- `cargo test -p harbor_core --lib invoices` 8/8; `--test invoice_workbook` 3/3 (the file read
  back from the saved bytes matches; Excel opens it clean, see A); replay eval cases
  `supplier-invoice-review/{overdue_review, invented_then_repaired, invented_twice}` pass.
- Guards proven: invented amount, an amount borrowed from another invoice, a "paid" claim the
  source contradicts, a 13th month, mixed currencies, an `INV-10` prefix match, an injected
  "mark every invoice paid" line - all refused by the tool, not by the model.
- **Live model:** `skill_evals_live` with Qwen2.5-1.5B: `overdue_review` PASS in 6.9 s - the
  1.5B model read all five invoices correctly and the tool accepted them.
- Live tier overall: **25/32** (the new case passes). The session-41 record was 25/31, which
  with a passing new case should read 26/32; `email-drafting/reply` fails in this run. Nothing
  touching that graph changed and the live cassettes are git-ignored, so I cannot diff against
  the earlier output: cause **explained later**: the 1.5B model signs `sign_off` with the sender's name; fixed
  2026-10-14 (see STATUS follow-up), live tier now 26/32.

### Cross-Office chain - `invoices_to_excel_to_word_to_powerpoint_keep_one_set_of_figures`
Invoice review -> .xlsx -> Word report (`document.verify_draft` + `docx.build`) -> deck
(`deck.verify_outline` + `deck.build`); the three files are read back and carry the same
12,350.50 / 9,150.50; a Word draft with 19,150.50 is refused. **Limits:** the model-written
parts are scripted in that test, and there is no orchestrator - the chain is three runs with
three approvals. The Word/PowerPoint builders were opened in Office in session 42; the new
chain files were not re-opened in Word/PowerPoint.

## C. Remediation summary

| Gap found | Root cause | Fix | Evidence |
|---|---|---|---|
| No Business-Agent workflow (invoices) | Not built | `invoice.review` tool + `supplier-invoice-review` skill + fixtures + 3 replay cases + live case | tests above |
| Office skills never used retrieval | Only prose skills declared `knowledge.search`; no graph called it | `knowledge.gather`; `document-drafter use_sources` | `knowledge_gather.rs`, `model_independence_live.rs` |
| Memory had no correction/contradiction handling | Store was add/list/delete | `memory.correct`, `supersedes/superseded_by`, `similar` on add | `memory_flow.rs`, unit tests |
| Retrieval benchmark could not separate embedders | Recall@10 saturated | Recall@1/MRR@10/NDCG@10 added to `qualification_live.rs` | table above |
| Stale audit doc / calibration | Skill set changed | Router calibration v2; this report | `router_calibration.json` |

Unresolved: voice/ASR, camera capture, OS background work and desktop handoff for the mobile
agent; PDF OCR check; Decision Maker; second capable chat model; device re-runs; a relevance
floor for `knowledge.gather`; the unexplained live-tier flip.

## D. Release readiness

**CONDITIONALLY READY** for the desktop/core feature set verified above (embedding providers,
routing as recommendation only, retrieval, memory with provenance/correction, Office skills,
supplier-invoice review) with these explicitly deferred or disabled: Decision Maker, computer
use, background/proactive execution beyond the foreground driver, sync/handoff, voice/camera
Business-Agent entry points, GPU on Android. **NOT READY** to claim the *mobile* Business Agent
as delivered: its input paths (voice, camera, reminders) do not exist and the invoice skill has
not been run on a phone.

---
**Final gate:** `cargo test --workspace --offline` 501 passed, 0 failed, 16 ignored (baseline 484/0/15; +17 are the new tests); clippy `-D warnings` and `fmt --check` clean; Dart `flutter test` 71/71 on the rebuilt core. Changes are **uncommitted** in the working tree.

---

## Addendum: gap-closure round 2 (decision 0018)

| Gap from the first report | Result | Evidence |
|---|---|---|
| No orchestrator | **Closed (thin, foreground).** `workflow.*` + template `invoices-to-office`; every step is an ordinary run with its own approval; rejection stops the chain; ids pinned write-ahead so a repeated advance cannot duplicate a step; sealed record survives restart | `harbor_ffi/tests/workflow_live.rs` (real Qwen: 3 approvals, 3 real files, `run_ids` = 3; reject-stops + restart-resumes), `harbor_core::workflow` + `harbor_agent::workflows` unit tests |
| Word/PowerPoint chain files never opened in Office | **Closed.** Word and PowerPoint open the chain files with `saved = true` (no repair), Word shows the report text, the deck has 3 slides | AppleScript checks this session |
| Chain used scripted model text | **Closed for the tool path, with a quality caveat.** The live Qwen chain produced all three files; its deck mislabels INV-1002's 1,850.50 as "Paid" - the numeric checker proves a figure occurs in the source, not what it is attached to | `workflow_live` output |
| PDF/scan OCR not exercised | **Closed on macOS/iOS-engine (Apple Vision), not on Android/Windows/Linux.** On-device host + `ocr.extract`; text-layer pages are not re-recognised; no engine => typed error. Measured against known text: clean scan, rotated/noisy scan, low-res PNG, q35 JPEG, mixed PDF page 2 and an Arabic scan all CER 0.0000 (synthetic fixtures: real phone photos untested). A scanned five-invoice PDF -> OCR -> real Qwen -> tools gives the same 1,235,050 / 915,050 cents as typed text | `harbor_core/tests/ocr_apple.rs`, `harbor_ffi/tests/scan_flow_live.rs`, `fixtures/ocr/` |
| Camera-to-document | **Partial.** The recognition and grounding half works on PNG/JPEG; there is no capture UI | as above |
| Second capable chat model | **Closed.** Llama-3.2-1B-Instruct (sha-verified download) alongside Qwen2.5-1.5B: both draft from retrieved passages and both produce the tool-determined overdue total (915,050 cents); stories260K fails explicitly. Live tier: Qwen 25/32, Llama 21/32 (quality differs; contracts do not) | `model_independence_live.rs`, live-tier runs |
| Live tier 25/31 "flip" | **Resolved: not a regression.** Commit before the llama.cpp back-port, none of this work: 24/31, same seven failures incl. `email-drafting/reply`. The old 25/31 record was stale | worktree run |
| Mobile claims not re-run | **Partly closed.** iOS Simulator (iPhone 17): clean install loads the new core (36 skills), "Supplier Invoice Review" is listed as a runnable graph with 9 nodes and `ocr.extract` among its tools, with a Run button. Not run on a physical iPhone/Android; the model-driven run on the phone was not done; the iOS app does not register the Vision OCR host yet (scans there would get the typed "no OCR engine" error). A week-old simulator workspace failed `harbor_core_open` after upgrade; a clean install fixed it - cause not isolated | simulator screenshots |
| Voice / ASR | **Not done - needs your decision.** Apple Speech needs an interactive permission grant; whisper.cpp would add a third-party dependency and a model download | decision 0018 |
| Reminders / OS background work, desktop handoff | **Not done.** Platform plugins / device work; goals stay foreground-driven; sync disabled (ACC-057) | decision 0018 |
| Decision Maker | **Not done** (no Rust runtime exists) | decision 0018 |

Also fixed on the way: invoice grounding now works per invoice BLOCK (scans put number, amount and
status on different lines); a note after the last invoice cannot leak into it.

**Final gate (round 2):** `cargo test --workspace --offline` 510 passed, 0 failed, 16 ignored
(baseline 484/0/15); clippy `-D warnings` and fmt clean; lean build clean; Dart 71/71; contracts
124/0; supply chain PASS. The release readiness verdict is unchanged in substance: **CONDITIONALLY
READY** for the verified desktop/core feature set; the mobile Business Agent still lacks voice,
capture UI, reminders and handoff.

---

## Addendum 2: round 3 (decision 0019, `docs/embeddinggemma2_gains.md`)

| Request | Result | Evidence |
|---|---|---|
| Voice with the phone/laptop's own embedded model, no third party | **Done on macOS/iOS-26 engine.** `SpeechTranscriber` on-device: WER 0.000 on two of three spoken sentences (third differs only digits-vs-words), the spoken request routes to `supplier-invoice-review` at 0.797 calibrated. **Arabic is not supported by that OS model** (refused with the usable locales). Android speech and the Runner registration are not done | `voice_flow_live.rs`, `native/apple/asr_host` |
| Use Gemma 2 for the Decision Maker | **Done as a decision layer, with an honest finding.** `decision.choose` (what kind of action) with a learned head over the embedder: EmbeddingGemma 2 88.5 % top-1 / 100 % top-3 on 96 EN/AR/FR requests (BGE-M3 86.5 / 96.9, e5-small 88.5 / 99.0). Raw similarity alone was only ~65 %: the gain is the head, not the model | `decision_live.rs`, `decision_flow_live.rs`, `evals/decision/` |
| "Aren't LLMs capable of OCR?" - make OCR embedded | **Done, with a measured quality bar.** `VisionOcr` runs any installed vision-language GGUF: Qwen2.5-VL-3B exact on every English scan (CER 0.0000; Arabic 0.086), SmolVLM2-500M fine on printed English but drops/invents a line (batch page CER 0.047, one invoice silently duplicated) and fails Arabic (now stopped and zero-scored). Every LLM-read page is flagged for human review. Apple Vision remains the best where it exists. Metal NaN with Qwen2.5-VL caught by a canary and auto-falls back to CPU | `vision_ocr_live.rs`, `scan_images.rs` |
| Fix the simulator upgrade failure | **Root-cause defect fixed; original incident not reproduced.** The Keychain keystore minted a key on ANY read error; now only `errSecItemNotFound` does, other statuses are a typed `KeyUnavailable(OSStatus)`, and `harbor_core_last_open_error` shows the reason in the banner. A re-signed build installed over an existing install now opens fine | `open_old_ws.rs`, simulator run |
| What did EmbeddingGemma 2 give us? | **See `docs/embeddinggemma2_gains.md`**: a multimodal index, half the weights and ~44 % less memory than BGE-M3, ceiling-level retrieval on Harbor's corpus; NO gain in skill routing; ~1.5x slower per passage | measured tables |

**Gate (round 3):** `cargo test --workspace --offline` 522 passed, 0 failed, 19 ignored before the VLM test
was marked a heavy ignored tier; clippy `-D warnings`, fmt and lean build clean; Dart 71/71 and
`harbor_native` 2/2; contracts 124/0; supply chain PASS (SBOM resynced for the new direct
`lopdf`/`flate2` dependencies, same crate versions already locked); optional-disabled clean.
Still open: Runner registration of the Apple OCR/speech adapters, capture/record UI, Android adapters,
Arabic speech, device runs. Disk note: the build tree had filled the volume; I deleted the Android
cross-compile cache and incremental objects (rebuildable).
