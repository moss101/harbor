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
| iOS / iPadOS | `com.cogniwiss.harborlm` | `ios/Runner.xcodeproj` → `PRODUCT_BUNDLE_IDENTIFIER` |
| Android | `com.cogniwiss.harborlm` | `android/app/build.gradle.kts` → `applicationId` |
| macOS | `dev.harbor.harborApp` | `macos/Runner/Configs/AppInfo.xcconfig` (not part of the 2026-11 mobile submission; unchanged) |

The mobile store identity (Harbor LM, Cogniwiss Labs) was set on
2026-11-03. The Android Kotlin `namespace` stays `dev.harbor.harbor_app`;
only `applicationId` is the store identity. Read identifiers from the build
outputs rather than from memory —
`/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" <App>/Info.plist`
and `apksigner verify --print-certs` / the APK's manifest.

## Privacy policy

Canonical text: `apps/harbor_app/assets/privacy/privacy_policy.md` (shown
in the app under Settings → Privacy Policy). Public URL for both store
records: **https://harbor-lm.pages.dev/privacy** (Cloudflare Pages project
`harbor-lm`), rebuilt from the same file:

    python3 tools/build_privacy_site.py
    npx wrangler pages deploy build/privacy_site --project-name harbor-lm --branch main

`privacy_statement.md` below is the superseded RC 1.0 statement; do not
paste it into a store record.

Wording rules (from the release goal):

- Never claim "works with every Hugging Face model" — Harbor qualifies
  catalog packages and scores arbitrary GGUFs via Fit Score; that is the
  honest statement.
- Never claim "all processing is always offline" — Harbor is local-first
  with explicit, authorized, brokered egress for model acquisition, and the
  network behavior is independently verified.
- Sync and all other optional capabilities are disabled at RC and must be
  described as such, not as shipping features.
