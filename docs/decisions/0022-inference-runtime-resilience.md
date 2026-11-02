# Decision 0022 — Inference runtime resilience: own the layer around llama.cpp, not a new engine

Date: 2026-11-02
Status: Proposed. Nothing in this record is implemented yet; each phase below lands
with its own evidence and moves its line to "implemented" here.

## Problem

Harbor runs every GGUF model (chat, EmbeddingGemma 2, multimodal projectors) through a
pinned llama.cpp snapshot (decisions 0002, 0013, 0015, 0016). The runtime has failed us
in four distinct ways, all on the record:

1. **Silent wrong numerics on GPUs.** The iOS simulator GPU (no bfloat) and an Adreno 829
   phone over Vulkan both loaded EmbeddingGemma 2 and embedded wrongly. Only the numerics
   canary (0015) caught it.
2. **Process-fatal failures.** llama.cpp asserts call `abort()`; `GGML_VK_DISABLE_F16=1`
   segfaults on the Adreno driver (0016). `gguf.rs` guards known assert paths by hand
   (`n_batch`, context bounds), but an unknown one still takes the whole app down, and
   the canary cannot observe a crash.
3. **Binding lag.** `llama-cpp-2` vendors a snapshot behind upstream, so supporting a new
   architecture means back-porting `src/` hunks into `third_party/llama-cpp-sys-2`, and
   the multimodal joint batch needed a second patched crate (`third_party/llama-cpp-2`).
4. **Build fragility.** NDK/Vulkan header fetching, PATH ordering for iOS cargo targets,
   and an iOS device build that can ship a stale `libharbor_ffi.a`.

The question raised on 2026-11-02 was whether to write our own inference engine, more
resilient than llama.cpp, covering iOS, Android, Windows, Linux and macOS.

## What was evaluated

- **llama.cpp** (ggml-org, MIT): about 2,185 contributors, release builds several times
  a day (b12032 on 2026-11-01), 18 ggml backends including Metal, Vulkan, OpenCL (Adreno),
  Hexagon, CUDA, HIP, SYCL and CPU. GGUF is its native format; every model Harbor ships
  is GGUF.
- **TensorFold** (ashhart/TensorFold @ `088cfbd`, Apache-2.0, created 2026-06-19): a Zig
  server engine of about 743k lines with Metal and CUDA kernels, plus unofficial HIP and
  Intel Level Zero ports. Its Vulkan code is contract tests only. It loads MLX/safetensors
  checkpoints and per-model-family kernels for a handful of large models (Qwen 2B/27B,
  Nemotron 30B-A3B, GLM). Its own `docs/quantization.md` says GGUF needs its own
  readers and kernels. There is no on-device inference on phones; `app/mobile/` is a
  React Native client marked preview. About 5,450 commits in 4.5 months, over half by one
  author. Its `docs/known-issues.md` records the same model decoding different tokens
  across backends and chip generations.

## Decision

1. **Do not write a new inference engine, and do not adopt TensorFold.** A new engine
   inherits the GPU driver bugs behind failures 1 and 2 and adds its own, gives up
   llama.cpp's backend and architecture coverage, and turns every new model (as
   EmbeddingGemma 2 was) into kernel work. TensorFold covers none of Harbor's targets:
   phones, GGUF, small embedders.
2. **Own a resilience layer around a pinned llama.cpp.** The layer turns failures 1–4 into
   handled, visible states. It is built in the phases below, each independently useful.
3. **Keep the CPU path the one Harbor always falls back to.** A GPU backend is used only
   on a device whose (backend, device, driver, model, runtime) combination has passed the
   canary and has not crashed. Everything else runs on the true CPU path (empty device
   list, decision 0016's fix).

## Phases

### Phase 1 — Crash sentinel and qualification cache (all platforms, in-process)

- Key: `(backend, device id, driver version, model sha256, runtime revision)`.
- Before a GPU backend is initialised for a key, write and fsync a sentinel file in the
  app support directory. Clear it only after the numerics canary passes. If the sentinel
  is present at the next open, the previous attempt died during GPU init or canary: record
  `crashed` for that key and pin CPU.
- A second sentinel brackets in-flight GPU generation. Two consecutive sessions that end
  with it set on the same key mark that key `crashed` as well.
- Results (`qualified`, `canary_failed`, `crashed`) persist in a small qualification
  store. A changed driver version or runtime revision makes a key eligible for one
  re-test. The status is shown in diagnostics, never hidden.
- This is the only crash defence available on iOS, where Harbor cannot spawn a helper
  process. It works there because the sentinel survives the crash.

Evidence to land with it: a unit test per state transition; a forced-abort test (child
process aborting between arm and clear) showing the next open pins CPU; the Adreno 829
phone ending in `canary_failed` and the iOS simulator in CPU, as recorded today.

### Phase 2 — Pre-load validation

- Parse and bound-check the GGUF header, metadata and tensor table in Rust before
  llama.cpp sees the file: tensor count and offsets within the file, known type ids, sane
  context and embedding sizes, architecture on the allow-list for this runtime revision.
  Extend `core/fuzz` with a GGUF target over this parser.
- Check a memory budget before load (weights + KV cache + compute buffers against
  available memory: `os_proc_available_memory` on iOS, `ActivityManager.MemoryInfo` on
  Android, OS queries on desktop). Refuse with a typed error instead of being killed by
  jetsam or the low-memory killer.

Evidence: fuzz corpus run with no llama.cpp abort reachable from a file that passes
validation; a refusal test per budget rule.

### Phase 3 — Out-of-process worker where the OS allows it

- macOS: an XPC service bundled in the app. Windows: a child process in a job object.
  Linux: a child process. Android: a service in its own `android:process`, over Binder.
- The worker owns llama.cpp. A crash surfaces as a typed `RuntimeCrashed` error, feeds the
  Phase 1 store, and the next request restarts the worker (CPU-pinned for that key).
- iOS stays in-process (Phase 1 covers it). The worker protocol carries the existing
  `ModelProvider` calls only; nothing else crosses the boundary.

Evidence: kill-the-worker tests on each desktop OS and on an Android device, with the
request completing on the restarted worker and no app exit.

### Phase 4 — Own bindings from a pinned llama.cpp commit

- Replace `llama-cpp-2` / `llama-cpp-sys-2` with a Harbor-owned sys crate: bindgen over
  `llama.h` and `mtmd.h` from a llama.cpp commit pinned as a submodule (or a hashed
  tarball, matching how `build_android_vulkan.sh` pins its inputs), plus a thin safe
  wrapper exposing only what `harbor_inference` uses, including the joint embedding batch
  and `mtmd` output embeddings that `third_party/llama-cpp-2` patches in today.
- Updating the runtime becomes: bump the commit, rebuild every target, run the
  qualification matrix. Retire `third_party/llama-cpp-2` and `third_party/llama-cpp-sys-2`
  and their `[patch.crates-io]` lines in the same change.
- `harbor_inference::gguf::runtime_revision()` reports the llama.cpp commit.

Evidence: the existing provider, multimodal and Matryoshka tests passing unchanged on
macOS Metal + CPU, iOS simulator, an iPhone, Android arm64 (emulator and device); embedding
scores within the 0.5 % band recorded in decision 0016.

### Phase 5 — Upstream and cross-platform numerics in CI

- Report the Adreno fp16 embedding error and the `GGML_VK_DISABLE_F16` segfault upstream
  with minimal reproductions, and send fixes where we have them.
- Run the numerics canary set (text, Matryoshka, multimodal) on every runtime bump across
  the platforms CI can reach, and keep device-only results in the qualification record.

## Alternatives rejected

- **Our own engine (Rust or Zig).** See Decision 1. Reconsider only for one narrow case,
  a small CPU-only engine for the embedder if bit-identical vectors across platforms ever
  become a requirement, and only after Phases 1–3 exist.
- **Fork llama.cpp.** A long-lived fork recreates the binding-lag problem one level down.
  Harbor carries a fork-free pin plus upstreamed fixes.
- **Enable Vulkan broadly on Android and rely on the canary.** Decision 0016 already
  rejected this: the canary cannot catch a segfault. Phases 1 and 3 are what would make
  re-enabling it per device safe.

## Exclusions

No new model, backend or platform is enabled by this decision. Windows and Linux evidence
requires real machines; until those runs exist, those lines stay "not run" as in 0016.
