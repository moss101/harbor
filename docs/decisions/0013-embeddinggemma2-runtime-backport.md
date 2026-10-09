# Decision 0013 — EmbeddingGemma 2 runs: `gemma-embedding2` back-ported into the pinned runtime

Date: 2026-10-09
Status: Accepted (loads and retrieves live; catalog pin NOT shipped — see
Consequences). Supersedes the "BLOCKED" state recorded in decision 0011 §1.

## Problem

Decision 0011 recorded EmbeddingGemma 2 as blocked: the official GGUF declares
`general.architecture = gemma-embedding2`, and neither the pinned
`llama-cpp-sys-2 0.1.156` nor the newest release (0.1.159) vendors a llama.cpp
that knows it. That decision rejected bumping the pin (re-qualifies every chat
and embedding path) and did not consider back-porting the one upstream change.

## Findings

- Upstream llama.cpp added the architecture in one commit, `4fbc76dec5`
  (PR #30054, 2026-10-06, "support embeddinggemma2 (text+vision+audio)"). Its
  `src/` footprint is five files: arch enum + name, a `case` in the model
  factory / memory / rope-type switches, one new graph file, one struct in
  `models.h`. New model files are picked up by the existing CMake glob.
- The model card (google/embeddinggemma-2): 768-d, Matryoshka 512/256/128
  (re-normalize), mean pooling, 8,192-token shared context, task prefixes,
  and **never float16** (NaN / silent degradation). The ggml-org GGUF is
  ungated, Apache-2.0, rev `bfcd2987`; Q8_0 weights are 310 MB
  (sha256 `2188ac1d…b09135`).

## Decision

Vendor `llama-cpp-sys-2 0.1.156` at `third_party/llama-cpp-sys-2` (precedent:
`formualizer-eval`, decision 0001 addendum) and apply only the `src/` hunks of
`4fbc76dec5`, via `[patch.crates-io]`. Two helper calls differ in the 0.1.156
snapshot and were adapted to the same semantics as this tree's Gemma-1
embedding model (details and the audit recipe in
`third_party/llama-cpp-sys-2/HARBOR_PATCH.md`). Version stays 0.1.156, so no
other binding or API changes and decision 0002's pin discipline is preserved
(this is the same runtime plus one architecture, not a new runtime).
Isolation: the enum value is appended, other edits are single `case` labels;
no other architecture's code path changes.

## Evidence (this machine, Apple Metal)

- Production path, `harbor_ffi/tests/embeddinggemma2_live.rs`: installs the
  GGUF through the staged installer, opens `KnowledgeService`, dimension 768,
  ingest embeds all chunks with finite vectors, retrieval correct for English,
  **Arabic and French queries against English text**, `memory:` isolation
  holds, scores finite (top > 0.3) and bit-stable across runs.
- Router, same 106 labeled cases as bge-m3: top-1 60% (ar 61 / fr 67 / en 59),
  top-3 80%. bge-m3 is ~62% overall, so routing accuracy is a wash; the score
  band differs (EG2 0.6–0.8 vs bge-m3 0.7–0.9). The bge-m3 bars (0.75/0.02)
  would answer 13% of requests with EG2, so `RouteThresholds::for_policy`
  now ships 0.70/0.02 for the Gemma policy (44% coverage, 79% precision when
  confident on this set; 0.70/0.05 = 29% / 87%). Small corpus: re-measure
  before relying on the precision number.
- Float16: Metal showed no NaN / degradation. Other platforms were later
  verified (decision 0015): CPU on macOS, Android arm64 (emulator), and the
  iOS simulator — whose GPU path DOES degrade silently and is now caught by a
  numerics canary that falls back to CPU.

## Consequences

- EmbeddingGemma 2 is a shipped catalog option (signed epoch 5, decision
  0015) and was verified on macOS, Android arm64 (emulator) and the iOS
  simulator. `fixtures/models/embeddinggemma-2-Q8_0.gguf` is gitignored
  (>100 MB).
- Multimodal: the `src/` hunks cover the TEXT tower. The vendored mtmd
  already supports the model's projector types, so image/audio embedding was
  built in decision 0015 (joint forward pass, `media:` sources). Video is
  not (no frame decoder).
- Retire the vendored tree when a `llama-cpp-sys-2` release vendors llama.cpp
  at or after `4fbc76dec5`.
