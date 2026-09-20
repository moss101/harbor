# Decision 0007 — Security review of the 1.1 branch (production plan C4)

Date: 2026-09-20
Status: Accepted (findings fixed; none open at "high")

## Scope
`git diff harbor-v1.0.0-rc2..HEAD` (14 commits, 203 files): the JSON FFI
boundary (`harbor_core_call`, new `run.commit_proposal`, `diag.*`,
`catalog.list`, `model.fit_estimate`), the graph executor and its commit
path, the tool layer and the nine graphs, the diagnostics log/export, the
acquisition/staging path, the GGUF provider, the Flutter app, the Python
tools and both workflows. Method: `/security-review` (one identification
pass, one independent false-positive pass per candidate), fuzzing (decision
context in `docs/STATUS.md` session 37), and the plaintext-at-rest
inspection.

## What held
- Commit authority: a commit requires `WAITING_APPROVAL` in the durable
  log and a bound `artifact.commit` approval; base bytes must hash to the
  bound base, the re-applied batch to the bound output, before any durable
  event; receipts expire; a rejected decision cannot be committed later;
  Save New Copy uses exclusive create; Overwrite revalidates inside the
  write window and treats a third version as a conflict.
- Tool allowlist below the model (SEC-005): the allowlist is the embedded
  graph's tool set; model output only lands at the node's `out` pointer.
- Path handling: `fs.read_workspace_file` containment; listing paths that
  escape staging are refused; pinned sha256 enforced.
- Catalog bootstrap: first import needs the caller-pinned root; a later
  root is never substituted; signature verified before acceptance.
- Diagnostics crypto: domain-separated key, AEAD frames, no overwrite.
- No new network code; workflows only interpolate maintainer inputs.

## Findings and fixes
1. **Model-authored formulas had no content gate** (candidate severity
   Medium, false-positive pass 6/10 — below the report threshold, fixed
   anyway because it is the deterministic gate the architecture promises).
   `formula.build_operations` accepted any formula a model spelled (the
   `formula-audit` triage node) as long as it differed from the original,
   and `apply_xlsx` wrote it verbatim, so a hostile workbook carrying a
   prompt injection in a formula literal could make Harbor write
   `=WEBSERVICE(...)` or a UNC external reference into the file the user
   then approves and saves. Fix: `check_formula_allowed` — only functions
   in the qualified set, no `[Book]` / UNC / URL / DDE syntax, sheet
   references must exist in the workbook, string literals stripped first —
   applied in `formula.build_operations` (model decisions) and again in
   `apply_xlsx` (every batch, last gate below the model). Tests:
   `formula_content_gate_refuses_unqualified_functions_and_external_references`,
   `build_operations_refuses_model_formulas_outside_the_gate`.
2. **Redaction gap for paths with spaces** (candidate Low, false-positive
   pass: not a vulnerability, a precision defect — fixed). `redact()`
   stopped at the first space, so a document filename fragment could reach
   the diagnostics export although its manifest says "file paths" are
   never included. Fix: the filename part is matched lazily up to an
   extension (`open /Users/amina/Documents/Q3 plan.docx failed` →
   `open <path:.docx> failed`); the plaintext-at-rest inspection now seeds
   a path with spaces and asserts neither fragment survives.

## Evidence
`evidence/security/review-1.1.json` (machine-local, commit-bound) records
the scope, the two findings with their verdicts and the fixing commit.
