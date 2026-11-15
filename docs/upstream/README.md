# Upstream reports (decision 0022 phase 5)

Drafts of the bug reports Harbor owes upstream projects. A draft is filed
only by a person, after it has been re-run on the upstream version named in
it; filing is public, so nothing here is sent automatically.

| Draft | Project | Status |
|---|---|---|
| [llama-cpp-adreno-vulkan-embeddinggemma2.md](llama-cpp-adreno-vulkan-embeddinggemma2.md) | ggml-org/llama.cpp | Draft. Needs a re-run on the vendored pin and the device's driver version. |
| [llama-cpp-vk-disable-f16-segfault.md](llama-cpp-vk-disable-f16-segfault.md) | ggml-org/llama.cpp | Draft. Needs a re-run and a backtrace. |

Before filing:
1. Rebuild the Android Vulkan core at the pin in `third_party/llama.cpp/HARBOR_PIN.json`
   (`scripts/build_android_vulkan.sh`) and reproduce with upstream's own tools
   (`llama-embedding`), not Harbor's app: maintainers can run those.
2. Fill in the `TODO` fields (driver version from `vulkaninfo --summary` or
   `adb shell dumpsys SurfaceFlinger | grep GLES`, the exact commit).
3. Search existing issues first (`gh issue list -R ggml-org/llama.cpp --search "adreno embedding"`).
4. Record the issue URL in the table above and in decision 0022.
