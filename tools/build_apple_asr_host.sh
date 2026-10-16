#!/bin/zsh
# Build the Apple on-device speech adapter (decision 0019) as a dylib the
# qualification tests dlopen; the same source compiles into the macOS/iOS
# Runner (iOS 26+ / macOS 26+ SpeechTranscriber).
set -euo pipefail
repo_root="${0:A:h:h}"
src="$repo_root/native/apple/asr_host/ASRHost.swift"
out_dir="$repo_root/native/apple/asr_host/build"
mkdir -p "$out_dir"
swiftc -emit-library -target "$(uname -m)-apple-macos26.0" -sdk "$(xcrun --show-sdk-path)" \
  -framework Speech -framework AVFoundation -O "$src" -o "$out_dir/libharbor_asr_host.dylib"
sha=$(shasum -a 256 "$out_dir/libharbor_asr_host.dylib" | cut -d' ' -f1)
echo "{\"artifact\":\"libharbor_asr_host.dylib\",\"sha256\":\"$sha\",\"sdk\":\"$(xcrun --show-sdk-version)\"}" > "$out_dir/build_info.json"
echo "built $out_dir/libharbor_asr_host.dylib $sha"
