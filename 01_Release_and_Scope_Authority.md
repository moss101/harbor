# Harbor release and scope authority

Revision 3 is the corrected specification authority. Milestones describe completed deliverables; priority never determines release applicability. The order is M0_CONTRACT, M1_LOCAL_REFERENCE, M2_PLATFORM_BETA, M3_GA_CORE, M4_OPTIONAL_SERVICES.

## Authority and source files
The normative machine-readable inputs are the eight schemas, 25_Feature_Registry.json, 04_Implementation_Backlog.csv, 05_Acceptance_Matrix.csv, 07_Screen_Inventory.csv and 09_Security_Test_Matrix.csv. Narrative contracts define semantic checks beyond JSON Schema. A contradiction is a package validation failure; neither prose nor a permissive schema silently overrides a safety invariant. File 18 and task backlinks are generated projections.

## Release selection
1. Validate harbor.release/v1 and bind the descriptor to the exact gate-catalog and feature-registry SHA-256. Platform and architecture must be a supported pair. The target includes OS version, device class and qualification profile.
2. Reject unknown/duplicate flags, missing feature dependencies, unsupported feature/platform pairs and enabled features earlier than their earliest milestone. Core is implicit and cannot be disabled. M3 and later require public Hub acquisition and RAG. Other features, including Arabic OCR, are explicitly selected.
3. For every gate, classify platform mismatch as N/A_PLATFORM, then a disabled feature as N/A_DISABLED, then a later milestone as N/A_MILESTONE. The remaining blocking gates are REQUIRED. All requires every platform separately; semicolons express platform unions. No other selector grammar is supported.
4. Every enabled feature must have at least one REQUIRED activation gate from its registry entry. A qualification candidate may exercise the feature only in an isolated test build. User-facing activation requires passing build-bound evidence for all its activation gates. Disabled paths are tested unconditionally by ACC-064.
5. Every REQUIRED gate needs exactly one harbor.gate_result/v2 result with matching descriptor digest, target, feature set, commit and build digest. PASS requires nonempty evidence objects whose referenced files exist and whose hashes match. Evidence must identify the executable tests and, where applicable, security scenarios, qualification profile and fixture identities. FAIL, BLOCKED, missing, stale or incompatible evidence blocks release. N/A cannot satisfy a REQUIRED gate.

## Core and optional scope
Core includes the adaptive shell, GGUF inference, public model acquisition, protected agents, local files/PDF, Office artifacts, local Knowledge/RAG, EN/AR UI, accessibility, egress observability, recovery and four-platform packaging. Optional M4 capabilities are authenticated/gated Hub access, connectors, remote inference, diagnostics upload, sync, platform system-model providers, desktop sandbox and app lock. Speech is not enabled by this registry; adding it requires an explicit contract and activation gate.

## Milestone and evidence rules
M0 freezes contracts and validates this dossier. M1 proves one local reference workflow using cached/local model installation with network disabled. M2 repeats it across supported platform/architecture targets and qualifies native behavior. M3 completes GA quality/performance and packaging. M4 qualifies optional services individually. A task may not depend on a later-milestone task; a gate may not require a later-milestone implementation. Split tasks if a contract and its implementation finish at different times.

Task completion is not release evidence. The package validator checks specification structure, negative fixtures and projections. Product gates remain unexecuted until real runtime evidence exists. Use `python3 tools/validate_dossier.py` to check the package and `python3 tools/validate_dossier.py --release descriptor.json --results results.json --evidence-root evidence` to evaluate a candidate. Build promotion must independently verify the supplied build digest against the signed artifact.
