#!/bin/sh
# Build the Android arm64 native core and the inference worker launcher, and
# install both into the app's jniLibs (decision 0022 phase 3).
#
#   scripts/build_android_core.sh [--vulkan <workdir>]
#
# Produces apps/harbor_app/android/app/src/main/jniLibs/arm64-v8a/
#   libharbor_ffi.so     the core (loaded by Dart FFI)
#   libharbor_worker.so  the launcher: an EXECUTABLE named like a library so
#                        the installer extracts it into the native library
#                        directory (gradle useLegacyPackaging = true), the one
#                        place an app may execute files from.
# CPU-only by default; --vulkan builds the core through
# scripts/build_android_vulkan.sh (decision 0016: opt-in).
# Requires an Android NDK (ANDROID_NDK_HOME, default
# ~/Library/Android/sdk/ndk/27.1.12297006).
set -eu
repo="$(cd "$(dirname "$0")/.." && pwd)"
jni="$repo/apps/harbor_app/android/app/src/main/jniLibs/arm64-v8a"
NDK="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/27.1.12297006}"
[ -d "$NDK" ] || { echo "Android NDK not found at $NDK" >&2; exit 1; }
TC="$(echo "$NDK"/toolchains/llvm/prebuilt/*)"

# NDK env stays in this subshell: with the NDK clang first on PATH an
# iOS-simulator bindgen build in the same shell fails later.
(
  export ANDROID_NDK="$NDK" ANDROID_NDK_HOME="$NDK"
  export CC_aarch64_linux_android="$TC/bin/aarch64-linux-android30-clang"
  export CXX_aarch64_linux_android="$TC/bin/aarch64-linux-android30-clang++"
  export AR_aarch64_linux_android="$TC/bin/llvm-ar"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/bin/aarch64-linux-android30-clang"
  # Google Play rejects native libraries whose LOAD segments are not 16 KB
  # aligned (apps targeting Android 15+); the NDK 27 default is 4 KB.
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
  export PATH="$HOME/.cargo/bin:$TC/bin:$PATH"
  cd "$repo/core"
  if [ "${1:-}" = "--vulkan" ]; then
    "$repo/scripts/build_android_vulkan.sh" "${2:?usage: $0 --vulkan <workdir>}" \
      build --release -p harbor_ffi --features vulkan
  else
    cargo build --release -p harbor_ffi --target aarch64-linux-android
  fi
  cargo build --release -p harbor_worker --target aarch64-linux-android
)

out="$repo/core/target/aarch64-linux-android/release"
cp "$out/libharbor_ffi.so" "$jni/libharbor_ffi.so"
cp "$out/harbor_worker" "$jni/libharbor_worker.so"
"$TC/bin/llvm-nm" -D --defined-only "$jni/libharbor_ffi.so" | grep -q " harbor_worker_main$" \
  || { echo "FAIL: libharbor_ffi.so does not export harbor_worker_main" >&2; exit 1; }
file "$jni/libharbor_worker.so" | grep -q "executable" \
  || { echo "FAIL: libharbor_worker.so is not an executable" >&2; exit 1; }
for so in libharbor_ffi.so libharbor_worker.so; do
  "$TC/bin/llvm-readelf" -lW "$jni/$so" | awk '$1=="LOAD" && $NF!="0x4000" && $NF!="0x10000" {bad=1} END {exit bad}' \
    || { echo "FAIL: $so has LOAD segments that are not 16 KB aligned" >&2; exit 1; }
done
echo "installed: $jni/libharbor_ffi.so, $jni/libharbor_worker.so"
