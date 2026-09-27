# Decision 0008 — Authoring skills, new-file creation and the repair loop

Date: 2026-09-26
Status: Accepted (implemented; seven skills added or made runnable; replay tier is a CI gate; live tier measured three times).

## Problem
After decision 0006 Harbor could review and repair documents but not make one: every
propose tool edited an attached file, and the document families users ask for first — a
spreadsheet, a deck, a proposal or letter, an email — were prose declarations
(`spreadsheet-analyst`, `presentation-builder`, `email-drafting`) or absent. Separately,
the live tier on the pinned 1.5B model had stalled at 13/18 with every failure "a model
fact contained by a deterministic node": the verifier caught the mistake and the run ended
with it. Nothing told the model what it had got wrong.

## Decision
1. **New files are creation batches over the empty base.** 02 §"Artifact batches and
   safe save" already says a new artifact starts from "an immutable empty base version
   with the SHA-256 of empty bytes". A creation batch binds that hash; every precondition
   names a target that does not exist yet and expects the same hash; the operations are
   the schema's own kinds (`sheet.insert` + `cell.set`, `slide.insert` + `slide.update`,
   `metadata.set`) plus one additive kind, `block.insert`, for document paragraphs (see
   the correction-log addendum). Rendering is deterministic (no clock, no generated ids,
   fixed zip timestamps), so `decide_and_commit` re-derives the approved output from the
   batch alone, needs no bytes from the host, and allows only Save New Copy — there is no
   original to overwrite. Every rendered package passes
   `harbor_artifacts::package_integrity` (well-formed parts, every relationship target
   present, every part typed) before it is proposed.
2. **The model picks parameters; code writes the file.** `workbook.build` writes every
   formula (totals, computed columns, `IFERROR` around division) and recalculates through
   the pinned engine; `deck.build` lays out the cover and content slides and copies cited
   sources into the speaker notes; `docx.build` decides headed vs. letter layout and takes
   the address block, salutation and sign-off from the inputs; `email.render` formats the
   final text. The model's output is a small typed spec. Where the model's structure is
   wrong in a way code can repair without inventing anything, code repairs it and says so
   (`normalize_table_spec`, `normalize_outline`, `strip_letter_furniture`): a column both
   typed and computed is built once, by formula; a repeated header row and "Company1"
   example rows are removed; a repeated bullet or a slide that only repeats others is
   removed; a salutation or "Yours sincerely, [Your Name]" the model wrote into a letter
   body is removed because Harbor prints its own. These only ever remove, never touch a
   figure, and every repair is returned with the proposal.
3. **The model points; code copies.** Report to Slides numbers the report's statements
   (`document.sentences`, page by page) and a bullet cites a sentence number; the
   verifier checks every figure in the bullet is in that sentence, and `deck.build`
   copies the sentence and its page into the notes. The first design asked the model for
   a page and a verbatim quote; the pinned model quoted page headings, repeated one slide
   five times and was cut off mid-word (live run 1). A number is a decision it can make;
   a quote is a copy it cannot.
4. **Verify, then repair once — with what the model can use.** Each drafting node is
   followed by a deterministic verifier returning `{ok, problems, warnings}`. A problem
   is a sentence the model can act on ("Row 3 (Gym): 45 under "Amount" does not appear in
   the description…", "6.4m is not in sentence [1]; cite the sentence that states it"),
   and it is blocking: invented figures, unverifiable citations, template text, a missing
   section, the wrong slide count. Warnings (repetition, length) are shown with the result
   and never withhold it. A failed check follows a bounded back-edge
   (`max_iterations: 1`) to the drafting node, whose context gains `optional` items that
   are omitted while empty, so the first attempt reads exactly as before. A second
   failure takes the edge's `exhausted` continuation: `needs_input` with the problems
   listed, or — where the verifier's nulling is the safe answer (meeting notes, thread
   summaries) — completion with the unverifiable fields nulled.
5. **Budgets are proven, not hoped.** `Graph::worst_case` walks every path — bounded
   edges to their limits with the executor's per-`from->to` counters, maps at
   `max_items`, structured retries — and validation refuses a graph whose declared
   `max_steps`/`max_tool_calls` cannot cover its worst run, so a repair loop can never
   turn into a "budget exceeded" failure. Context tokens stay a runtime check.
6. **Seven skills.** New: Spreadsheet Builder (`sheet-builder`), Table Cleanup
   (`table-cleanup`: no model call without an instruction; formula cells untouched; no
   row ever deleted), Report to Slides (`report-to-slides`), Document Drafter
   (`document-drafter`: proposal, report, letter), Thread Summary (`thread-summary`).
   Made runnable: Presentation Builder (notes → deck) and Email Drafting (draft or reply;
   the result is text to copy — Harbor has no mail connector and sends nothing). Runnable
   graphs: 9 → 16 of 35 built-ins.
7. **Repair loops on existing skills** where a live failure was substance a model could
   fix if told: team-update (invented figures), meeting-notes (now one
   `text.verify_items` call, which also nulls an owner who is not named next to the action
   they supposedly own) and formula-audit (`formula.build_operations` now reports
   rejected and unaddressed decisions as problems).

## What the live tier taught (three runs on Qwen2.5-1.5B-Instruct Q4_K_M)
- **Run 1 (retry saw its previous answer): 20/31.** In every retry the model copied its
  previous answer byte for byte and ignored the listed problems — meeting notes, formula
  triage, table spec, deck outline, letter.
- **Run 2 (retry saw only the problems): 21/31.** Regeneration fixed the table specs
  (sheet-builder 0/2 → 2/2 once structural slips were repaired in code) but made field
  repairs worse: the meeting-notes retry dropped a correct action. A stricter repetition
  check also withheld a usable proposal.
- **Run 3: 25/31.** Field-repair skills (meeting notes, thread summary) see their previous
  answer again with "change only what the problems name"; generation skills see only the
  problems; repetition and length are warnings; duplicates are removed in code; Report to
  Slides cites sentence numbers.

The general lesson is decision 0006's, one level up: a retry is only as good as the
decision it asks for. When the problem is a field, show the answer and ask for the field;
when it is the structure, repair the structure in code; when it is a copy, stop asking the
model to copy.

## What the harness measured (honest numbers)
- **Replay tier:** 54/54 cases across 16 skills in ~2 s with no weights (was 23/23 across
  9). Every repair loop is proven to run exactly once and to stop: the guard cases feed a
  bad first answer and assert `node_runs` 2 and the repaired or `needs_input` outcome;
  three of them replay the pinned model's actual live outputs.
- **Live tier, before:** 13/18, re-measured at `eec218b` this session (same five
  failures as sessions 35 and 40).
- **Live tier, after:** **25/31** on the working tree over `eec218b`
  (`evidence/skill_evals/live-6a1a2eb6d156.json`, 3 runs recorded under
  `evals/skills/*/cassettes/live/`):
  - the original 18 cases: **13/18, unchanged** — the same five fail. The repair loops
    run (formula-audit and meeting-notes both retry) but the pinned model repeats the
    unchanged "fix" formulas and re-normalises "Friday 26 September" to a date; the
    misattributed runbook owner is now nulled by the attribution check, so one of the two
    failing meeting-notes assertions passes. doc-coauthoring and second-look have no loop
    and did not move.
  - the 13 new live cases: **12/13** — sheet-builder 2/2 (but see the 27 Sep correction below), table-cleanup 3/3,
    report-to-slides 1/1, document-drafter 2/2, email-drafting 2/2, thread-summary 1/1,
    presentation-builder 1/2. The failure is an exact slide count: asked for three
    slides from short notes, the model repeats a slide; once the repeat is removed two
    remain, and the run stops rather than pad.

## Found along the way (by the new tests)
- **PDF page mapping was wrong for every multi-page PDF.** `extract_pages` split on form
  feeds pdf-extract 0.12 does not emit, so all pages came back as one "page 2", and
  `artifact.read` then added one to an already 1-based index. Page citations in PDF
  Research would have been wrong. Pages are now extracted one at a time.
- **The deck writer produced packages PowerPoint would offer to repair:** every notes page
  linked to slide 1, the notes master had no content type and no theme, the chart link was
  the literal `chart{n}.xml`, and a title with `&` read back escaped. Rewritten to the full
  minimal package (master text styles, title and content layouts, a notes master with its
  own theme, presentation properties, 16:9, system fonts) under the integrity check.
- **The workbook reader turned a numeric string stored as text into a number,** so "15"
  stored as text was invisible to cleanup; the snapshot now carries `stored_as_text`.
- **The compatibility classifier flagged `presProps`/`viewProps`/`tableStyles`** — parts
  every PowerPoint-saved deck carries — as unknown, putting the banner on real decks.

## Consequences
- A skill that makes a file is data again: a graph with one model node, one verifier, one
  build tool and an approval, plus eval cases.
- The Run sheet saves a created file under the tool's suggested name through the same
  commit path (no attachment, no Overwrite), renders enum inputs as choices and integer
  inputs as digits-only fields, and copies text results to the clipboard.
- Table Cleanup's graph declares a model node, so the host binds a model even for the
  no-instruction path that never calls it (the planner requires a model for any graph
  with model nodes). Kept conservative on purpose.
- Fine-tuning stays deferred. If it is ever done, the recorded live cassettes of passing
  runs are the training data and the replay tier is how it would be measured; the runs
  above say the higher-value lever for this model class is more decisions moved into code.

## Checked in Office
The five files `created_samples.rs` writes from the replay cassettes (a budget workbook,
the report and notes decks, a proposal and a letter) were opened in Microsoft Excel,
PowerPoint and Word 16.113.2 on macOS. None raised a repair prompt and none was marked
modified on open, so Excel's recalculation agrees with the cached values Harbor wrote;
formulas, the currency format and the frozen header row read back as written, each notes
page belongs to its own slide, and the documents use Word's own Title and Heading 1
styles. The check found one defect, since fixed: a cover without a subtitle kept an empty
subtitle placeholder, which PowerPoint shows as "Click to add subtitle"; the shape is now
omitted. Reproduce from `core/` with
`HARBOR_WRITE_SAMPLES=<dir> cargo test -p harbor_core --test created_samples -- --ignored`.

## Not done (explicitly)
- Formatting beyond what the IR carries: fills, fonts other than bold header and total
  rows, per-deck themes, images in created decks, document headers and footers.
- `workbook-to-deck` (a deck whose charts come from verified workbook values): the chart
  element exists in `slide.update`; the graph is the next step.
- Editing an existing deck through batches (Deck Review still returns fixes).

## Correction and follow-up (27 September 2026)
Running the skills on the iPhone simulator (STATUS, session 42) showed that
the budget sheet-builder live pass above proposed a Spent of 60 for Internet,
a figure the description gives only as planned; the replayed contract case
asserted it as correct. `workbook.verify_spec` now refuses a figure the
description gives once in two columns of a row, and `repair_duplicated_figures`
empties the copy when the description names the right column before the
figure. The same run found four more defects (number format, "null" words,
the email greeting the sender, save-name collisions on mobile), all fixed
there.
