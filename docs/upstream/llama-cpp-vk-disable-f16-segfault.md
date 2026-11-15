# Draft: GGML_VK_DISABLE_F16=1 segfaults on Adreno 829

**Project:** ggml-org/llama.cpp · **Component:** ggml-vulkan · **Status:** draft, not filed

## Summary
Setting `GGML_VK_DISABLE_F16=1` (fp32 shaders) to work around the fp16
overflow in the companion report makes the process segfault on an Adreno 829
while loading or running EmbeddingGemma 2 on Vulkan. Without the variable the
process runs (with wrong embeddings).

## Environment
Same as the companion report (TODO: fill in from it).

## Reproduction
```sh
GGML_VK_DISABLE_F16=1 ./llama-embedding -m embeddinggemma-2-Q8_0.gguf \
  --pooling mean -ngl 99 -p "title: none | text: hello"
```
Observed: SIGSEGV. TODO: backtrace (`adb shell` + `debuggerd -b <pid>`, or a
tombstone from `adb bugreport`) and whether it happens at pipeline creation
or at the first graph compute.

## Why it matters
An embedder app cannot catch a segfault in-process. Harbor now runs llama.cpp
in a worker process on Android and pins a device to the CPU after such a
crash, but the fp32 path is the natural fix for the fp16 overflow and is
currently unusable on this GPU.
