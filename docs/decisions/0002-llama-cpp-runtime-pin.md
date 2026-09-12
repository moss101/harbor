# Decision 0002 — llama.cpp runtime pin

Date: 2026-09-12
Status: Accepted.

## Problem
`17_Model_Provider_and_Package_Contract.md` and goal §4 require a pinned llama.cpp runtime
for the GGUF provider. The revision and its integrity must be recorded and reproducible.

## Decision
- `harbor_inference` gains an optional `gguf-backend` feature depending on
  `llama-cpp-2 = "=0.1.156"`. The llama.cpp source snapshot is vendored inside the
  published `llama-cpp-sys-2` crate (see its Cargo.toml include list), so the crate
  pin fixes the exact runtime revision: `llama.cpp/llama-cpp-sys-2@0.1.156`
  (reported by `harbor_inference::gguf::runtime_revision()`).
- The provider implements the `ModelProvider` contract over installed packages from
  `harbor_modelhub` (weights path from the package manifest, hashed at install time).
- Prompt assembly is a minimal documented template with greedy decoding at temperature 0
  for determinism; model-native chat templates are a follow-up keyed to GGUF metadata.
- Backend init is process-global (llama.cpp constraint), shared via a OnceLock.

## Evidence
- Real-model test `core/harbor_inference/tests/gguf_provider.rs` installs the tiny
  stories260K model (GGUF v3, sha256 recorded in 26_Qualification_Profiles.json
  `test_fixtures`) through the staged-install path and generates text on-device
  (Metal) with token accounting. Run with:
  `cargo test -p harbor_inference --features gguf-backend`

## Exclusions
Pre-1900 dates are a formula-engine concern (decision 0001); this decision pins the
inference runtime only. Mobile/Windows cross-builds of the pinned snapshot are M2 scope.
