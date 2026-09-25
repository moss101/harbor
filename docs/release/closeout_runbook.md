# Harbor 1.0.0-rc2 — close-out runbook for the eight blocked gates

Every gate below is blocked ONLY on operator-supplied resources whose
absence was verified on the qualification machine on 2026-09-14
(`evidence/device_qualification.json` →
`session_29_external_prerequisite_audit`). After each gate closes, re-run
the affected evidence and finish with the bundle re-assembly (bottom).

## Gate matrix

| Gate | Blocked on | Prerequisite to supply | Resume path |
| --- | --- | --- | --- |
| MAC-02 | Apple Developer identity + notarization | `HARBOR_APPLE_SIGNING_IDENTITY` + notarytool keychain profile | `scripts/package_apple.sh`, then `xcrun notarytool submit` |
| MAC-03 | Physical Mac device-tier evidence | second, minimum-spec Mac (PERF-01 machine can double) | launch + workspace + inference checklist into `evidence/device_qualification.json` |
| IOS-02 | Physical iPhone/iPad | device + development signing | `flutter run --release`, §4 steps 6–19 checklist |
| IOS-03 | Apple signing for iOS + TestFlight | same identity as MAC-02 | `scripts/package_apple.sh`, TestFlight upload |
| AND-03 | Physical Android device(s) | handset(s) + USB | install release APK via adb, rerun §10 checklist |
| AND-04 | Play Console + upload key | `HARBOR_ANDROID_KEYSTORE`, `HARBOR_ANDROID_KEYSTORE_PASS`, `HARBOR_ANDROID_KEY_ALIAS`, `HARBOR_ANDROID_KEY_PASS` | `scripts/package_android.sh`, internal-testing upload, store-artifact requal |
| WIN-01 | Windows machine | any Windows 10/11 x64 host | `scripts/build_windows.ps1` + `docs/release/windows_qualification.md` |
| PERF-01 | Minimum-spec Apple-silicon device | e.g. M1 8 GB host | performance protocol → freeze thresholds per class |

## Per-gate execution

### MAC-02 + IOS-03 — Apple identity, signing, notarization

```bash
# Prereq: Apple Developer Program membership; identity in the login
# keychain (Xcode → Settings → Accounts → Manage Certificates).
security find-identity -p codesigning -v   # must list the identity
export HARBOR_APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (<team>)"
scripts/package_apple.sh                   # signs macOS app + iOS archive
# Notarization (macOS). Paths are from the REPO ROOT, and the zip has to
# be made — notarytool takes an archive, and nothing above produces one.
app=apps/harbor_app/build/macos/Build/Products/Release/harbor_app.app
ditto -c -k --keepParent "$app" /tmp/harbor_app.zip
xcrun notarytool store-credentials HARBOR_NOTARY --apple-id <id> --team-id <team>
xcrun notarytool submit /tmp/harbor_app.zip --keychain-profile HARBOR_NOTARY --wait
xcrun stapler staple "$app"
# Staple the APP, not the zip, then prove it — this is exactly what the
# MAC-02 gate reads, so if these two disagree the gate wins:
xcrun stapler validate "$app"
codesign -d --entitlements - "$app" | grep -q app-sandbox   # entitlements survived signing
# TestFlight (iOS). Organizer lists .xcarchive files and altool takes an
# .ipa; the script's `flutter build ios --release --no-codesign` produces
# NEITHER, so with the identity exported above it now runs `flutter build
# ipa` and leaves the artifact here:
cd apps/harbor_app && flutter build ipa --export-method app-store
xcrun altool --upload-app -f build/ios/ipa/*.ipa -t ios \
  --apiKey <key-id> --apiIssuer <issuer-id>   # or Transporter.app
# Needs the App Store Connect record and a matching provisioning profile
# to exist first — without them the export fails rather than producing
# something unuploadable.
```

### IOS-02 — physical iPhone

**The simulator cannot stand in for this, and not only because it is not
a shipping target: its inference output is DEGENERATE.** Session 40 ran
`second-look` there on qwen2.5-1.5b-instruct-q4_k_m through the
`grammar_constrained` path and got multilingual token soup with a
repetition loop — schema-valid, semantically worthless — where the same
weights, schema and prompt produce a correct answer in 124 tokens on
macOS. A `model.structured` node on the simulator therefore "succeeds"
while returning nothing usable, and one that runs long is garbage
filling its array to the token cap.
So: never tick an inference, quality, eval or performance item from a
simulator run. The simulator is good for UI reachability, file pickers,
plumbing and crash-freedom, and for nothing that depends on what the
model actually said. Whether a physical device shares the fault is
UNKNOWN — the simulator has its own Metal path (SimMetalHost) and its
own ggml kernel build — and step 6 below is the first thing that will
tell us.

```bash
# Prereq: device attached + trusted; development signing in Xcode.
cd apps/harbor_app && flutter run --release -d <device-id>
# Execute the §4 steps 6–19 checklist (inference, artifact preview,
# durable run replay, background/foreground, Local Only, Arabic/RTL,
# VoiceOver, thermal) and record into evidence/device_qualification.json.
#
# Safe-commit on mobile: "Overwrite original" is deliberately NOT offered
# on iOS or Android — both pickers hand the app a COPY of the chosen file
# (UIDocumentPicker `.import`; SAF resolved via a cache copy), so an
# overwrite there would rewrite a temporary file and report success over
# a document the user still has unchanged. Confirm the option is absent
# and that "Save new copy" writes where the user chose. If you ever see
# Overwrite on a phone, that is the bug, not the fix.
#
# File pickers: open a file from EVERY entry point — Home attach, Work
# "Open a file", skill-run attach, Knowledge ingest, Models "Import
# GGUF" — and confirm each one actually presents the system picker AND
# that the file you want is selectable rather than greyed out. Do not
# tick this from one surface. On iOS a type group with no
# `uniformTypeIdentifiers` throws before any picker is built, so a dead
# entry point looks exactly like a user who changed their mind: no
# sheet, no error, no log line. All five were inert this way until
# session 40. `file_type_groups_test` guards the groups; only a device
# shows that the UTIs actually match real files.
```

### AND-03 — physical Android

```bash
adb install -r apps/harbor_app/build/app/outputs/flutter-apk/app-release.apk
# Launch, workspace + native-core check (remember: extractNativeLibs=false
# — no libharbor_ffi.so path in /proc/PID/maps is NOT a load failure),
# inference, lifecycle, TalkBack, network capture; record device-tier record.
```

### AND-04 — Play Console + store-signed AAB

```bash
# Prereq: Play Console account; upload key CREATED BY THE OPERATOR and
# registered in Play App Signing (the repo never generates or stores keys).
export HARBOR_ANDROID_KEYSTORE=/safe/path/upload.keystore
export HARBOR_ANDROID_KEYSTORE_PASS=...
export HARBOR_ANDROID_KEY_ALIAS=...
export HARBOR_ANDROID_KEY_PASS=...
scripts/package_android.sh                # store-signed AAB
# Upload to internal testing; install the STORE-DELIVERED artifact on a
# device and re-run the §10 reduced suite against it.
```

### WIN-01 — Windows qualification

```powershell
# On a Windows 10/11 x64 host with Rust + Flutter installed:
git clone https://github.com/moss101/harbor.git; cd harbor
core: cargo test --workspace
scripts/build_windows.ps1
# Execute docs/release/windows_qualification.md end to end (launch,
# workspace files, DPAPI keychain behavior, inference) and record.
```

### PERF-01 — minimum-spec device

Run `python3 tools/run_performance_qualification.py --write` on the
min-spec host with the device profile recorded; freeze the per-class
thresholds from measurement (`fixtures/qualification/`).

## After all gates close

```bash
cd core && cargo test --workspace
cd ..
python3 tools/run_performance_qualification.py --write
python3 tools/generate_gate_evidence.py --write
python3 tools/validate_dossier.py --write
python3 tools/assemble_release_evidence.py --version 1.0.0-rc2 --write
python3 tools/check_ring_gate.py \
    --report evidence/releases/1.0.0-rc2/release_gate_report.json \
    --ring 1 --platforms mac,ios,android
```

Run the assembly on the qualification machine and WITHOUT `--partial`;
the release workflow passes `--partial` because a CI runner cannot
produce the network capture or the performance run, and a partial bundle
cannot decide a ring (`docs/release/rings.md`).

`release_declared` is not flipped by hand — the assembler computes it
from the table (no `FAIL*`, no `BLOCKED_*`, complete bundle), so a
platform Harbor is not shipping has to be recorded `N/A_PLATFORM` by its
gate rather than left blocked. `check_ring_gate.py` prints each ring
clause with the gate ids that violate it and exits non-zero on NO-GO;
`--ring ga` adds the three clauses only the operator can answer
(`--confirmed crashes,checklist,evals`).

## Machine-local facts the operator will need

- Dossier seal: any commit touching tracked inputs requires
  `python3 tools/validate_dossier.py --write` IN THE SAME COMMIT.
- Real-network tests are qualification-machine-only (HF resets runner IPs).
- Never run two flutter commands concurrently (startup lock).
- Flutter/Dart live at `~/harbor-tools/flutter/bin/` (not on PATH);
  cross-compiles need the rustup shims + NDK env (docs/STATUS.md).
- macOS ad-hoc rebuilds need one keychain Allow (or delete the
  `dev.harbor.core` item) — signed releases are unaffected.
