# Formula qualification platform tiers — operator runbook

The formula engine qualification (`22_Formula_Coverage.json`) requires
five platforms before any target may flip to PASS. Three tiers are
machine-bound (macOS, iOS Simulator, Android emulator). They were first
recorded 139/139 cases · 99/99 targets on engine `0.9.3+harbor-textfix`,
bundle `e1fc88a3…`; those records are kept but marked superseded. On
2026-10-04 the engine gained the Excel-fidelity patch set (decision 0001
addendum 2, `0.9.3+harbor-excel-fidelity`) and the corpus grew to 398 targets,
so the three tiers were re-run on commit `59a9269` (bundle `31d30c11…`, 407
targets, no deviations): macOS 1261/1261, iOS Simulator 1261/1261, Android emulator
1261/1261. (An intermediate run on `9e32a51` failed two Android cases, IMCOSH and
IMSECH, on the last printed digit of a complex-number text result; those cases now
compare the real and imaginary parts numerically.) The remaining tiers need hardware this repository's
machines do not have. Exact commands, verified on this host:

## Physical iOS/iPadOS arm64 device — DONE (2026-10-04)

Run on an iPhone 17 Pro Max (iOS 27.0) from commit `59a9269`: 1261/1261 cases ·
407/407 targets (`evidence/formula_evals/ios-device-arm64-59a9269.json`). The runner
is `tools/qualify_ios` (see its README for the exact commands, the signing/trust
steps and the one trap: the `[patch.crates-io]` entry). iPadOS is not separately run.

## Physical Android arm64-v8a device — DONE (2026-10-05)

Run on a OnePlus CPH2767 (Android 16, arm64-v8a) from commit `2d2614c`: 1261/1261
cases · 407/407 targets (`evidence/formula_evals/android-device-arm64-2d2614c.json`).
The commands below are the ones used (NDK 27.1 linker is set in `core/.cargo/config.toml`;
only `adb` authorisation on the phone was needed).

```
NDK=~/Library/Android/sdk/ndk/27.1.12297006/toolchains/llvm/prebuilt/darwin-x86_64/bin
export PATH="$HOME/.cargo/bin:$PATH"          # rustup first, not Homebrew rust
CC_aarch64_linux_android=$NDK/aarch64-linux-android24-clang \
AR_aarch64_linux_android=$NDK/llvm-ar \
  cargo build --release -p harbor_formula --target aarch64-linux-android --example qualify_dump
adb push core/target/aarch64-linux-android/release/examples/qualify_dump /data/local/tmp/
adb shell chmod +x /data/local/tmp/qualify_dump
adb shell /data/local/tmp/qualify_dump > evidence/formula_evals/android-device-arm64-<commit>.json
```
Build inside the core workspace so the vendored engine is linked (a standalone crate
needs the `[patch.crates-io]` entry; see `tools/qualify_ios/README.md`).

## Windows x64 / arm64

Windows binaries cannot be linked on the macOS hosts (no MSVC linker,
no mingw). On a Windows host with Rust + the `x86_64-pc-windows-msvc`
(or arm64) target:

```
cargo build --release -p harbor_formula --example qualify_dump
target\release\examples\qualify_dump.exe out.json
```

## After every tier

1. Verify `engine_version` is `0.9.3+harbor-excel-fidelity`, the integrity hash
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
