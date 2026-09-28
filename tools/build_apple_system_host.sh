#!/bin/zsh
# Build the Apple system-model host adapter (HBR-033) as a dylib the
# qualification harness dlopens (core/harbor_core/tests/
# skill_evals_system.rs). The same source compiles into the macOS/iOS
# Runner for the app-side registration path (decision 0009).
#
# Records the SDK and toolchain it built with: the AFM contract is
# OS-provisioned, so evidence must name the SDK it was qualified on.
set -euo pipefail

repo_root="${0:A:h:h}"
src_dir="$repo_root/native/apple/system_host"
include_dir="$repo_root/native/apple/include"
out_dir="$src_dir/build"
out="$out_dir/libharbor_system_host.dylib"

sdk=$(xcrun --show-sdk-path)
sdk_version=$(xcrun --show-sdk-version)
swift_version=$(swift --version 2>&1 | head -1)

mkdir -p "$out_dir"
swiftc -emit-library \
  -target "$(uname -m)-apple-macos26.0" \
  -sdk "$sdk" \
  -framework FoundationModels \
  -I "$include_dir" \
  -O \
  "$src_dir/AFMHost.swift" \
  -o "$out"

sha=$(shasum -a 256 "$out" | cut -d' ' -f1)
cat > "$out_dir/build_info.json" <<EOF
{
  "artifact": "libharbor_system_host.dylib",
  "sha256": "$sha",
  "sdk": "$sdk_version",
  "swift": "$swift_version",
  "target": "$(uname -m)-apple-macos26.0",
  "source": "native/apple/system_host/AFMHost.swift"
}
EOF
echo "built $out"
echo "sha256 $sha"
cat "$out_dir/build_info.json"
