#!/usr/bin/env bash
# Android packaging up to the credential boundary.
#
# What this script does (machine-completable today):
#   1. Runs the release test suites (fast subsets).
#   2. Builds the release AAB/APK. If operator signing credentials are
#      provided via environment variables, they are used for signing;
#      otherwise it falls back to the debug key and says so HONESTLY
#      (debug-key-signed artifacts are NOT store-distributable).
#   3. Prints exactly which credentials unlock the remaining boundary.
#
# Credential boundary — the script NEVER:
#   - generates, imports, copies or stores operator private keys;
#   - writes key material to the repository (env vars only, read here);
#   - uploads anything anywhere.
#
# Operator-provided credentials (all optional):
#   HARBOR_ANDROID_KEYSTORE      path to the operator's upload keystore
#   HARBOR_ANDROID_KEYSTORE_PASS keystore password
#   HARBOR_ANDROID_KEY_ALIAS     key alias
#   HARBOR_ANDROID_KEY_PASS      key password
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
app="$repo/apps/harbor_app"

echo "== Harbor Android packaging (credential boundary) =="

echo "-- test subset --"
(cd "$repo/core" && cargo test -p harbor_artifacts -p harbor_formula --quiet)

# apksigner from PATH or the newest build-tools that has one; the signer
# of an APK is the only thing that says which key was really used.
android_signer_dn() {
  local apk="$1" tool
  tool="$(command -v apksigner || true)"
  if [[ -z "$tool" ]]; then
    for root in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Library/Android/sdk"; do
      [[ -n "$root" ]] || continue
      tool="$(ls -1 "$root"/build-tools/*/apksigner 2>/dev/null | sort -r | head -1)"
      [[ -n "$tool" ]] && break
    done
  fi
  [[ -n "$tool" ]] || { echo ""; return; }
  "$tool" verify --print-certs "$apk" 2>/dev/null \
    | sed -n 's/^Signer #1 certificate DN: //p' | head -1
}

cd "$app"
if [[ -n "${HARBOR_ANDROID_KEYSTORE:-}" ]]; then
  echo "-- release build with OPERATOR signing (credentials via env; never stored) --"
  cat > android/key.properties <<EOF
storeFile=${HARBOR_ANDROID_KEYSTORE}
storePassword=${HARBOR_ANDROID_KEYSTORE_PASS}
keyAlias=${HARBOR_ANDROID_KEY_ALIAS}
keyPassword=${HARBOR_ANDROID_KEY_PASS}
EOF
  trap 'rm -f android/key.properties' EXIT
  flutter build appbundle --release
  flutter build apk --release
  # Announcing the signing is not the same as doing it. Before this,
  # build.gradle.kts never read key.properties and asked for the debug
  # signingConfig unconditionally, so this branch produced a debug-key
  # bundle while printing "OPERATOR signing" — rejected at Play upload.
  # Read the signer off the artifact.
  signer_dn="$(android_signer_dn "build/app/outputs/flutter-apk/app-release.apk")"
  echo "   APK signer: ${signer_dn:-<none>}"
  case "$signer_dn" in
    *"CN=Android Debug"*|"")
      echo "FAIL: operator credentials were supplied but the APK is not signed with them"
      exit 1 ;;
  esac
else
  echo "-- release build with DEBUG key (NOT store-distributable) --"
  flutter build appbundle --release
  flutter build apk --release
  signer_dn="$(android_signer_dn "build/app/outputs/flutter-apk/app-release.apk")"
  echo "   APK signer: ${signer_dn:-<none>}"
  case "$signer_dn" in
    *"CN=Android Debug"*) : ;;
    *) echo "FAIL: expected the debug key here, got: ${signer_dn:-<none>}"; exit 1 ;;
  esac
  echo "   artifacts: build/app/outputs/bundle/release/app-release.aab (debug-key signed)"
  echo "            : build/app/outputs/flutter-apk/app-release.apk (debug-key signed)"
  echo "   signing:   debug key — evidence value is reproducible-release only;"
  echo "              the store-ready AAB requires the operator upload key (env vars)"
fi

cat <<'EOF'

-- remaining credential boundary (human/external input required) --
1. Operator creates an Android upload key (Android Studio or keytool) and
   keeps it OUT of this repository.
2. Play Console account + app registration (Play App Signing keeps the
   release key server-side; only the upload key is needed locally).
3. Re-run this script with HARBOR_ANDROID_KEYSTORE* env vars set, then
   `flutter build appbundle --release` produces the store-ready .aab.
EOF
