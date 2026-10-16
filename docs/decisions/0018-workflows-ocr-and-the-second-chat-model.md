# Decision 0018 — Multi-skill workflows, on-device OCR, and a second chat model

Date: 2026-10-10
Status: Accepted (implemented and tested). Follows decision 0017 and closes the
gaps its report listed.

## 1. Workflows (`harbor_core::workflow`, `harbor_agent::workflows`, `workflow.*`)

The missing orchestrator is deliberately thin. A workflow is an ordered list of
ORDINARY skill runs. It adds no execution path and no policy: every step has its
own approvals, protected effects, budgets and receipts, and the person approves
each file (`run.commit_proposal` / `run.decide`) exactly as for a hand-started run.

- Template `invoices-to-office`: `supplier-invoice-review` (Excel) ->
  `document-drafter` (Word management report) -> `presentation-builder`
  (PowerPoint). Steps 2 and 3 take the review's verified `summary_text` (read
  from step 1's finished run snapshot by JSON pointer), never a model's
  paraphrase of it. A missing prior value is an error, not an empty string.
- `workflow.start` (validates inputs, starts step 1), `workflow.advance` (starts
  the next step only if the current run COMPLETED with every approval granted;
  waiting returns `waiting`, a rejection / failure / cancel / missing proposal
  STOPS the workflow with a reason), `workflow.status`, `workflow.list`,
  `workflow.templates`.
- Durability and duplicate-action prevention: the record is a sealed store
  (domain-separated subkey, like memory/goals). The next run id is written
  BEFORE the run starts and pinned as that run's id, so a repeated or
  post-crash `advance` re-starts under the same id (idempotent) instead of
  creating a second step; `run_ids` can never shrink. The outputs live in the run
  snapshots, so a restart loses nothing.
- Not goal-eligible and not background: the app drives `advance`; iOS cannot
  host unattended execution and nothing here claims it.

## 2. OCR (`harbor_inference::ocr`, `ocr.extract`, `native/apple/ocr_host`)

Platform owns the engine (Apple Vision + PDFKit), Harbor owns policy: the host
ABI (`native/apple/include/harbor_ocr_host.h`) refuses any non-on-device host,
the result is a typed tool output (engine, per-page text, `text_layer` vs `ocr`,
confidence, pages to check by eye), PDF pages that already carry text are NOT
re-recognised, page numbers survive for citation, and with no engine a scanned
PDF is a typed `Unavailable` ("1 of 2 page(s) have no text layer ... no OCR
engine") instead of an empty document. `supplier-invoice-review` and the
workflow accept an attached scan or photo (`artifact_id`).

Grounding changed with it: an invoice's amount and status are now checked in the
invoice's own BLOCK (its number's line up to the next different invoice number,
a blank line, or 8 lines), because on a scan the number, amount and status sit on
different lines. Text after the last invoice (a footer, an injected note) stays
out of the last block.

Android has no OCR host yet (ML Kit would be the equivalent) and Windows/Linux
none: on those platforms scans are refused with the typed error, text layers
still work.

## 3. Second chat model

Llama-3.2-1B-Instruct Q4_K_M (bartowski GGUF, 807,694,464 bytes, sha256
`6f85a640...df83`, checked against the repository's published hash) joined
Qwen2.5-1.5B as a second capable, different-family chat model (git-ignored like
the other large fixtures).

## 4. Live-tier "flip" resolved

On the commit before the llama.cpp back-port with none of this work, the live
tier is 24/31 with the same seven failures (including `email-drafting/reply`).
The session-41 "25/31" record was stale; nothing regressed.

## Still not done (needs a decision, hardware or a platform permission)

- Voice: no ASR engine. Apple Speech needs an interactive TCC authorization and
  whisper.cpp would be a new third-party dependency plus a model download -
  neither is added without your approval.
- Camera capture UI, local-notification reminders, OS background work and
  desktop handoff: platform plugins / device work, unverifiable here; goals
  stay foreground-driven, sync stays disabled (ACC-057).
- MediaPipe Decision Maker: no Rust runtime exists; the router stays
  recommend-only.
