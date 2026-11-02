# Draft: Vulkan on Adreno 829 produces wrong EmbeddingGemma 2 embeddings (fp16, no bf16)

**Project:** ggml-org/llama.cpp · **Component:** ggml-vulkan · **Status:** draft, not filed

## Summary
On an Adreno 829 (OnePlus CPH2767, Android), the Vulkan backend loads
EmbeddingGemma 2 (`gemma-embedding2`), offloads all 25 layers and returns
finite but wrong embeddings. The same GGUF on the CPU backend, and on Metal
(Apple M-series), gives correct embeddings. The device reports fp16 support
and no bf16. EmbeddingGemma 2's model card warns that its activations overflow
float16; this looks like the Vulkan fp16 path overflowing silently.

## Environment
- Device: OnePlus CPH2767, Adreno 829, Android (TODO: Android version, driver
  version from `vulkaninfo --summary`)
- llama.cpp: TODO exact commit (first seen on b10405 + the gemma-embedding2
  back-port of 4fbc76dec5; re-run on the current pin before filing)
- Model: `ggml-org/embeddinggemma-2-GGUF`, `embeddinggemma-2-Q8_0.gguf` (rev bfcd2987)
- Build: `-DGGML_VULKAN=ON`, arm64-v8a, NDK (TODO version)

## Reproduction (upstream tools)
```sh
# on device, with the Vulkan build:
./llama-embedding -m embeddinggemma-2-Q8_0.gguf --pooling mean -ngl 99 \
  -p "title: none | text: The cat is sitting on the mat." \
  -p "title: none | text: A cat sits on a mat." \
  -p "title: none | text: Quarterly revenue increased by ten percent." \
  --embd-output-format array
# the same with -ngl 0 --device none (true CPU) for comparison
```
Expected: sentence 1 is closest to sentence 2 (cosine ~0.8 on CPU and Metal).
Observed on Vulkan: the ranking breaks. In Harbor's retrieval test an English
contract question retrieved a travel document.

## What was tried
`GGML_VK_DISABLE_FUSION`, `GGML_VK_DISABLE_MMVQ`, `GGML_VK_DISABLE_GRAPH_OPTIMIZE`,
`GGML_VK_DISABLE_MULTI_ADD` and `GGML_VK_DISABLE_ASYNC` each still give wrong
embeddings. `GGML_VK_DISABLE_F16=1` segfaults (separate report).

## Note for maintainers
`-ngl 0` alone is not CPU-only when a GPU backend is compiled in: the
scheduler still offloads host-weight ops, so the wrong result persists. An
empty device list is needed to compare against the true CPU path.
