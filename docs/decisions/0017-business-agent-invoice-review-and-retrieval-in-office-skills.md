# Decision 0017 — Supplier-invoice review, opt-in retrieval for Word drafting, memory correction

Date: 2026-10-10
Status: Accepted (implemented and tested; see `docs/verification_2026-10-10.md`).

Context: an independent verification pass found three capabilities the agreed
scope needed that did not exist: a Business-Agent workflow (invoices ->
overdue -> comparison -> Excel + summary), any runnable Office skill that used
retrieval, and memory correction/contradiction handling.

## 1. `supplier-invoice-review` (skill #36, runnable graph #17)

The model only READS invoices into records. Everything a payment decision
depends on is code (`harbor_core::tools::invoices`, tool `invoice.review`,
`Read` risk, no side effects):

- every invoice number, supplier, date, amount and paid status must occur in
  the source text (amounts on a line that mentions that invoice; boundary-aware
  so `INV-10` is not found in `INV-1001`, `4,200.00` not in `14,200.00`);
- "paid" must agree with what the source's own line says; if it says nothing the
  record must carry `paid: null` (reported as "status unknown", never overdue);
- overdue = unpaid and due date strictly before the caller-supplied `as_of`;
- amounts are summed in integer cents; mixed currencies are refused;
- previous records (CSV) are compared deterministically: new, amount changed,
  supplier changed, previously paid but now unpaid, missing from this batch;
- the workbook spec and the written summary are built from the same figures;
  `workbook.build` writes the totals as formulas and the engine recalculates.

An unsupported record goes back to the model once with the problems listed; a
second failure ends in `needs_input`. Saving the workbook is the existing
protected `artifact.commit` approval. The skill is not goal-eligible (two
required inputs), by the existing "sole text input" rule.

Prompt-injection stance: text inside an invoice that tells the reader to mark
things paid cannot change a result - the tool reads the status from the source
lines of each invoice, and the tool has no side effects (test
`an_injected_instruction_in_the_invoice_text_cannot_flip_a_status`). An
invoice line that itself says "paid" is indistinguishable from a real paid
invoice; the approval step and the shown source are the control there.

## 2. `knowledge.gather` + `document-drafter` `use_sources`

No runnable skill used retrieval before this (the five skills that declare
`knowledge.search` are prose-only). `knowledge.gather` is opt-in, never fails
a run (off / no index / search error => the brief unchanged, `available:false`
with a reason) and returns passages, citations and `source_text = brief +
passages`. `document.verify_draft` now checks figures against `source_text`:
a figure may come from the brief or from a passage that is cited with the
result, never from the model. With `use_sources` off the graph behaves exactly
as before (all four existing replay cases pass unchanged).

Limit: passages are not ranked against a calibrated floor at this layer, so an
irrelevant passage can make its figures "grounded". The user opts in and sees
the citations; a relevance floor per embedder is future work.

## 3. Memory correction and contradiction surfacing

`memory.correct {id, text}` stores the correction as a new record
(`supersedes`), marks the old one `superseded_by`, removes the old one from the
index (it can no longer be retrieved as a current fact) and keeps it listed for
audit until deleted. `memory.add` now returns `similar`: the three nearest
CURRENT memories with scores, so the person sees a possible restatement or
contradiction before moving on. Nothing is resolved automatically - a cosine
score cannot tell agreement from contradiction. Old sealed files load unchanged
(`serde(default)`).

## 4. Router calibration v2

The skill set changed (35 -> 36), so all five calibration entries were
re-measured (108 labeled requests; thresholds unchanged by the selection rule).
Both embedders route the invoice request to the new skill with confidence.

## Not done (recorded, not hidden)

- No skill-to-skill orchestrator: the Excel -> Word -> PowerPoint chain is
  three runs, each with its own approval, composed through the same verified
  figures (`invoices_to_excel_to_word_to_powerpoint_keep_one_set_of_figures`).
- No ASR/voice provider, no camera-capture workflow, no OS background tasks.
- Memory is not injected into skill prompts automatically.
