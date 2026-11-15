# Harbor LM 1.1.0 (2) — store readiness, 2026-11-03

Branch `runtime-resilience` @ 16385f7 plus the uncommitted changes of
2026-11-03 listed at the end. Publisher: Cogniwiss Labs. Privacy contact:
privacy@cogniwiss.com. Public privacy policy:
https://harbor-lm.pages.dev/privacy.

**Verdict for both stores: DO NOT SUBMIT.** The mandatory release rule is
triggered on both. Signing is impossible with what is on this machine, and
there is an open store-policy gap (in-app reporting of AI output) plus
unfixed functional defects. Nothing was uploaded to App Store Connect or
Play Console, and no review was requested.

PASS = verified today on this machine. FAIL = verified wrong or missing.
NOT VERIFIED = not run against this build. Earlier evidence that is bound
to older commits is quoted for context and is never counted as a PASS.

## Blockers (release rule)

| # | Store | Blocker | What clears it |
|---|---|---|---|
| B1 | Apple | No distribution signing. The only team in Xcode is "Mohsin Mustafa (Personal Team)" 4UJAPZ2U5D (free provisioning), and the only identity is "Apple Development". A free team cannot create an App Store Connect record, an Apple Distribution certificate or an App Store provisioning profile. | Enroll Cogniwiss Labs in the Apple Developer Program as an organization (needs a D-U-N-S number), add the account in Xcode, create the App ID `com.cogniwiss.harborlm` and the App Store Connect app record. |
| B2 | Google | No Play Console account or upload key on this machine (`HARBOR_ANDROID_*` unset, no `android/key.properties`). The AAB built today is debug-key signed and Play will reject it. | Create the Play Console developer account for Cogniwiss Labs. Generate an upload keystore, kept outside the repo. Enroll in Play App Signing. Run `scripts/package_android.sh` with the four env vars. |
| B3 | Both | No in-app way to report offensive AI output. Google's AI-Generated Content policy requires reporting "without needing to exit the app". Apple 1.2 now names "AI content creation apps" and asks for a reporting mechanism. Today the only route is the e-mail address in the privacy policy. | Product decision (see "Open decisions"). |
| B4 | Both | Functional defects from `docs/UPGRADE_PLAN_2026-10-28.md` are still open. The ones a reviewer can hit: C6 (30 s overall deadline on model downloads; a ~1 GB download on a phone connection may never finish), C14 (Import GGUF reads the whole file into RAM, so a phone can OOM), C2 (models whose end-of-turn token is EOG but not EOS run to max_tokens), A10 (Android back exits the app from every screen). | Fix and re-test on devices (WS-1). Run C6 first with a real >30 s download, because the plan itself marks it unverified. |
| B5 | Both | Store screenshots are from RC 1.0, carry the old name "Harbor", and come from an emulator or simulator. There are no iPad screenshots, although the app targets iPad (`TARGETED_DEVICE_FAMILY = "1,2"`). | Capture fresh screenshots on devices from this build. |
| B6 | Both | This build has not run on a physical iPhone or Android phone. | Run the device checklists in `docs/release/closeout_runbook.md` (IOS-02, AND-03). |

## Fixed today

- **Play 16 KB page size.** `libharbor_ffi.so` and `libharbor_worker.so`
  were linked with 4 KB LOAD alignment, which Play rejects for apps
  targeting Android 15+. Now `-z max-page-size=16384` is set in
  `core/.cargo/config.toml`, `scripts/build_android_core.sh` and
  `.github/workflows/release.yml`, and both the script and CI assert the
  alignment. Verified in today's AAB: every `.so` is 0x4000 or 0x10000.
  The release workflow also never rebuilt `libharbor_worker.so` and
  shipped the tracked binary; it builds it now.
- **Store identity.** The ID is `com.cogniwiss.harborlm` on iOS and
  Android (`applicationId`; the Kotlin namespace is unchanged), and the
  display name is "Harbor LM". macOS is unchanged and is not part of this
  submission.
- **Backups.** Android now sets `allowBackup=false` and
  `dataExtractionRules`, which exclude everything from both cloud backup
  and device transfer. Before, Auto Backup could copy the encrypted
  databases to Google Drive, and a restored copy could never be opened
  because the Keystore key is not exportable. On iOS, `harbor-data` is
  excluded from iCloud and Finder backup (`AppDelegate.swift`).
- **Stop did not stop (C1).** `op.start_generate` and `op.start_ingest`
  passed a fresh never-set flag instead of the op's cancel flag. They now
  pass `progress.cancel`. The code compiles; the fix has not been run
  against a real model.
- **Failed model load (C7).** A failed chat-model load no longer leaves
  the model recorded as loaded. Regression test:
  `core/harbor_ffi/tests/chat_failed_load.rs`, which passes.
- **Stale core in iOS device builds.** The "Stage Harbor Native Archive"
  phase declared no outputs and ran after Sources, so Xcode linked
  `Runner` before cargo had finished rebuilding `libharbor_ffi.a`. The
  first device build after a Rust change shipped the previous core: the
  binary linked at 05:49:15, the archive was finished at 05:50:25, and the
  old user-agent was in the binary. The phase now runs first and declares
  the archive as its output. Re-proved with a stale core: the archive was
  finished at 05:52:28, Runner linked at 05:52:33, and the new code was
  present.
- **User agent.** Model requests used to send
  `harbor-modelhub/0.1 (+https://github.com/mohsin/harbor)`, a wrong
  personal URL. They now send `HarborLM (+https://harbor-lm.pages.dev/privacy)`.
- **Privacy policy.** Written from a code audit and published (Phase 2,
  below). It is shown in the app under Settings → About → Privacy Policy.
  The old `privacy_statement.md` is marked superseded; it was inaccurate.

## Apple App Store

| Area | Check | Result | Evidence / note |
|---|---|---|---|
| Technical | iOS release build compiles (unsigned) | PASS | `flutter build ios --release --no-codesign` → Runner.app 61.3 MB, `com.cogniwiss.harborlm`, "Harbor LM", PrivacyInfo.xcprivacy and the policy asset bundled. Not an IPA: exporting one needs B1 |
| Technical | Runs on a physical iPhone/iPad (this build) | NOT VERIFIED | Earlier device tiers (1261/1261 formula cases; multimodal 7/8) are bound to older commits |
| Technical | Known defects a reviewer can hit (B4) | FAIL | C6, C14, C2 open |
| Technical | Generation cancel | NOT VERIFIED | Fixed in code today, not run on a device |
| Technical | Voice input | NOT VERIFIED | On-device SpeechAnalyzer, iOS 26+ only. On iOS 15–25 it reports unsupported; check that the UI says so clearly |
| Privacy | Policy URL public, no auth | PASS | `curl` → 308 → `/privacy/` 200, CSP headers set |
| Privacy | Policy in the app, easy to reach | PASS | Settings → About → Privacy Policy; `test/privacy_policy_test.dart` |
| Privacy | Same content in both places | PASS | The page is generated from the app asset (`tools/build_privacy_site.py`) |
| Privacy | Policy matches behaviour | PASS (code audit) | Network: only user-initiated HF search, file list and download (`harbor_modelhub`, `harbor_net`). No analytics, ads, crash upload or accounts. All declared permissions are used |
| Privacy | App Privacy label | Recommend **Data Not Collected** | Nothing reaches Cogniwiss. Search words and model ids go to Hugging Face at the user's request, and the policy discloses this |
| Privacy | PrivacyInfo.xcprivacy | NOT VERIFIED | FileTimestamp and DiskSpace declared. App Store Connect validation (needs B1) is the real check; it may also ask for SystemBootTime or UserDefaults reasons from linked code |
| Privacy | Usage strings | PASS | Microphone and camera strings are accurate. There is no NSSpeechRecognitionUsageDescription, and SpeechAnalyzer with on-device assets has not been shown to need one: NOT VERIFIED on a device |
| Compliance | 1.2 report mechanism for AI content | FAIL | B3 |
| Compliance | 2.5.2 downloaded code | PASS | Models are data (GGUF weights), not executable code |
| Compliance | 4.2.3(ii) download disclosed before first use | PASS | The install dialog quotes the size; free space is checked against the size plus 1 GiB before the first byte |
| Compliance | 5.1.1(v) account deletion | N/A | No accounts |
| Compliance | Payments | N/A | No IAP, subscriptions or ads |
| Compliance | Export compliance | PASS (existing analysis) | `ITSAppUsesNonExemptEncryption=false`; see `store/apple_export_compliance.md` |
| Compliance | Age rating questionnaire | TO ANSWER | Model output is unfiltered, so answer the AI and chatbot questions honestly. Expect 13+ or higher |
| Compliance | Reviewer access | TO WRITE | Review notes must say a model has to be downloaded first (Models → Recommended → Qwen2.5 1.5B, about 1.1 GB, Wi-Fi) and that voice needs iOS 26 |
| Licensing | Catalog model licenses | PASS | Qwen2.5-1.5B Apache-2.0, bge-small MIT, multilingual-e5-small MIT, bge-m3 MIT, EmbeddingGemma 2 Apache-2.0 (checked on huggingface.co/api/models/google/embeddinggemma-2: apache-2.0, ungated) |
| Metadata | Screenshots / description | FAIL | B5. `product_description.md` is RC 1.0 and says "Harbor"; rename it and drop the "audit log you can inspect" and "Network Log" claims, because that UI does not exist |
| Signing | Team, certificate, profile, IPA, upload | FAIL | B1 |

## Google Play

| Area | Check | Result | Evidence / note |
|---|---|---|---|
| Technical | Flutter test suite | PASS | `flutter test` 213 passed; `flutter analyze` clean; `cargo test -p harbor_net` and the new `chat_failed_load` test pass |
| Technical | Release AAB builds | PASS | `flutter build appbundle --release --target-platform android-arm64` → 49.4 MB, arm64-v8a only. Final build sha256 prefix f3b5e4680724759d; debug-key signed, so it cannot be uploaded (B2) |
| Technical | 16 KB page alignment | PASS | Fixed today (see above) |
| Technical | Target SDK | PASS | targetSdk 36 (Flutter 3.47.4 default) meets the new-app requirement of API 36 by 31 Aug 2026; minSdk 24 |
| Technical | Runs on a physical phone (this build) | NOT VERIFIED | Earlier OnePlus CPH2767 tier is bound to older commits |
| Technical | Known defects (B4) | FAIL | Includes A10 (back button exits) |
| Privacy | Public policy URL | PASS | https://harbor-lm.pages.dev/privacy |
| Privacy | Data safety form | Recommend: no data shared. For data collected, the conservative answer is **App activity → Other user-generated content / In-app search history: collected, not shared, optional, app functionality, not stored by the developer**, because model search words leave the device through the app. "No data collected" is arguable, since the developer receives nothing; decide this before submitting | Encrypted in transit: yes (TLS) |
| Privacy | Backups | PASS | Fixed today |
| Permissions | INTERNET, RECORD_AUDIO only; no background recording | PASS | Recording starts only on Record; no foreground service; CAMERA uses the system intent |
| Compliance | AI-Generated Content: in-app reporting | FAIL | B3 |
| Compliance | Billing, account deletion | N/A | None |
| Compliance | App content declarations | TO ANSWER | Ads: no. Target audience: 13+ or 18+ (not children). Content rating questionnaire: AI chatbot, unfiltered |
| Compliance | Closed-testing rule | NOT VERIFIED | A personal developer account created after Nov 2023 must run a closed test with 12+ testers for 14 days before production. An organization account is exempt |
| Metadata | Screenshots / description | FAIL | B5 |
| Signing | Upload key, Play App Signing, upload | FAIL | B2 |

## Phase 5 — on-device performance

Nothing was measured on a phone today, so every Phase 5 item is NOT
VERIFIED for this build. For context only: on the M5 Pro reference Mac
(Qwen2.5-1.5B Q4_K_M), load was 153 ms p50, TTFT 53 ms p50 and 148 tok/s.
The minimum-spec device classes are still `BLOCKED_DEVICE_EVIDENCE`. Store
listings must not quote speed numbers. The iOS **simulator's** Metal path
produces garbage output (closeout runbook, IOS-02), so a default simulator
run must never be used for any inference claim.

Suitable devices (claim only this): iPhone and iPad on iOS 15+ with A12 or
newer, and arm64 Android 7.0+ (minSdk 24). Fit Score gates which models a
device is offered. Voice input needs iOS 26+ or Android 13+ with on-device
recognition.

## Phase 6 — remaining security and privacy risks

- On iOS, picker copies (tmp/Inbox) and Android cache copies of opened
  files are never deleted by the app.
- Saved, converted and exported files are plaintext in Documents, which
  the iOS Files app can see. The policy discloses this.
- The `title` columns of memory and knowledge sources are plaintext (C4),
  and the network audit table stores request paths, including search
  words, in plaintext. The policy discloses the network log.
- Hugging Face search installs have no pinned hash. They are trusted on
  first download, which the policy discloses.
- llama.cpp's default logger writes model-load information to stderr
  (`llama_log_set` is never called). It contains no prompt text at
  default verbosity and is not persisted.
- There is no in-app "delete all data"; uninstalling is the only full
  wipe, and activity history (which holds questions) cannot be deleted
  item by item. The policy says so.
- `cargo audit` (2026-11-03) found 7 advisories. **Fixed:** rustls-webpki
  0.103.15 → 0.103.16 (RUSTSEC-2026-0382, distrusted intermediates) and
  webpki-roots 1.0.9 → 1.0.11 (RUSTSEC-2026-0381). Both are on the TLS
  path that model downloads use. Also fixed: ctutils 0.4.2 → 0.4.8
  (RUSTSEC-2026-0385). **Accepted:** RUSTSEC-2026-0429 (zip `extract()`
  abort), with the same reasoning as the three accepted zip advisories:
  `git grep '.extract('` finds no non-test call. Recorded in
  `core/deny.toml` and `core/.cargo/audit.toml`. `cargo audit` now exits 0.
  The Dart/pub side has no network dependencies; `flutter pub outdated`
  was not reviewed.

## Open decisions for the publisher

1. **Reporting AI output (B3).** Options:
   - (a) A "Report" action on each answer that opens a sheet. It sends a
     short report to a Cogniwiss endpoint (for example a Cloudflare
     Worker) only when the user presses Send. This adds a disclosed,
     user-initiated network flow, and the policy and Data safety form
     must change.
   - (b) A "Report" sheet that composes the report and offers copy or
     share to privacy@cogniwiss.com. This needs no network, but the user
     leaves the app to send it, which may not satisfy Google's wording.
2. **Data safety wording** for model search terms (see the table above).
3. **Developer accounts** for Cogniwiss Labs on both stores (B1, B2).

## Changes made on 2026-11-03 (uncommitted)

`.github/workflows/release.yml`, `core/.cargo/config.toml`,
`scripts/build_android_core.sh` (16 KB); the jniLibs `.so` files (rebuilt);
`android/app/build.gradle.kts`, `AndroidManifest.xml`,
`res/xml/data_extraction_rules.xml`; `ios/Runner.xcodeproj/project.pbxproj`,
`Info.plist`, `AppDelegate.swift`; `core/harbor_ffi/src/{lib,knowledge}.rs`,
`core/harbor_ffi/tests/chat_failed_load.rs`, `core/harbor_net/src/transport.rs`;
`assets/privacy/privacy_policy.md`, `lib/surfaces/privacy_policy_page.dart`,
`settings_surface.dart`, the l10n files, `pubspec.yaml`, `core/Cargo.lock`, `core/deny.toml`, `core/.cargo/audit.toml`,
`test/privacy_policy_test.dart`; `tools/build_privacy_site.py`; the store
docs. The dossier seal (`tools/validate_dossier.py --write`) must be rerun in
the same commit.

## Update — 2026-11-03, second pass (after "accounts are ready")

Done and verified in this pass:

- **C6 (downloads):** streaming now uses a 60 s idle timeout instead of a
  30 s overall deadline (`harbor_net`), with tests. The full 1.04 GB
  Qwen2.5-1.5B download completed in the iOS simulator through the app,
  and its hash was verified.
- **C14 (import):** `install_from_path` now stream-hashes in 1 MiB chunks
  (`PackageInstaller::ingest_reader`), with tests.
- **C2 (stop):** generation stops on any end-of-generation token
  (`is_eog_token`), with a fixture test. A model where this matters (e.g.
  Gemma) was not run live.
- **A10 (Android back):** a back stack through visited surfaces; the app
  exits only from Home (`test/back_navigation_test.dart`). Not yet run on
  a phone.
- **New defect found and fixed:** every first install failed with
  "Install failed: No such file or directory (os error 2)". The free-space
  preflight probed the not-yet-created `models/` directory.
  `free_disk_bytes` now probes the nearest existing ancestor, with a
  regression test. Verified in the simulator.
- **Install errors now show the reason.** They used to show a bare
  "Install failed".
- **Branding:** the product name is "Harbor LM" in the wordmark and in 16
  English and 9 Arabic UI strings. The feature names (Harbor Lens, Harbor
  Sheet, Harbor Office Suite) are unchanged.
- **Required sign-in (Firebase Auth):**
  - Firebase project `harbor-lm` (Analytics off). Google and Apple
    providers are enabled. iOS and Android apps are registered, with the
    upload-key and debug-key SHA-1s.
  - The config files are gitignored (the repo is public).
  - iOS offers Sign in with Apple and Google; Android offers Google.
  - Settings → Account has Sign out and Delete account. Deletion asks the
    user to sign in again and revokes the Apple token.
  - Tests use a fake auth. Real sign-in was **not exercised** (it needs a
    real account and, on iOS, the paid team's provisioning).
- **Report endpoint:** a Cloudflare Worker, `harbor-lm-reports`, with D1
  storage, Firebase ID-token verification (RS256, aud/iss = harbor-lm),
  20 reports per user per hour and 180-day retention (cron). Live: health
  returns 200; requests without a valid token get 401.
  - In the app, a flag on each answer opens a Report sheet. The question
    is included only if the user leaves the switch on.
  - Tests: worker JWT tests and Flutter widget/HTTP tests.
- **Privacy policy:** rewritten for accounts, reports, Firebase,
  Cloudflare and transfers, and redeployed.
- **Apple:** App ID `com.cogniwiss.harborlm` registered under team
  3TGFX25HMF with Sign in with Apple. App Store Connect record "Harbor LM"
  created (Apple ID 6830021206, SKU harborlm-ios).
- **Android upload key** created at `~/.harbor-keys/harborlm-upload.jks`
  (passwords in `harborlm-upload.properties`, 0600, outside git).
  **BACK IT UP.**
- `flutter test`: 220 passed. `flutter analyze`: clean.
- **Partial screenshots:** `screenshots/harbor_lm/ios_6.9/01_home.png` and
  `03_models.png`. This was a simulator debug build with
  `HARBOR_SCREENSHOT_MODE` and the CPU-only model.

Still blocking, in order:

1. **Xcode does not have the Cogniwiss team.** Only the free Personal
   Team is listed, so there is still no distribution certificate. You
   need to add the Apple ID in Xcode → Settings → Accounts.
2. **Play Console app not created.** The form is filled in, but Google's
   declarations must be ticked by you (my click was blocked).
3. **EU trader status (DSA)** in App Store Connect is for the Account
   Holder to declare.
4. **Apple 5.1.1(v) risk:** a forced login for an on-device app with no
   account-based features may be rejected.
5. **Remaining screenshots:** iPhone chat, iPad 13", and Android phone.
6. **Physical devices:** this build has not run on a phone, and real
   sign-in has not been tested.
7. **Quality:** general questions are answered with "Based on the
   provided evidence…" and a NOT GROUNDED badge. The grounded-answer
   prompt is used even when no sources exist; it should switch to a
   plain chat prompt.
8. **Not done:** download resume; `dart`/pub dependency audit.
