# Harbor — Model Compatibility (release candidate 1.0.0)

**What Harbor does not claim:** that it works with every Hugging Face
model. It does not, and it will not say so anywhere in the store listing.

**What is true:**

1. **Signed catalog packages.** The signed Harbor catalog (epoch-verified,
   hash-pinned per file) declares supported packages. At RC these are:
   - `qwen2.5-1.5b-instruct` (GGUF, Q4_K_M) — chat/instruction, ~1.0 GB
   - `bge-small-en-v1.5` — embedding model for grounded retrieval (RAG)
   - `stories260k` — tiny evaluation/demo model
2. **Arbitrary GGUF acquisition with honest scoring.** Any GGUF on
   Hugging Face can be inspected and acquired through the Egress Broker,
   but Harbor computes a **Fit Score** (architecture, size, quantization,
   context, and device memory) before download and warns when a model is
   unlikely to run well. Acquisition ≠ qualification: an off-catalog
   model carries no support claim.
3. **Runtime.** On-device inference uses llama.cpp (pinned snapshot) with
   Metal acceleration on Apple silicon. Vulkan/CUDA routes are unqualified
   in this release and are not claimed anywhere.
4. **Determinism.** Generation is greedy-deterministic for reproducible
   evaluation; evaluation results are bound to the model hash, runtime
   revision, corpus hash and build identity recorded in the release
   evidence.

**License presentation:** model packages carry upstream licenses
(see `model_licenses.md`); the license is displayed in the catalog before
acquisition.
