#!/bin/sh
# Build harbor_ffi (or run its tests) for Android arm64 WITH the Vulkan GPU
# backend (decision 0016).
#
#   scripts/build_android_vulkan.sh <workdir> cargo-args...
#   e.g. scripts/build_android_vulkan.sh /tmp/hv build --release -p harbor_ffi --features vulkan
#
# The Vulkan backend needs build-time headers the NDK does not ship: the
# Khronos Vulkan C++ bindings (vulkan.hpp) and an INSTALLED SPIRV-Headers
# package. They are fetched into <workdir> (never into the repo) at exact
# pinned commits and checked against pinned SHA-256s; `glslc` comes from the
# NDK's shader-tools. Requires: curl, tar, cmake, shasum, an Android NDK
# (ANDROID_NDK_HOME, default ~/Library/Android/sdk/ndk/27.1.12297006).
set -eu

VULKAN_HEADERS_SHA=c46850864f4661461b0f6cb9922c058ffea4915e
VULKAN_HEADERS_SHA256=539ff8eaa4abf22fddbd98225c68a0c5ddb940075a4048c8b7f5d04449070a4e
SPIRV_HEADERS_SHA=86f980c731e62ae4eaf383d320449d71687936bf
SPIRV_HEADERS_SHA256=d11ef011d0788ed002d7cc96224d298f43b126cabe887cd4d46199cb19ecf386

work="${1:?usage: $0 <workdir> cargo-args...}"
shift
NDK="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/27.1.12297006}"
[ -d "$NDK" ] || { echo "Android NDK not found at $NDK" >&2; exit 1; }
mkdir -p "$work"
work="$(cd "$work" && pwd)"

fetch() { # name url sha256 dest
  if [ ! -d "$4" ]; then
    curl -fsSL -o "$work/$1.tgz" "$2"
    got="$(shasum -a 256 "$work/$1.tgz" | cut -d' ' -f1)"
    [ "$got" = "$3" ] || { echo "$1: checksum mismatch ($got)" >&2; exit 1; }
    mkdir -p "$4"
    tar xzf "$work/$1.tgz" -C "$4" --strip-components=1
  fi
}
fetch vulkan-headers "https://github.com/KhronosGroup/Vulkan-Headers/archive/$VULKAN_HEADERS_SHA.tar.gz" "$VULKAN_HEADERS_SHA256" "$work/vulkan-headers"
fetch spirv-headers "https://github.com/KhronosGroup/SPIRV-Headers/archive/$SPIRV_HEADERS_SHA.tar.gz" "$SPIRV_HEADERS_SHA256" "$work/spirv-headers"
if [ ! -f "$work/spirv-install/share/cmake/SPIRV-Headers/SPIRV-HeadersConfig.cmake" ]; then
  cmake -S "$work/spirv-headers" -B "$work/spirv-build" -DCMAKE_INSTALL_PREFIX="$work/spirv-install" >/dev/null
  cmake --install "$work/spirv-build" >/dev/null
fi

TC="$(echo "$NDK"/toolchains/llvm/prebuilt/*)"
GLSLC="$(echo "$NDK"/shader-tools/*/glslc)"
export ANDROID_NDK="$NDK" ANDROID_NDK_HOME="$NDK"
export CC_aarch64_linux_android="$TC/bin/aarch64-linux-android30-clang"
export CXX_aarch64_linux_android="$TC/bin/aarch64-linux-android30-clang++"
export AR_aarch64_linux_android="$TC/bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/bin/aarch64-linux-android30-clang"
export PATH="$HOME/.cargo/bin:$TC/bin:$PATH"
export VULKAN_INCLUDE_DIR="$work/vulkan-headers/include"
export SPIRV_HEADERS_DIR="$work/spirv-install/share/cmake/SPIRV-Headers"
export SPIRV_HEADERS_INCLUDE_DIR="$work/spirv-install/include"
export VULKAN_GLSLC="$GLSLC"

cd "$(dirname "$0")/../core"
exec cargo "$@" --target aarch64-linux-android
