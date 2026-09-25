# Harbor — Store & Release Collateral

Final release materials for the Harbor v1.0.0 RC (goal §17). Every document
here is written against the qualified behavior of the build identified in
`evidence/releases/` — no capability may be described here that is not
backed by a PASS gate or explicitly labeled as disabled/pending.

| File | Purpose |
| --- | --- |
| `product_description.md` | Store listing description (EN + AR) |
| `privacy_statement.md` | Privacy statement (EN + AR) |
| `local_first_explained.md` | Local-first explanation for store review and users |
| `supported_devices.md` | Supported-device matrix |
| `model_compatibility.md` | What "works with a model" means (no overclaiming) |
| `release_notes.md` | RC release notes (EN + AR) |
| `changelog.md` | Changelog from initial public commit |
| `faq.md` | Help/FAQ |
| `security.md` | Security contact and disclosure policy |
| `model_licenses.md` | Model-license presentation |
| `apple_export_compliance.md` | Analysis behind `ITSAppUsesNonExemptEncryption=false` |

## Bundle identifiers (enter these EXACTLY in the store records)

They are not the same string on every platform, which is legal and easy
to transcribe wrong; a mismatch is rejected at upload, on Apple after
notarization has already been paid for in time.

| Platform | Identifier | Source of truth |
| --- | --- | --- |
| iOS / iPadOS | `dev.harbor.harborApp` | `ios/Runner.xcodeproj` → `PRODUCT_BUNDLE_IDENTIFIER` |
| macOS | `dev.harbor.harborApp` | same project; `codesign -dv` reports it as `Identifier=` |
| Android | `dev.harbor.harbor_app` | `android/app/build.gradle.kts` → `applicationId` |

Note the difference: Apple uses `harborApp`, Android `harbor_app`. Read
them from the build outputs rather than from memory —
`/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" <App>/Info.plist`
and `apksigner verify --print-certs` / the APK's manifest.

Wording rules (from the release goal):

- Never claim "works with every Hugging Face model" — Harbor qualifies
  catalog packages and scores arbitrary GGUFs via Fit Score; that is the
  honest statement.
- Never claim "all processing is always offline" — Harbor is local-first
  with explicit, authorized, brokered egress for model acquisition, and the
  network behavior is independently verified.
- Sync and all other optional capabilities are disabled at RC and must be
  described as such, not as shipping features.
