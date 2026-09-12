# Decision 0004 — Production model package selection (model_package_sha256)

Date: 2026-09-12
Status: Accepted for M1/M2 qualification; catalog-upgradable.

## Decision
Bind `model_package_sha256` to **Qwen2.5-1.5B-Instruct Q4_K_M GGUF**
(sha256 6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e,
1,117,320,736 bytes) from the official Qwen repository.

## Rationale
- License: Apache-2.0 (cleared for redistribution within Harbor packages).
- Source: official Qwen repository on Hugging Face (no third-party re-quant).
- Size/quant: Q4_K_M fits the qualified reference device comfortably
  (Fit Score: Excellent/Good band) and represents the product's
  "Balanced" tier shape.
- Capability: instruct-tuned with a native chat template (exercises the
  template path end-to-end).

## Qualification evidence (executed)
- Chat: correct deterministic answer ("The capital of France is Paris.")
  with greedy decoding via the model-native template; provenance shows
  executed_on + ON_DEVICE. Run:
  `cargo test -p harbor_inference --features gguf-backend --test qwen_qualification`
- Embeddings continue to qualify via bge-small-en-v1.5 (separate binding).

## Network constraint note
During this session the local network intermittently reset TLS handshakes
for the 1.1GB resolve URL, so the bytes were fetched directly (identical
URL, hash verified into the staged-install path); the BROKERED streaming
acquisition path itself is proven by the stories260K/bge e2e tests
(`acquire_real_model_through_broker_end_to_end`). Upgrade path: change the
model by updating the signed catalog entry (hash re-pinned inside the
signature) and re-running the qualification tests — no code changes.
