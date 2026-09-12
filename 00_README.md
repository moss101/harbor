# Harbor corrected implementation authority

Revision 3, 12 September 2026. This dossier corrects release-scope, milestone, schema, traceability and mobile-layout inconsistencies. Product qualification still requires implementation and build-bound evidence.

## Read first

1. `Harbor_Implementation_Authority_Reconciled.docx` for product and engineering authority.
2. `Harbor_UI_UX_Design_Authority_Reconciled.docx` for interaction, responsive layout and states.
3. Files 01, 02 and 03 for precise release, runtime and architecture contracts.
4. `28_Correction_Log.md` for Revision 3 changes and outstanding product evidence.

Contradictions between narrative and executable contracts fail validation. Permissive schema acceptance never waives a safety invariant. Authorities must be corrected together.

## Executable package

- 115 implementation tasks and 81 acceptance gates in files 04 and 05.
- 47 routes with real task/state/platform/language/recovery mappings in file 07.
- 47 security scenarios with test IDs and gate mappings in file 09.
- Eight schemas, including approval receipts, commit journals and release descriptors.
- File 25 registers 14 capabilities, dependencies and activation gates.
- File 26 and `fixtures/qualification/` define measurement procedures and source fixtures.
- File 27 defines external-file save guarantees and safe-copy fallback.
- `tools/validate_dossier.py`, `tools/contracts.py` and `tools/test_contracts.py` provide reproducible checks. Setup and evidence formats are in `tools/README.md`.

Files 18, 19, 20 and 24 are generated projections, validation reports and the package manifest. Files 06 and 14 define tokens and language scope; files 08 and 10 describe repository boundaries and release checks. Files 11–17 define sync, Office/device, privacy/storage, evaluation and providers. Files 21/22 govern Office/formula qualification; file 23 tracks reconciliation.

## Release rule

Validate target and feature configuration before selecting gates. Every applicable blocking gate requires PASS evidence for its exact descriptor, commit, build, platform/architecture and qualification identity. Disabled, platform-inapplicable and later-milestone gates have explicit N/A states. Core disabled-path tests still run. Optional services require their own activation evidence.

Run `python3 tools/validate_dossier.py` after installing `requirements-validation.txt`. Structural PASS certifies checked specification properties only. All 71 formula targets remain unqualified. Missing production model/engine/device/evaluation bindings or approved performance thresholds block release. Historical reviews under `reviews/` are not current authority.
