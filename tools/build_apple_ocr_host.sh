#!/bin/zsh
# Build the Apple Vision OCR adapter (decision 0018) as a dylib the
# qualification tests dlopen. The same source compiles into the macOS/iOS
# Runner for the app-side registration path.
set -euo pipefail
repo_root="${0:A:h:h}"
src="$repo_root/native/apple/ocr_host/OCRHost.swift"
out_dir="$repo_root/native/apple/ocr_host/build"
mkdir -p "$out_dir"
swiftc -emit-library -target "$(uname -m)-apple-macos14.0" -sdk "$(xcrun --show-sdk-path)" \
  -framework PDFKit -framework Vision -framework ImageIO -framework CoreGraphics -O \
  "$src" -o "$out_dir/libharbor_ocr_host.dylib"
sha=$(shasum -a 256 "$out_dir/libharbor_ocr_host.dylib" | cut -d' ' -f1)
echo "{\"artifact\":\"libharbor_ocr_host.dylib\",\"sha256\":\"$sha\",\"sdk\":\"$(xcrun --show-sdk-version)\"}" > "$out_dir/build_info.json"
echo "built $out_dir/libharbor_ocr_host.dylib $sha"
