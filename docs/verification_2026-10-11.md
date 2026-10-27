# Harbor — verification record for the production-finishing sessions (2026-10-11)

Covers the five sealed slices that implement handoff tasks T1–T5 of
`docs/HANDOFF_2026-10-10.md`, plus the re-run of the heavy live tiers on the
final tree (HEAD `20f7bc9`, working tree clean). Companion to
`docs/verification_2026-10-10.md`, which this record does not supersede —
everything verified there (retrieval, routing, calibration, Office chain,
workflow) still stands on this tree; only the areas touched since are re-run
here.

## A. Work committed in sealed slices

| Commit | Slice | Gate at seal |
|---|---|---|
| `f88cf82` | T1: register Apple OCR/ASR hosts with the core from the runners (iOS AppDelegate + pbxproj, macOS runner, `models.install_from_path` extra-files) | flutter analyze clean; flutter test green; seal fresh |
| `ece7135` | T2: read-class tool reads run as a cancellable background op (`op.start_tools_read`/`cancel` + op surfaces) | flutter analyze clean; flutter test green; seal fresh |
| `dea5d39` | T3: capture and scan into the composer, reviewed before use (`CaptureService`, iOS `HarborCapture.swift`, mic/photo flows, review-before-send) | flutter analyze clean; flutter test green; seal fresh |
| `e415f1b` | T4: open the vision OCR engine from the Models surface (`models.installed` `roles` in core, `ocr.open`/`ocr.release` bindings, "Use for scanning" action, l10n en+ar, 6 new widget tests) | `cargo test -p harbor_ffi` 22 binaries ok; dylib rebuilt; flutter test 91/91; seal fresh |
| `20f7bc9` | T5: Android capture host — recorder, camera, on-device speech (`HarborCapture.kt`, MainActivity wiring, manifest RECORD_AUDIO + FileProvider + IMAGE_CAPTURE query, `CaptureService.transcribeFile`, on-device-first record flow) | `:app:compileDebugKotlin` BUILD SUCCESSFUL (JBR 25); flutter analyze clean; flutter test 93/93; seal fresh |

Every slice was staged first, then `tools/validate_dossier.py --write` +
`tools/check_dossier_seal.py` ("dossier seal is fresh") before the commit; the
tree is clean at HEAD. Final-tree gate re-run for this record: full
`flutter test` green (see §B), all heavy live tiers below green.

## B. Heavy tiers re-run on the final tree (§4 commands, real models)

All runs on HEAD `20f7bc9` from `core/`, fixtures under `fixtures/models/`
(note: the test binaries resolve `HARBOR_*` paths from the package directory,
so absolute fixture paths are required — a `../fixtures/...` value fails with
"embedding model missing").

| Tier | Result | Detail |
|---|---|---|
| `ocr_apple` | ok (1/1, 43.5 s) | Apple Vision reads the scan fixtures on device, CER 0.0000 |
| `voice_flow_live` | ok (1/1, 25.3 s) | spoken request transcribed on device (SpeechTranscriber) and routed like typed text |
| `router_live` | ok (1/1, 26.5 s) | EmbeddingGemma 2 routes the builtin eval inputs; histogram emitted |
| `decision_live` | ok (1/1, 31.9 s) | head threshold re-chosen for embeddinggemma2 `(0.96875, 0.3)`; sweep precision 1.000 |
| `qualification_live` | ok (1/1, 84.1 s) | all six gates CLEAR: retrieval_recall@10 328/328, citation_supported 288/288, abstention 83/88, tool_selection 144/144, known_answer_numeric 328/328, unauthorized 0; evidence written to `evidence/knowledge_evals/live-2188ac1deca4.json` (gitignored) |
| `workflow_live` | ok (2/2, 147.4 s) | real-Qwen 3-approval chain produces the three files; deck caveat from 2026-10-10 unchanged |
| `decision_flow_live` | ok (1/1, 84.6 s) | same decisions from either embedder, nothing authorized |
| `model_independence_live` | ok (1/1, 187.7 s) | switching the chat model leaves retrieval/memory/routing untouched |
| `scan_flow_live` | ok (1/1, 89.2 s) | scanned invoice batch → same verified report as typed text |
| `vision_ocr_live` | ok (1/1, 515.6 s) | Qwen2.5-VL-3B: CER 0.0000 on scanned_invoice_en (conf 0.995), degraded (0.998), photo (0.998), batch (0.999); Arabic 0.0860 (conf 0.985) — identical to the 2026-10-10 record |
| `skill_evals_live` Qwen2.5-1.5B | **25/32 PASS** | the same seven known failures as at handoff: presentation-builder/exact_count, meeting-notes/owners_en, email-drafting/reply, doc-coauthoring/decision_memo, second-look/model_skip, formula-audit ×2 — no regression, no new failure |
| `skill_evals_live` Llama-3.2-1B | **21/32 PASS** | matches the 21/32 recorded on 2026-10-10 (quality differs; contracts do not) |

No skill set or exemplar changed, so the router/decision calibration JSON
`version` fields were **not** bumped (per the checklist rule).

App-level gates on the final tree: `flutter analyze` clean; full
`flutter test` **93/93 re-run green on HEAD `20f7bc9`** for this record (the
suite includes `vision_ocr_test.dart` 6/6 and `capture_test.dart` 14/14); the
debug dylib was rebuilt after the cargo test runs (cargo test removes it —
known quirk).

## C. Task status against the handoff

- **T1 (Apple OCR/ASR registration) — Done, live-verified.** The runners
  register the platform hosts before `harbor_core_open`; `ocr_apple` and
  `voice_flow_live` pass against the freshly built `libharbor_ocr_host.dylib`
  / `libharbor_asr_host.dylib` on this tree.
- **T2 (async read-class tools) — Done.** `op.start_tools_read` +
  cancellation, op list surfaces, widget coverage; slice `ece7135`.
- **T3 (capture/record UI) — Done on the app side.** Mic recording and photo
  capture flow into the composer behind an explicit review step; widget tests
  cover the flows; slice `dea5d39`. Runtime on hardware: see T7.
- **T4 (vision-LLM OCR in the app) — Done.** `models.installed` now reports
  file `roles`; packages with an `mmproj` file offer "Use for scanning",
  which opens the portable engine via `ocr.open`; later `ocr.extract` calls
  (composer scan) route through it. Live quality on the final tree matches
  the record above. Slice `e415f1b`.
- **T5 (Android) — Compile-verified; runtime Unverified.**
  `:app:compileDebugKotlin` succeeds and the Dart side is unit/widget tested,
  but no emulator or device run was made: RECORD_AUDIO grant flow,
  ACTION_IMAGE_CAPTURE resolution and SpeechRecognizer were **not** exercised
  at runtime. Recorded honestly as Unverified, not Pass.
- **T6 (Windows/Linux host) — Blocked** (no Windows/Linux host available in
  this environment).
- **T7 (physical devices) — Blocked (operator).** No phone reachable; the
  iOS-simulator engine-registration checks from the 2026-10-10 record stand;
  Android runtime remains Unverified until someone runs the app on a device
  or AVD.
- **T8 (quality gaps) — Open, unchanged** (semantic deck check, relevance
  floor, more retrieval consumers, held-out decision/router sets, Arabic
  speech/OCR, live-tier prompt work). The Qwen 25/32 line is the same seven
  failures documented at handoff.
- **T9 (operator-bound release steps) — Blocked (operator)**: signed catalog
  epoch, Apple Developer identity/TestFlight, Play upload key, Windows host,
  M1 perf qualification, `release.yml` run.

## D. Security matrix check (`09_Security_Test_Matrix.csv`)

No new network paths were introduced by T1–T5: the Apple hosts wrap OS
frameworks (Vision/Speech/AVFoundation), the Android host uses
`MediaRecorder`/`SpeechRecognizer`/`ACTION_IMAGE_CAPTURE` with a FileProvider
scoped to `cacheDir/capture/`, and the Dart side talks to the in-process FFI.
Egress control SEC-030 rows are untouched; the standing
`evidence/network_capture.json` capture binds to its commit and was not
invalidated (no dependency or network-code changes landed in these slices).
Prompt-injection grounding for scanned/transcribed content re-verified by
`scan_flow_live`/`workflow_live` (numeric checkers + approvals per step).
Permissions: RECORD_AUDIO is now declared on Android with the mic string;
CAMERA is deliberately undeclared (system camera app handles it). Protected
effects still require per-step approval — `workflow_live` proves it on this
tree.

## E. Licence / notices / SBOM refresh

`tools/generate_third_party_notices.py` and `tools/generate_sbom.py` re-run on
the final tree: **byte-identical output** (git reports no diff) — T1–T5 added
no new direct dependencies (core changes are a `roles` field; app changes are
platform APIs only). Model licences stand as recorded on 2026-10-10
(EmbeddingGemma 2 Apache-2.0, SmolVLM2 Apache-2.0, Llama-3.2 community
licence); **Qwen2.5-VL-3B remains unrecommended for catalog bundling until its
licence is confirmed** — it is exercised only from local fixtures.

## F. Release verdict

**CONDITIONALLY READY** — broader than 2026-10-10: the mobile Business Agent's
input paths now exist and are verified at the contract level. Specifically
**READY (desktop/core, macOS + iOS engine)**: on-device OCR (Apple Vision and
portable vision-LLM), on-device speech (Apple), capture→review→send flows,
async read ops, workflow/retrieval/memory/Office skills — all proven by the
live tiers in §B. **Unverified, not claimed**: Android capture runtime
(compiles; never run), vision-LLM OCR opened from a physical device.
**Blocked (operator)**: T6 Windows/Linux host, T7 phone runs, T9 signed
catalog/TestFlight/Play/perf qualification. **NOT READY** to ship the
catalog-bundled Qwen2.5-VL-3B (licence) and to claim background/reminders
execution (T8, by design foreground-only).

The single biggest remaining risk to the mobile claim is that T5 was never
executed on an Android runtime; a simulator/AVD smoke run
(record → transcript → scan → review) would upgrade it from Unverified, and
the operator steps in T9 gate the release itself.
