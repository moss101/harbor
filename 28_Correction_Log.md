# Harbor dossier correction log

Revision 3, 12 September 2026, addresses the twelve review findings plus cancellation, plaintext, sync and contrast observations.

## Corrected authority

The package contains 115 tasks, 81 acceptance gates, 47 routes, 47 security scenarios, eight schemas and 14 registered capabilities. Both Word authorities summarize the same contracts. Diagrams with overlapping labels or superseded flows were removed from the Word authorities; their original files remain in `assets/` for design history. All concept figures are illustrative; the current registry, routes and contracts govern implementation.

- Core approvals and egress are unconditional. Optional capabilities have activation gates, dependencies, platform restrictions and earliest milestones. Core tests cover disabled authority paths.
- Definition and implementation milestones are separated. M1 proves one declared local reference target; M2 repeats the workflow across supported platform/architecture targets.
- Gate results bind a release descriptor, build/commit, target, features and hashed test reports. Evidence existence, hashes, identity and security results are checked.
- Runtime/effect/artifact/model envelopes move to v3, and release results to v2. New v1 schemas cover approval receipts, release descriptors and commit journals. Legacy envelopes require explicit migration.
- Canonical arguments, single-effect receipts, lease generations, typed events/operations and semantic validation replace permissive placeholders. Product transactions must still enforce ownership and revocation.
- Security scenarios have test IDs, applicability and gate backlinks. Screen maps identify actual implementation tasks and originating recovery routes.
- The 640 px canvas minimum applies only at viewport widths >=1024. Compact controls fit the viewport at 200% text scale. Allowed muted-text/surface pairs are defined and checked.
- External files default to Save New Copy. Overwrite stays disabled until the exact adapter passes concurrent-writer and crash-boundary qualification.
- Qualification profiles define workloads, repetitions, numerical/date/error semantics, fidelity tolerances, language strata and baseline quality thresholds. Supplied source cases are starting fixtures, not a complete production corpus.
- Sync transfer requires source-stop acknowledgement, with explicit device expiry and tombstone retention. Plaintext files have operation windows and cleanup rules; crypto-erasure claims exclude already decrypted/exported copies.

## Addendum — 18 September 2026 (decision 0006: skill graphs, tool layer, eval harness)

- `03_Architecture_Contracts.md` §6 (Harbor Skill v1) is extended, not replaced: a `harbor.skill/v2` manifest carries an executable graph (`schemas/graph.schema.json`, `harbor.graph/v1`). Control flow is data; the model only fills schema-constrained slots inside `model.*` nodes; the tool allowlist of a graph is exactly the set of tools its `tool.call` nodes name (SEC-005 becomes structural); cycles exist only through edges that declare an iteration bound. Prose (`v1`) manifests remain valid declarations and are shown as such.
- `schemas/run_event.schema.json` gains explicit `run.step_started` / `run.step_completed` branches with optional `node_id`, `input_hash`, `output_hash` and `tool` fields, and the generic display branch excludes those two types. Existing step events (`step_id` + `description`/`summary`) still validate; the change is additive. Rust `harbor_agent` payloads carry the same optional fields.
- §3 (tool contract) is now implemented as code: `harbor_core::tools` registers tools with JSON Schema, risk class, requirements, timeout and output limit; arguments are validated and canonicalized below the model; calls outside the run allowlist are refused before dispatch. The closed catalog grows by `artifact.placeholders`, `artifact.fill_placeholders`, `formula.audit`, `formula.build_operations`, `text.verify_fields` and `text.detect_language` — all `read` or `propose` class; no tool commits anything.
- Approval nodes prepare the effect durably (`run.effect_prepared` with the canonical args hash, `run.approval_requested`) and park the run in `WAITING_APPROVAL`; the proposal carries the base content hash and the proposed output hash the receipt binds. The protected commit itself remains a host effect executed after the decision (unchanged from §5).
- Skill evaluation is a real harness (`harbor.skill_eval/v1`): typed assertions, a replay tier that runs with no model weights (CI), a live tier bound to model/runtime identity (qualification machine) and a record mode. The prose `eval_cases` of v1 manifests are documentation, not tests.
- Nine schemas now exist (graph added). Package manifest and structural validation regenerated.

## Verification and remaining work

Run `python3 tools/validate_dossier.py` after installing `requirements-validation.txt`. The check is read-only. `--write` regenerates files 19, 20 and 24 only after checks pass. See `tools/README.md` for evidence evaluation.

This dossier supplies specification validation, not application code, measured benchmarks, deployed cryptography or complete device/Office/evaluation corpora. All 71 formula targets remain unqualified. Missing production identities and approved performance thresholds remain release blockers. Structural PASS never implies product PASS.

The earlier review under `reviews/` is a historical snapshot excluded from the current authority manifest.
