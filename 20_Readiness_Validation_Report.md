# Harbor dossier validation report

Specification validation: **PASS**. Validator `harbor-dossier-validator/1.0.0`.

115 tasks, 81 gates, 47 screens, 47 security scenarios, 9 schemas and 14 explicit features.

Checks cover ID and backlink integrity, milestone ordering, feature activation, security coverage, screen semantics, schema validity, negative contract fixtures, color pairs, document revision and package integrity.

Required core GA gates by platform for public Hub, RAG and Arabic OCR: iOS: 66, Android: 66, macOS: 67, Windows: 67. Counts describe gate definitions; each architecture/device qualification still needs evidence.

Contract regression cases: 124 passed, 0 failed.

Product readiness remains **BLOCKED**. No product gate is marked PASS by this report. Formula qualification remains 0 of 99; model, engine, adapter, fixture, evaluation and device bindings require implementation evidence. Performance thresholds must be approved and measured.

Input manifest SHA-256: `e392a6708aed94ba8f6c17f9ad59212a657ff9a7016034e2a9a9412d4c326115`. File 19 records every input digest. The package manifest excludes itself and includes this report.

Reproduce: install `requirements-validation.txt`, then run `python3 tools/validate_dossier.py`. Regenerate derived reports only after authority edits with `python3 tools/validate_dossier.py --write`.

Issues: None.
