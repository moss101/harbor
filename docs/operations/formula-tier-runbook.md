# Formula qualification platform tiers — operator runbook

The formula engine qualification (`22_Formula_Coverage.json`) requires
five platforms before any target may flip to PASS. Three tiers are
machine-bound and already recorded (macOS, iOS Simulator, Android
emulator — all 139/139 cases · 99/99 targets on engine
`0.9.3+harbor-textfix`, corpus bundle `e1fc88a3…`). The remaining tiers
need hardware this repository's machines do not have. Exact commands,
verified on this host:

## Physical iOS/iPadOS arm64 device

```
rustup run 1.97.1-aarch64-apple-darwin cargo build --release \
  -p harbor_formula --target aarch64-apple-ios --example qualify_dump
# Sign for development, push to an attached device (Xcode Devices,
# devicectl, or an app container) and run:
./qualify_dump out.json   # copy out.json back to the host
```
Then save as `evidence/formula_evals/ios-device-arm64-<commit>.json`
(bind the commit the binary was built from — the in-device git probe is
empty by design) and append the tier to `qualification_results`.

## Physical Android arm64-v8a device

```
export NDK_CLANG=~/Library/Android/sdk/ndk/28.2.13676358/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android35-clang
rustup run 1.97.1-aarch64-apple-darwin env \
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$NDK_CLANG \
  cargo build --release -p harbor_formula \
  --target aarch64-linux-android --example qualify_dump
adb push core/target/aarch64-linux-android/release/examples/qualify_dump /data/local/tmp/
adb shell /data/local/tmp/qualify_dump > android-device.json
```

## Windows x64 / arm64

Windows binaries cannot be linked on the macOS hosts (no MSVC linker,
no mingw). On a Windows host with Rust + the `x86_64-pc-windows-msvc`
(or arm64) target:

```
cargo build --release -p harbor_formula --example qualify_dump
target\release\examples\qualify_dump.exe out.json
```

## After every tier

1. Verify `engine_version` is `0.9.3+harbor-textfix`, the integrity hash
   matches `engine.rs`, and the bundle sha is `e1fc88a3…`.
2. `python3 tools/pin_engine.py --check`.
3. Add the tier record to `22_Formula_Coverage.json`
   (`qualification_results`), regen the dossier
   (`tools/validate_dossier.py --write`), commit.
4. When the full five-platform set qualifies with zero deviations, flip
   target statuses to PASS **with** `qualification_results` bound per
   the verification rule.

Gotchas: use the rustup toolchain (Homebrew rust has no cross stds);
the Android linker flag is required or the link step fails; simctl/
adb run contexts have no git, so commit binding is host-side.
