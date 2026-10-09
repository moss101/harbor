# Decision 0014 — Routing is independent of the generative LLM; calibration belongs to the embedder

Date: 2026-10-09
Status: Accepted (implemented and tested).

## Rule

One Harbor runtime, any qualified local LLM. The embedding layer (retrieval,
memory lookup, skill discovery, candidate ranking) and the generative layer
(whatever chat model the user installed) are separate providers with separate
loaded models and no shared configuration.

## What guarantees it

- **Separate providers.** `KnowledgeService` owns the embedding provider;
  `ChatHandle` owns the chat provider. Switching, installing or removing a
  chat model touches neither the index nor the router cache.
- **Interchangeable embedders, compatible indexes.** EmbeddingGemma 2 and
  BGE-M3 (and e5) are selected by package id; the index identity hashes model
  revision, dimension, instruction policy and chunker, so vectors from two
  embedders are never mixed — a different embedder rebuilds from the sealed
  texts (ACC-055 path). Nothing is retrained or reconfigured when the chat
  model changes.
- **Versioned, benchmark-calibrated bars per embedder**
  (`harbor_core/src/router_calibration.json`): bge-m3 v1 0.75/0.02,
  embeddinggemma-2 v1 0.70/0.02, multilingual-e5-small v1 0.80/0.02, each with
  its corpus size, coverage, precision and evidence file (106 labeled requests,
  EN/AR/FR). An embedder with **no entry is uncalibrated**: the router still
  ranks, but abstains with `uncalibrated` instead of scoring against another
  model's bars (bge-small-en, which routes 32% overall and 6% of Arabic, is
  deliberately left out). Re-measure and bump `version` when the model,
  quantization, instruction policy or skill set changes. Measured on Q8_0 /
  Metal only.
- **Ambiguity goes to the user's selected chat model, through the provider
  contract** (`router::disambiguate`, `skills.suggest {chat_package}`), only
  when the embedder abstains as `ambiguous`, only if the provider verifies the
  model supports chat (constrained decoding is used when it also verifies
  structured output). The answer is validated against the offered ids; an
  off-list or malformed answer is discarded. The chosen skill only reorders a
  suggestion list. Live check: the Qwen2.5-1.5B fixture chose `email-drafting`
  for a supplier-reply request; scripted stand-ins cover rogue, malformed and
  non-chat models.
- **High-risk actions never depend on any model's confidence.** The router
  and the disambiguator can only recommend. Starting a run goes through the
  executor's admission and approval machinery (approval gates, effect classes,
  safe-commit), which is deterministic and does not read router output.
  Scheduled goals follow the same rule: they never approve.
- **Offline and mobile memory.** Everything above runs on-device; no network
  path exists. `knowledge.release` unloads the embedding weights under memory
  pressure and every embed path reloads them on demand (tested); the prewarmed
  router cache holds only 35 vectors and survives a release.

## Done since (decision 0015)

- Matryoshka truncation (512/256/128-d) as a mobile index-size option, with
  its own calibration entries (`dimension` in the calibration data; 128-d has
  none) and the truncation in the index identity.
- `knowledge.release` is wired to the OS memory-warning callback.
- Embedding results verified on macOS CPU, Android arm64 and the iOS
  simulator; calibration itself was measured on Metal / Q8_0 only.

## Not done

- Calibration on other quantizations, and re-measurement per backend.
