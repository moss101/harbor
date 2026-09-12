# Harbor Release Qualification Checklist — Reconciled

A release is qualified by the machine-readable applicability rules in `05_Acceptance_Matrix.csv`. **Do not use “all P0 tasks pass” as a release rule.** Tasks implement capabilities; acceptance gates plus evidence qualify them.

## M0 — Contract freeze
- [ ] Gate evaluator produces deterministic required-gate set for target platform/features.
- [ ] Run/effect/artifact/network/storage/provider contracts are schema-versioned and negative/semantic fixtures pass.
- [ ] Office support matrix, device support manifest and language matrix are approved.

## M1 — Local reference workflow
- [ ] One declared reference device completes local model install → workbook verified recalculation → analysis → deck creation → diff → approval → safe save.
- [ ] Kill/restart/replay succeeds at every run transition.
- [ ] Effect crash-window and artifact stale-write fault injection pass.
- [ ] Unsupported/stale formulas cannot be presented as verified results.

## M2 — Four-platform beta
- [ ] iOS, Android, macOS and Windows build/package smoke suites pass on declared support floor.
- [ ] Independent traffic capture matches broker/network log.
- [ ] Private blob/temp/journal plaintext-leak inspection passes.
- [ ] Office compatibility corpus and adaptive Work Canvas breakpoints pass.
- [ ] EN/AR UI, RTL, screen-reader/keyboard and semantic color contrast pass.
- [ ] Model package interruption/catalog rotation/provider-absence tests pass.

## M3 — Core GA
- [ ] Every applicable blocking gate through M3 is PASS with build-bound evidence.
- [ ] Performance thresholds are populated from lowest qualified devices; no TBD remains.
- [ ] RAG index migration/source revocation and EN/AR grounded-answer gates pass.
- [ ] Store/notarization/privacy declarations match observed application behavior.
- [ ] SBOM, migration N-2, rollback, last-known-good catalog and recovery procedures are attached.

## M4 — Optional services
Enable each feature independently only after its blocking gates pass: authenticated/gated Hub access, remote inference/endpoints, connectors, diagnostics upload, E2EE sync. Disabled features are N/A_DISABLED and must not leave dormant authority paths enabled.


## Corrected package checks
- [ ] Run tools/validate_dossier.py and the bundled contract/regression suite.
- [ ] Check target-specific gate lists, feature dependencies and disabled-path coverage.
- [ ] Verify all security scenarios and originating screen/recovery states have real mappings.
- [ ] Verify the eight versioned schemas plus approval, lease, canonicalization and commit semantic checks.
- [ ] Bind qualification fixtures, model/engine/runtime/device identities and approved thresholds; unresolved bindings remain BLOCKED.
- [ ] Verify external overwrite adapters or keep Save New Copy; exercise every durable crash boundary.
- [ ] Rebuild Word authorities after contract changes, render and inspect them, then regenerate the package manifest.
