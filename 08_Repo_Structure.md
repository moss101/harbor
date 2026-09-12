# Harbor reference repository structure

```text
harbor/
├── apps/harbor_app/                  # Flutter iOS/Android/macOS/Windows shell
├── packages/harbor_ui/               # Tokens/components/adaptive shell
├── packages/harbor_domain/           # Dart presentation/domain types only
├── packages/harbor_native/           # FFI + federated native adapters
├── core/harbor_core/                 # Orchestration facade
├── core/harbor_agent/                # Run state, executor lease, approvals, effects
├── core/harbor_inference/            # Provider capabilities/router/llama.cpp
├── core/harbor_modelhub/             # Catalog/HF/download/package/Fit Score
├── core/harbor_net/                  # Egress broker + network audit events
├── core/harbor_artifacts/            # OOXML/PDF IR, diff, safe commit
├── core/harbor_formula/              # Pinned spreadsheet engine adapter + coverage
├── core/harbor_render/               # Cross-platform Harbor Office preview renderer
├── core/harbor_knowledge/            # Index identity/chunk/embed/search/eval handles
├── core/harbor_store/                # Encrypted DB + private blob/key lifecycle
├── core/harbor_security/             # Capabilities/crypto/policy/receipts
├── core/harbor_sync/                 # Optional E2EE sync protocol
├── native/apple/                     # Secure store, file bookmarks, system model adapters
├── native/android/                   # Keystore, SAF, system model/thermal adapters
├── native/windows/                   # Credential store, file replace, system model adapters
├── third_party/llama.cpp/            # pinned source + patch manifest
├── schemas/                          # machine-readable contracts
├── fixtures/office|models|security|rtl|rag|effects/
├── evals/en|ar|mixed/                 # pinned evaluation datasets
├── tests/contract|integration|fault|performance|security|release/
├── evidence/                         # CI/release output; not hand-authored PASS claims
├── docs/decisions/                    # dated architecture/release decision records
└── .github/workflows/                # untrusted PR CI separate from signing/promotion
```

## Dependency rules
Flutter/UI never owns policy. Native adapters never own product policy. Network calls require `harbor_net`. Protected effects require `harbor_agent`. Artifact writes require `harbor_artifacts` commit service. Private blobs require `harbor_store`. Optional sync cannot import OS capability handles or approval receipts. Cross-crate dependency checks are CI-enforced.


## Dossier tooling
This specification package additionally includes tools/contracts.py, tools/validate_dossier.py, tools/test_contracts.py, requirements-validation.txt, 25_Feature_Registry.json, 26_Qualification_Profiles.json and 27_Artifact_Commit_Qualification.md. These are contract/qualification tools and fixtures; they are not Harbor's application implementation. The application repository layout above is the intended implementation layout.
