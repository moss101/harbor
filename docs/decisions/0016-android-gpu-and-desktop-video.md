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

## Android GPU

The Android build previously had no GPU backend, which is why a phone took
~3.5 s per image on the CPU. The vendored `llama-cpp-sys-2` already knows how
to cross-compile ggml's **Vulkan** backend for Android; Harbor now exposes it as
`harbor_inference/vulkan` / `harbor_ffi/vulkan` (off by default). The build
needs headers the NDK lacks (Khronos Vulkan C++ bindings, an installed
SPIRV-Headers); `scripts/build_android_vulkan.sh` fetches them at pinned
commits with pinned SHA-256s into a work directory outside the repo, and uses
the NDK's `glslc`. The runtime picks a Vulkan GPU when the device has one and
falls back to the CPU otherwise; the embedding numerics canary (decision 0015)
rejects a GPU path that embeds wrongly — EmbeddingGemma 2 must not run in
float16, and mobile GPU drivers are exactly where that could bite.
