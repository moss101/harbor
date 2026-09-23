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
# Notarization (macOS):
xcrun notarytool store-credentials HARBOR_NOTARY --apple-id <id> --team-id <team>
xcrun notarytool submit build/macos/Build/Products/Release/harbor_app.zip --keychain-profile HARBOR_NOTARY --wait
xcrun stapler staple build/macos/Build/Products/Release/harbor_app.app
# TestFlight (iOS): Xcode → Organizer → upload, or altool.
```

### IOS-02 — physical iPhone

```bash
# Prereq: device attached + trusted; development signing in Xcode.
cd apps/harbor_app && flutter run --release -d <device-id>
# Execute the §4 steps 6–19 checklist (inference, artifact preview,
# durable run replay, background/foreground, Local Only, Arabic/RTL,
# VoiceOver, thermal) and record into evidence/device_qualification.json.
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
