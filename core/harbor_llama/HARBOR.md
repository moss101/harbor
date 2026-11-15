# harbor_llama / harbor_llama_sys (decision 0022, phase 4)

Harbor-owned bindings to llama.cpp, replacing the patched `llama-cpp-2` /
`llama-cpp-sys-2` 0.1.156 trees that used to live in `third_party/` behind
`[patch.crates-io]`.

```
third_party/llama.cpp/   upstream ggml-org/llama.cpp, unmodified subset, pinned
                         (HARBOR_PIN.json: tag, commit, tarball sha256, tree hash)
core/harbor_llama_sys/   build.rs (CMake + cc + bindgen) and raw FFI;
                         derived from llama-cpp-sys-2 0.1.156 (MIT OR Apache-2.0)
core/harbor_llama/       safe wrapper, trimmed to what harbor_inference uses;
                         derived from llama-cpp-2 0.1.156 (MIT OR Apache-2.0)
```

`harbor_inference` depends on `harbor_llama` under the dependency name
`llama-cpp-2`, so its `llama_cpp_2::...` paths did not change.

## Updating the runtime

1. `python3 -I tools/vendor_llama_cpp.py --tag <tag> --commit <40-hex> --date <YYYY-MM-DD>`
   (downloads the GitHub tarball, records its sha256, re-extracts the subset,
   rewrites HARBOR_PIN.json). `--check` re-hashes the tree against the pin.
2. Fix API breaks in `harbor_llama_sys/wrapper_*` and `harbor_llama/src`.
   If the JSON-schema converter gains a dependency on another `common/`
   function, the link fails on an undefined symbol: add it to
   `harbor_llama_sys/common_shim.cpp`, or extend `COMMON_SOURCES` in build.rs
   and the pin tool's include list.
3. Rebuild every target (macOS, iOS device + simulator, Android CPU + Vulkan,
   Windows) and run the qualification matrix. `runtime_revision()` changes
   with the pin (`llama.cpp@<tag>+<commit[..10]>`), so embedding indexes
   record the new runtime.

## harbor_llama_sys: what changed from llama-cpp-sys-2 0.1.156

- Builds from `third_party/llama.cpp` (override: `HARBOR_LLAMA_CPP_DIR`) and
  generates `LLAMA_CPP_TAG`, `LLAMA_CPP_COMMIT`, `LLAMA_CPP_REVISION`,
  `LLAMA_CPP_RUNTIME` from HARBOR_PIN.json. `LLAMA_BUILD_COMMIT` comes from
  the pin, not from `git` in the enclosing Harbor checkout.
- `mtmd` is built by llama.cpp's own CMake target (`LLAMA_BUILD_MTMD=ON`, the
  explicit `add_library(mtmd ...)` list) instead of a `tools/mtmd/**/*.cpp`
  glob, which now also matches programs with their own `main()`. The static
  `vendor-hash` library mtmd needs (xxHash / SHA-1 / SHA-256; no install rule)
  is linked from the CMake build tree.
- `common`: only `json-schema-to-grammar`, `json-schema`, `json`, `trie`,
  `unicode` are compiled (plus `common_shim.cpp`: three string helpers copied
  from `common.cpp`). llama-common, cpp-httplib, subprocess and download code
  are not built (`LLAMA_BUILD_COMMON=OFF`).
- Pinned OFF: `LLAMA_SUBPROCESS`, `LLAMA_CONNECT`, `LLAMA_OPENSSL`,
  `LLAMA_BUILD_UI`, `LLAMA_USE_PREBUILT_UI`, `MTMD_VIDEO`, `GGML_OPENMP_FETCH`,
  `GGML_CPU_KLEIDIAI` — nothing is downloaded at build time.
- `wrapper_common.cpp`: `json_schema_to_grammar` takes a `common_json`
  (`common_json::parse`); only the JSON-schema, grammar-sampler and
  string-free shims are kept (fit / memory breakdown / MTP speculative /
  lazy grammar / sampler-accept shims removed).
- Removed: CUDA, ROCm, OpenCL, MKL, system-ggml, dynamic-link and
  dynamic-backends support, watchOS handling, DLL copying (Harbor links
  llama.cpp statically everywhere).
- Kept: Android NDK detection and Vulkan cross-build inputs
  (`VULKAN_INCLUDE_DIR`, `SPIRV_HEADERS_DIR`, `VULKAN_GLSLC`), MSVC handling,
  `GGML_*` / `CMAKE_*` env forwarding, target-cpu feature mapping.

## harbor_llama: what was kept, removed and added

Kept (the surface `harbor_inference` uses):

- `llama_backend::LlamaBackend::init`
- `model::LlamaModel::{load_from_file, new_context, str_to_token,
  token_to_piece, token_to_piece_bytes, token_eos, is_eog_token, chat_template,
  apply_chat_template, n_ctx_train, n_embd, n_embd_out, n_cls_out, n_vocab}`,
  `AddBos`, `LlamaChatMessage::new`, `LlamaChatTemplate`
- `model::params::LlamaModelParams::{default, with_n_gpu_layers, with_devices}`
- `context::params::LlamaContextParams` builders (`n_ctx`, `n_batch`,
  `n_ubatch`, `embeddings`, `pooling_type`, `n_threads`, `n_threads_batch`),
  `LlamaPoolingType`
- `context::LlamaContext::{decode, embeddings_seq_ith, get_logits_ith,
  n_batch, n_ctx}`
- `llama_batch::LlamaBatch::{new, add}`
- `sampling::LlamaSampler::{grammar, greedy, temp, dist, chain_simple, sample}`
- `json_schema_to_grammar`, `list_llama_ggml_backend_devices`,
  `LlamaBackendDevice(Type)`, `max_devices`
- `mtmd` (feature): `MtmdContext::{init_from_file, support_vision,
  support_audio, get_audio_sample_rate, tokenize, encode_chunk}`,
  `MtmdContextParams`, `MtmdBitmap::{from_buffer, from_image_data,
  from_audio_data}`, `MtmdInputText`, `MtmdInputChunks::{len, is_empty, get,
  total_tokens, eval_chunks}`, `MtmdInputChunk::{chunk_type, n_tokens,
  text_tokens}`, `MtmdInputChunkType`, `mtmd_default_marker`

Added by Harbor (decision 0015; carried over from the old patch):

- `LlamaBatch::new_embd` / `LlamaBatch::add_embd` — a batch of raw input
  embeddings, to embed text, image and audio positions in one forward pass.
- `MtmdContext::output_embeddings` — the embeddings left by `encode_chunk`.
- `LLAMA_CPP_TAG`, `LLAMA_CPP_COMMIT`, `RUNTIME_REVISION`.

Removed: sessions / state save-restore, KV-cache manipulation, speculative
decoding, llguidance, the GGUF metadata reader, timings, LoRA adapters,
KV / tensor-buffer overrides, model fitting, logit bias, token data arrays
and the remaining samplers, token attributes, NUMA init, log redirection
to `tracing`, and most model / context getters.

Fixed while trimming:

- `LlamaModel::chat_template(Some(name))` passed a pointer into a `CString`
  that had already been dropped; the name now outlives the call.
- `token_to_piece_bytes` leaked its buffer on the error path.
- `MtmdBitmap` derived `Clone` over an owning pointer (double free on drop of
  a clone); it is no longer `Clone`.
- `MtmdInputChunk` borrows its `MtmdInputChunks` (lifetime) instead of
  holding an untracked pointer.
- `mtmd_helper_bitmap_init_from_buf` takes `mtmd_helper_init_opt` (default
  options passed) and returns a wrapper whose (always null here) video
  context is freed if present.
- `with_devices` keeps one slot for the device list's null terminator.
- Uses the non-deprecated llama.h names (`llama_model_load_from_file`,
  `llama_init_from_model`, `llama_model_free`, `llama_vocab_*`,
  `llama_model_n_*`).

## Behaviour change noticed in the upgrade

llama.cpp v0.8.0's JSON-schema converter no longer rejects an unanchored
`pattern` (or a pattern combined with `minLength`/`maxLength`): it degrades
the property to an unconstrained string and prints a warning. Harbor's graph
validator (`harbor_core::graph`) already rejects such schemas, so structured
output is unaffected; `harbor_inference/tests/schema_grammar_probe.rs` now
asserts "not enforced" (error or unconstrained) rather than "rejected".

## Licence

MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE; see NOTICE), as the
upstream crates. llama.cpp itself is MIT (`third_party/llama.cpp/LICENSE`).
