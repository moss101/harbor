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
cp "$app/macos/Runner/PrivacyInfo.xcprivacy" "$macos_app/Contents/Resources/"
codesign --force --sign - "$macos_app/Contents/Frameworks/libharbor_ffi.dylib"
codesign --force --sign - "$macos_app"
echo "   dylib bundled: Contents/Frameworks/libharbor_ffi.dylib"
echo "   privacy manifest bundled: Contents/Resources/PrivacyInfo.xcprivacy"

if [[ -n "${HARBOR_APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "-- signing macOS app with OPERATOR identity (keychain-provided) --"
  codesign --deep --force --options runtime \
    --sign "$HARBOR_APPLE_SIGNING_IDENTITY" "$macos_app"
  codesign --verify --strict "$macos_app"
else
  echo "-- signing: ad-hoc (default). Distributable signing requires the operator identity. --"
fi

# `|| echo NOTE` on an iOS build cannot tell "no toolchain here" from
# "the code does not compile" — it would report the second as a skip and
# let the script exit 0. Probe for the SDK instead, and when it is
# present require the build to succeed.
have_sdk() { xcrun --sdk "$1" --show-sdk-path >/dev/null 2>&1; }

echo "-- iOS simulator build (compilation proof; not a store artifact) --"
# Current Flutter rejects --release/--profile for simulators; debug proves
# compilation and is what the live simulator verification installs.
if have_sdk iphonesimulator; then
  flutter build ios --simulator --debug
else
  echo "NOTE: simulator build skipped — no iphonesimulator SDK on this machine"
fi

echo "-- iOS device static core (production embedding; install/launch needs hardware) --"
# The Runner target force-loads this archive via OTHER_LDFLAGS[sdk=iphoneos*].
# Build it BEFORE the xcodebuild step so the link input exists deterministically.
(cd "$repo/core" && PATH="$HOME/.cargo/bin:$PATH" cargo rustc --release \
  -p harbor_ffi --target aarch64-apple-ios --crate-type staticlib)
lipo -info "$repo/core/target/aarch64-apple-ios/release/libharbor_ffi.a"
# nm errors on LLVM-bitcode members (compiler_builtins); the Rust FFI symbols
# live in regular Mach-O members, so tolerate member-level errors.
symbols="$(nm -gU "$repo/core/target/aarch64-apple-ios/release/libharbor_ffi.a" 2>/dev/null || true)"
echo "$symbols" | grep "_harbor_core_open" >/dev/null \
  || { echo "FAIL: harbor_core_open missing from device archive"; exit 1; }
echo "   archive ready: core/target/aarch64-apple-ios/release/libharbor_ffi.a"
if have_sdk iphoneos; then
  flutter build ios --release --no-codesign
else
  echo "NOTE: device build skipped — no iphoneos SDK on this machine"
fi

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
