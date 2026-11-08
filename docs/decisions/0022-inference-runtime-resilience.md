# Decision 0022 — Inference runtime resilience: own the layer around llama.cpp, not a new engine

Date: 2026-11-02
Status: Accepted and implemented on branch `runtime-resilience` (2026-11-02). See
"Implementation record" at the end for what was built, where it departs from the plan
below, and the evidence. Not yet run: a physical iPhone, a physical Android phone, Windows
and Linux machines.

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

## Implementation record (2026-11-02)

### Phase 1 — implemented
- `harbor_inference::resilience`: `DeviceKey` (ggml backend + device description +
  OS build fingerprint + weights sha256 + runtime revision), `QualState`
  (`verifying`, `ok`, `qualified`, `canary_failed`, `crashed`), store at
  `<models root>/.runtime/qualification.json` (atomic write, re-read on every access so
  a worker process and the app share it), sentinels under `.runtime/sentinels/` named
  `<key>.<pid>.<init|first|work>.<seq>`, fsynced before the risky step. `init` and
  `first` sentinels pin at once; `work` pins after two consecutive. Sentinels of a live
  pid are never read as crashes.
- Wired into `GgufLlamaCppProvider::load` (store-pinned keys load on the true CPU path),
  `generate_cancellable`, `embed`, the media embedder and vision OCR (`WorkTicket`).
- **Departure — the knowledge canary was too weak.** On v0.8.0 the iOS-simulator GPU
  keeps the canary's ranking while its vectors are wrong (cosine 0.79 against the CPU),
  so the ranking-only canary passed it. A GPU key that is not yet `qualified` must now
  also match a CPU reference embedding (cosine >= 0.99 per text); the check runs once
  per key because the store remembers the verdict.
- FFI ops `runtime.qualification` (records, GPU presence, inference mode and why) and
  `runtime.qualification_reset`; Settings -> "GPU acceleration" lists each key with its
  reason and a "Re-test the GPU" action; the knowledge backend label gains `cpu`
  (no GPU path) and `cpu_pinned`.
- Debug-only fault hooks for tests: `HARBOR_FAULT_ABORT_AT_TOKEN` (aborts only while
  a GPU workload is armed) and `HARBOR_FAULT_SLOW_TOKEN_MS`. Neither exists in release.

### Phase 2 — implemented
- `gguf_check` parses the header, metadata and tensor table with every count and length
  bounded; checks type ids against ggml's table, exact contiguous offsets, data bounds,
  the architecture allow-list, and the generic `GGML_ASSERT`s in `load_hparams`
  (block count, expert invariants). Drift tests re-derive the type table, the
  architecture list and the `LLAMA_MAX_*` caps from the vendored sources; moving to
  v0.8.0 they caught 22 new architectures and `LLAMA_MAX_EXPERTS` 512 -> 1024.
- `memory_budget`: iOS `os_proc_available_memory`, Android/Linux `MemAvailable`,
  macOS/Windows total RAM (refuse only what can never fit). Load refuses with a typed
  error when weights + KV cache + projector + a 256 MiB margin exceed it.
- Fuzz target `gguf_header` (2 x 60 s, ~2.2M runs each, no crash); in `tools/fuzz.sh`.
- Not replicated: about 30 architecture-specific asserts, tokenizer contents, split
  files, clip's own key requirements.

### Phase 3 — implemented, with two departures
- `harbor_inference::worker`: framed JSON + blob protocol, `WorkerProvider`
  (supervisor), the worker loop (`harbor_worker_main` in the core library), and
  `GgufRuntime`, which knowledge, chat and vision OCR now hold. Media embedding and OCR
  run in the worker too.
- **Departure — no XPC service, no Binder service.** The worker is a dependency-free
  launcher (`core/harbor_worker`, about 0.3-0.5 MB) that loads the SAME core library the
  app ships and calls `harbor_worker_main`: one copy of llama.cpp, one code path, the
  same stdin/stdout protocol on every OS. macOS: `Contents/MacOS/harbor_worker`,
  signed with `app-sandbox` + `inherit` only (`WorkerHelper.entitlements`). Windows:
  `harbor_worker.exe` beside `harbor_ffi.dll`. Android: `libharbor_worker.so` in
  jniLibs, extracted by `useLegacyPackaging = true` and executed from the native
  library directory with that directory on `LD_LIBRARY_PATH` (without it the
  installed app's worker could not find `libc++_shared.so`; found on the emulator).
  iOS: in-process, always.
- Exit status decides: SIGKILL/SIGTERM or exit 0 = stopped from outside (low-memory
  killer, Android's phantom-process limit, our own stop) — sentinels discarded, never a
  pin; anything else = crash — sentinels recovered. A call that died is retried once
  only when recovery pinned a key (so the retry runs on the CPU); an idle worker that
  died is replaced silently. The worker exits when its stdin closes.
- `HARBOR_INFERENCE_WORKER=0|1` overrides; auto mode starts the worker at open and falls
  back to in-process if it cannot start.

### Phase 4 — implemented
- `third_party/llama.cpp` = upstream **v0.8.0 (8642541b25, 2026-10-28)**, unmodified,
  742 files, pinned by tarball sha256 and a tree hash in `HARBOR_PIN.json`
  (`tools/vendor_llama_cpp.py --check`, in CI). It includes EmbeddingGemma 2
  (4fbc76dec5), so the decision 0013 back-port is gone.
- `core/harbor_llama_sys` (build.rs adapted from llama-cpp-sys-2, MIT/Apache, notices
  kept) and `core/harbor_llama` (the wrapper trimmed to what Harbor uses, ~2.3k lines,
  with the 0015 embedding-batch and mtmd-output additions; five wrapper bugs fixed, see
  its `HARBOR.md`). `third_party/llama-cpp-sys-2` and `third_party/llama-cpp-2` and their
  `[patch.crates-io]` lines are deleted. Download-capable build options are pinned OFF.
- `runtime_revision()` = `llama.cpp@v0.8.0+8642541b25`. **Migration:** existing
  knowledge indexes carry the old revision in their identity and rebuild from their
  sealed texts on first open (by design: vectors from different runtimes never mix);
  media items must be re-imported, as with any identity change (decision 0015).
- Upstream behaviour change: the JSON-schema grammar converter now degrades an
  unanchored `pattern` to an unconstrained string instead of refusing; Harbor's graph
  validator already refuses such schemas.

### Phase 5 — implemented (reports drafted, not filed)
- `harbor_inference/tests/numerics_canary.rs` (default backend vs true CPU, cosine
  >= 0.99 per text, Matryoshka 512/256/128) and a CI `numerics` job that fetches pinned,
  hashed fixtures (`tools/fetch_numerics_fixtures.py`) and runs it on Metal and CPU.
- `docs/upstream/`: the Adreno Vulkan wrong-embedding and `GGML_VK_DISABLE_F16`
  segfault reports. Filing is public and needs a re-run on the pin and the device's
  driver version first.

### Evidence (this Mac, Apple M5 Pro, macOS 27.0.1)
| Check | Result |
|---|---|
| `harbor_inference` unit + real-model tests (v0.8.0) | pass; resilience 17, gguf_check/memory_budget, 12/12 real fixtures |
| Crash mid-GPU-generation, in-process (real abort) | next process loads on CPU (`cpu_pinned`), generates; reset restores GPU |
| Worker tests (crash, OS kill, GPU-crash retry, cancel, parity) | 5/5 on macOS and on the Android 14 arm64 emulator |
| `harbor_ffi` full suite, `HARBOR_INFERENCE_WORKER=required` | 31/31 binaries on v0.8.0 (30/30 in-process and in worker mode before the runtime swap) |
| Flutter suite (auto mode: the worker is chosen) | 210/210 on v0.8.0, incl. the GPU card and worker-mode tests |
| Numerics canary v0.8.0, Metal vs CPU | cosine >= 0.99987; margins 0.3225 / 0.3244 |
| Media retrieval v0.8.0 vs 0.1.156 | ranks unchanged, scores within 0.15 % |
| Vision OCR (Qwen2.5-VL-3B) v0.8.0 | CER 0.0000 all pages (Arabic improved from 0.086) |
| iOS simulator (in-process) | GPU embeds wrong (cosine 0.79); first open `cpu_fallback`, second `cpu_pinned` |
| Sandboxed macOS release app, qualification harness | `inference_mode=worker`, Metal key `qualified`, retrieval 5/5 |
| Android 14 emulator, installed APK, harness | `inference_mode=worker` from the native library dir, `cpu`, retrieval 5/5 |

Not run: a physical iPhone (Xcode had no signed-in account on 2026-11-02), a physical
Android phone (none attached; the Adreno 829 result above is from decision 0016),
Windows and Linux (the launcher type-checks for `x86_64-pc-windows-msvc`; the rest of
the Windows code is checked by CI's `windows-core` job).

