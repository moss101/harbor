# Decision 0016 — Android GPU (Vulkan) build path and Windows/Linux video

Date: 2026-10-09
Status: Windows/Linux video: implemented and tested against real FFmpeg on
macOS (the same CLI behaviour on all OSes) but NOT run on Windows or Linux.
Android Vulkan: the backend builds for Android arm64 and the build is
reproducible; its on-device result is recorded separately once run.

## Windows and Linux video

macOS/iOS (AVFoundation) and Android (MediaMetadataRetriever) sample video
natively. Windows and Linux had nothing, so video was not offered there.

Decision: sample through the user's own **FFmpeg** (`lib/services/ffmpeg_frames.dart`),
not a bundled decoder. Bundling FFmpeg or a software H.264 decoder would put
codec patent and licence obligations (MPEG-LA, LGPL/GPL build flags) on Harbor;
the OS or the user's install carries them today. Native Media Foundation
(Windows) / GStreamer (Linux) samplers would avoid the dependency but need C++
in the Flutter runners that cannot be built or tested from this machine, so
they are future work rather than unverified shipped code.

- Lookup: `HARBOR_FFMPEG` / `HARBOR_FFPROBE`, next to the app executable
  (`.exe` on Windows), then `PATH`.
- Same shape as the native samplers: ~1 frame per second, evenly spaced
  midpoints, at most 24, JPEG, longest edge 512.
- Safety: no shell (argv list), the video path is only ever the argument after
  `-i` (a path that looks like an option stays a path), `-protocol_whitelist
  file` (a crafted file cannot make FFmpeg open a network URL), `-nostdin`,
  per-call timeouts, typed failures (`FfmpegUnavailable`, unreadable video) and
  never a partial frame set.
- Without FFmpeg the app says so ("Indexing video needs FFmpeg on this
  computer") instead of a generic "unsupported".

Verified: unit tests pin every command and failure path with a fake runner;
the live test samples the committed MP4s through a real FFmpeg 9.0 (Homebrew)
— 4 distinct JPEG frames. Not run: an actual Windows or Linux machine.

## Android GPU (Vulkan) — result on a real phone

The Android build previously had no GPU backend, which is why a phone took
~3.5 s per image on the CPU. The vendored `llama-cpp-sys-2` already knows how
to cross-compile ggml's **Vulkan** backend for Android; Harbor exposes it as
`harbor_inference/vulkan` / `harbor_ffi/vulkan` (off by default). The build
needs headers the NDK lacks (Khronos Vulkan C++ bindings, an installed
SPIRV-Headers); `scripts/build_android_vulkan.sh` fetches them at pinned
commits with pinned SHA-256s into a work directory outside the repo and uses
the NDK's `glslc`.

**Result on the OnePlus CPH2767 (Adreno 829, Vulkan, fp16 yes / bf16 no):
EmbeddingGemma 2 does NOT run correctly on this GPU.** ggml-vulkan finds the
device and offloads all 25 layers, but the embeddings are wrong — an English
contract question retrieved the travel document — which is the model card's
float16 hazard on a driver that does fp16 math. The numerics canary
(decision 0015) caught it at open (`cpu_fallback`) before any vector was
stored. Trying to repair it with ggml-vulkan's switches did not work:
`GGML_VK_DISABLE_F16=1` (fp32 shaders) makes the process **segfault**;
`DISABLE_FUSION`, `DISABLE_MMVQ`, `DISABLE_GRAPH_OPTIMIZE`, `DISABLE_MULTI_ADD`
and `DISABLE_ASYNC` still fail the canary. Conclusion: Vulkan stays an
opt-in build option, NOT enabled in Harbor's Android builds. A segfault cannot
be caught by an in-process check, so shipping Vulkan broadly would trade a
slow-but-correct CPU path for a risk of crashing on some drivers. Re-test on
other GPUs (Mali, other Adreno drivers) and when drivers update; OpenCL (the
Adreno-specific ggml backend) was not attempted — it also leans on fp16 and
needs an OpenCL loader this build does not have.

**A real bug found by this test, fixed:** the canary's "CPU fallback" set zero
GPU layers, which is NOT CPU-only when a GPU backend is compiled in — ggml's
scheduler still offloads heavy ops to the device, so the "fallback" kept
producing the GPU's wrong embeddings (both tests failed after a successful
fallback). The CPU pin now passes an empty device list, which removes the GPU
backend from the model entirely. After the fix, the same Vulkan-enabled build
falls back to a true CPU path and passes all text, Matryoshka and multimodal
tests on the phone (score 0.832777, identical to the CPU-only build).

Side effect on the Mac: a truly CPU-only run scores 0.836362 (the earlier
"CPU-only" 0.8330 was a hybrid with Metal offload); Metal scores 0.834097. All
within 0.5%. The iOS-simulator fallback score (0.836362) is the same true-CPU
number.
