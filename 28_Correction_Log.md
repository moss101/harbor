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

## Verification and remaining work

Run `python3 tools/validate_dossier.py` after installing `requirements-validation.txt`. The check is read-only. `--write` regenerates files 19, 20 and 24 only after checks pass. See `tools/README.md` for evidence evaluation.

This dossier supplies specification validation, not application code, measured benchmarks, deployed cryptography or complete device/Office/evaluation corpora. All 71 formula targets remain unqualified. Missing production identities and approved performance thresholds remain release blockers. Structural PASS never implies product PASS.

The earlier review under `reviews/` is a historical snapshot excluded from the current authority manifest.
