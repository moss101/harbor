# Harbor patch — llama-cpp-sys-2 0.1.156 + `gemma-embedding2`

Decision 0013. This tree is the exact crates.io `llama-cpp-sys-2 0.1.156`
source with ONE change: the `gemma-embedding2` architecture (EmbeddingGemma 2
text tower) back-ported into the vendored `llama.cpp/src`.

- Upstream: ggml-org/llama.cpp commit `4fbc76dec5`
  ("model: support embeddinggemma2 (text+vision+audio)", PR #30054,
  2026-10-06). Only the `src/` hunks are applied; the Python converter and
  test hunks are not.
- Files touched (diff against the registry source to audit):
  `llama.cpp/src/llama-arch.{h,cpp}`, `llama.cpp/src/llama-model.cpp`,
  `llama.cpp/src/models/models.h`, new `llama.cpp/src/models/gemma-embedding2.cpp`
  (picked up by the existing `models/*.cpp` CMake glob).
- Adapted to this snapshot (two calls, same semantics as the Gemma-1 embedding
  model in this tree): `load_swa_pattern(ml, 6)` ->
  `get_key_or_arr(SLIDING_WINDOW_PATTERN)` + `set_swa_pattern`;
  `build_inp_embd(tok_embd, sqrtf(n_embd))` -> `build_inp_embd(tok_embd)` +
  `ggml_scale(.., ubatch.token ? sqrtf(n_embd) : 1.0f)`.
- No other architecture's code path is modified: the new enum value is
  appended after `LLM_ARCH_GEMMA_EMBEDDING`, and the other edits are one new
  `case` label per switch.
- Licence: llama.cpp is MIT; the patch is a verbatim upstream change.

Retire this directory (delete it and the `[patch.crates-io]` line in
`core/Cargo.toml`) when a `llama-cpp-sys-2` release vendors llama.cpp at or
after `4fbc76dec5`.
