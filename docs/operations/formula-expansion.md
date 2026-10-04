# Formula expansion in batches (Excel as the oracle)

How Harbor grows its qualified spreadsheet function set beyond the original
99 targets. Authority: `22_Formula_Coverage.json` (target groups, fixture
dimensions), `core/harbor_formula/fixtures/formula_corpus.json` (the corpus).

## Principle

Expected values come from **real Microsoft Excel**, never from the engine.
A function is added to the corpus only when *every one* of its cases gives the
same answer in Excel and in the pinned engine ("probed-then-fixtured"). A
function where the engine deviates is not hidden: it is listed in its group's
`deferred_engine_deviations` with the formula, Excel's answer and the engine's,
so it can be fixed in the engine and re-run. Nothing becomes `PASS` until the
five-platform rule is met (`qualification_status` stays `REQUIRED_UNQUALIFIED`).

## Workflow (about 25 functions per batch)

1. Write `tools/formula_batches/batchNN.py`: for each function, 2–5 cases
   (`C(formula, cells, dim=…, note=…)`) covering values, coercion, blanks and
   errors. Use the DSL in `tools/formula_batches/dsl.py`.
2. `python3 tools/formula_batch.py all NN` runs, in order:
   - `oracle` — builds one workbook (a sheet per case, the formula in the same
     cell the qualifier uses, `ALL1000`), opens it in Excel and reads each
     result with 15 significant digits. Needs Excel and an unlocked screen.
     Newer functions get the `_xlfn.` (or `_xlfn._xlws.`) file prefix
     automatically; a workbook Excel refuses is reported, never silently read.
   - `probe` — evaluates the same cases on the pinned engine
     (`core/harbor_formula/examples/probe_cases.rs`, same clock and cell as
     `qualify::run_case`).
   - `compare` — lists every deviation (numbers agree to 1e-9; a 1e-6 tolerance
     is recorded on the case where only iterative functions differ).
   - `build` — writes the agreeing functions into the corpus and the coverage
     file (idempotent; it refuses to run if Excel gave no answer).
3. `cargo run -p harbor_formula --example qualify_dump -- out.json` must report
   zero failed cases; then `python3 tools/pin_engine.py --check` and
   `python3 tools/validate_dossier.py --write`.

## Known limits of the oracle

- Excel loads file formulas without dynamic-array semantics (`TRANSPOSE` in a
  normal cell gives `#VALUE!`), so array-valued functions are tested through
  `INDEX`/`ROWS`/`COLUMNS`/`SUM`; **spill behaviour is not claimed** (it stays
  preserve-only per `22_Formula_Coverage.json`).
- Locale-dependent output (`DOLLAR`, `ASC`) follows the oracle machine's regional
  settings (here Saudi Arabia: `SAR1,234.57`) and is deferred, not forced.
- Context-dependent functions (`SHEET`, `SHEETS`) and volatile ones
  (`RAND`, `RANDBETWEEN`, `RANDARRAY`) are out of scope of this corpus.
- Excel's own floating-point noise is not a target (`IMPOWER("2+3i",3)` prints
  `-46+9.00000000000001i`; the engine's `-46+9i` is the better answer).

## After any corpus or engine change

The fixture bundle hash changes, so the platform-tier results bound to the old
hash (`qualification_results`) are stale: re-run `qualify_dump` on macOS, the
iOS Simulator and the Android emulator and record the new tiers
(`docs/operations/formula-tier-runbook.md`).
