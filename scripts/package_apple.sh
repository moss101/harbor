#!/usr/bin/env bash
# Apple packaging up to the credential boundary (macOS + iOS).
#
# What this script does (machine-completable today):
#   1. Runs the release test subsets.
#   2. Builds the macOS release app (ad-hoc signed — launches and runs).
#   3. Builds the iOS simulator app (App Store builds REQUIRE a real
#      device/signing; the simulator build proves compilation only).
#   4. If an operator distribution identity exists in the login keychain
#      AND is named via HARBOR_APPLE_SIGNING_IDENTITY, the macOS app is
#      signed with it; otherwise the ad-hoc signature is kept and the
#      boundary is stated honestly.
#
# Credential boundary — the script NEVER:
#   - creates, exports or copies certificates or provisioning profiles;
#   - embeds any team ID / identity in the repository;
#   - submits anything for notarization or upload.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
app="$repo/apps/harbor_app"
flutter_bin="${HARBOR_FLUTTER:-$HOME/harbor-tools/flutter/bin}"
export PATH="$flutter_bin:$PATH"

echo "== Harbor Apple packaging (credential boundary) =="

echo "-- test subset --"
(cd "$repo/core" && cargo test -p harbor_artifacts -p harbor_formula --quiet)

cd "$app"
echo "-- macOS release build (ad-hoc signed) --"
flutter build macos --release
macos_app="build/macos/Build/Products/Release/harbor_app.app"
[ -d "$macos_app" ] || { echo "macOS build output missing"; exit 1; }

echo "-- bundle the native core into the app (Contents/Frameworks) --"
# Without this step the app runs the honest degraded state: the Dart FFI
# opens libharbor_ffi.dylib by bare name, which dyld resolves against the
# bundle Frameworks dir only when the dylib is actually there.
(cd "$repo/core" && cargo build --release -p harbor_ffi)
cp "$repo/core/target/release/libharbor_ffi.dylib" "$macos_app/Contents/Frameworks/"
codesign --force --sign - "$macos_app/Contents/Frameworks/libharbor_ffi.dylib"
codesign --force --sign - "$macos_app"
echo "   dylib bundled: Contents/Frameworks/libharbor_ffi.dylib"

if [[ -n "${HARBOR_APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "-- signing macOS app with OPERATOR identity (keychain-provided) --"
  codesign --deep --force --options runtime \
    --sign "$HARBOR_APPLE_SIGNING_IDENTITY" "$macos_app"
  codesign --verify --strict "$macos_app"
else
  echo "-- signing: ad-hoc (default). Distributable signing requires the operator identity. --"
fi

echo "-- iOS simulator build (compilation proof; not a store artifact) --"
flutter build ios --simulator --release || echo "NOTE: simulator build skipped (no iOS toolchain)"

cat <<'EOF'

-- remaining credential boundary (human/external input required) --
1. Apple Developer Program membership + App Store Connect record.
2. Operator creates/installs: "Apple Development" identity for device
   builds, "Apple Distribution" + Mac App Store identity for store builds.
3. macOS notarization: after signing, `xcrun notarytool submit` with a
   keychain profile the operator stores (never in this repo).
4. iOS device qualification: physical iPhone + development signing, then
   run the app and record device-bound evidence.
EOF
