# Harbor Architecture Contracts — Reconciled Build Authority

This file is normative. Contradictions with machine-readable or detailed safety contracts fail dossier validation; they are not silently resolved by precedence.

## 1. ModelProvider v3
Providers operate on explicit `ModelRef` variants (`InstalledPackage`, `SystemManaged`, `RemoteEndpoint`) and declare capabilities (`chat`, `tools`, `structured_output`, `embeddings`, `vision`, `audio`). Load/generate/embed are cancellable and own cleanup. Unsupported capability returns a typed error; router substitution requires explicit policy and visible UI. See `17_Model_Provider_and_Package_Contract.md`.

## 2. Privacy and egress
Workspace PrivacyMode is `LOCAL_ONLY | HYBRID | REMOTE_ALLOWED`. Acquisition/auth/download, remote inference, connectors, sync, diagnostics and OS-managed provisioning have separate egress policies. Local Only prohibits user/workspace-content egress, not an explicit model-acquisition session. All app-controlled traffic crosses the Egress Broker. See `13_Network_and_Storage_Policy.md`.

## 3. Tool contract
Tools expose JSON schema, risk class, required capabilities, timeout and output limits. Arguments are validated/canonicalized below the model. The model cannot mint capabilities or approvals.

## 4. Durable run v3
States: `CREATED, PLANNING, RUNNING, WAITING_APPROVAL, PAUSED, CANCELLING, COMPLETED, FAILED, CANCELLED`. Run events are append-only and replayed. Pause reasons/budgets persist. Exactly one generation-fenced executor lease owns a run. Unknown state/authority events halt replay; only envelope-marked `ignorable_display` events may be skipped. See `02_Runtime_Effect_and_Artifact_Contracts.md` and `schemas/run_event.schema.json`.

## 5. Protected external effects v3
Effect intent is durable before dispatch. States: `prepared, dispatched, committed, outcome_unknown, aborted`. Approval binds canonical args, target and policy version. Recovery uses provider idempotency or reconciliation; without either, dispatched-without-commit becomes `outcome_unknown` and is never blindly retried. See `schemas/effect_intent.schema.json`.

## 6. Harbor Skill v1
Skills contain instructions, declarative capability/tool allowlists, model policy and budgets. No arbitrary executable code is bundled by default. A skill cannot widen workspace privacy or OS capabilities.

## 7. Model package v3
Model packages may contain multiple verified data files. Install is staged and atomic. Arbitrary repository code is forbidden. Catalog signatures use monotonic epochs and key rotation. See `schemas/model_package.schema.json`.

## 8. Artifact mutation v3
Every edit is an operation batch bound to `artifact_id + base_version_id + base_content_hash`, with unique op IDs and per-op preconditions. Approval binds the proposed output hash. Revalidate immediately before safe commit; stale base produces conflict. Batches are all-or-nothing. See `schemas/artifact_batch.schema.json`.

## 9. Office calculation and rendering
Harbor owns a bounded Office feature matrix. Spreadsheet verified calculation uses the pinned `harbor_formula` adapter; cached values alone are never trusted for verified numerical conclusions. Unsupported content is preserved/reported or rejected, not silently dropped.

## 10. Storage
SQLite plus encrypted private blob store. Public weights are separate. Private artifacts/extracts/embeddings/previews/temp/backups have explicit keys, rotation and deletion rules.

## 11. RAG index identity
Index reuse requires exact matching identity across embedding model/hash, dimension, chunker, tokenizer, preprocessing and language policy. Source removal excludes future retrieval immediately.

## 12. Sync
Sync is optional and separately gated. It never transfers OS capabilities, executor leases or allow-once approval receipts.

## 13. Release authority
Release applicability is determined by `05_Acceptance_Matrix.csv`, not by task priority alone. PASS requires evidence tied to the build/commit. Disabled optional features are N/A_DISABLED.


## 14. Corrected executable authority
The release registry, schemas and semantic validators jointly define enforceable contracts. See 01 for activation and build evidence, 02 for cancellation/approvals/commit recovery, 25 for feature closure, 26 for qualification and 27 for storage-provider safe-save modes. External overwrite defaults to disabled until its adapter qualifies. Compact editing has viewport-sized controls; the 640-pixel minimum is desktop-only. Structural validation does not mark a product gate PASS.
