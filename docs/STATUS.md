# Harbor — project status

This file has ONE authoritative snapshot (below) followed by historical
session notes. The snapshot is rewritten each session; history is append-only
context. Evidence files under `evidence/` bind to the exact git commit they
ran at.

---

## Session 40 (2026-09-23): the release workflow runs end to end — and five things it produced were not what they claimed

- **Dry runs 3–7 green** (`gh workflow run release.yml`): `evidence`,
  `macos-app` and `android` all pass and `publish` correctly skips on a
  dispatch. Dry run 2's failure was the assembler's exit code, not a crash:
  it returns 1 on any `FAIL_NO_EVIDENCE`, and X-07 (real network capture)
  and X-08 (timed performance run) can never have evidence on a GitHub
  runner. Rather than weaken those gates, `--partial` — which the workflow
  always passes, because it always runs on a runner — leaves their statuses
  untouched, labels the bundle `bundle_completeness: "partial"`, lists
  `absent_machine_local`, says in the declaration reason that it cannot
  decide a ring, and tolerates ONLY those two absences. CI bundle: 13 PASS,
  4 BLOCKED_EXTERNAL, 4 BLOCKED_DEVICE_EVIDENCE, 2 N/A_DISABLED,
  X-07/X-08 FAIL_NO_EVIDENCE. `publish` is still the one untested job: it
  runs on tags only.
- **The macOS artifact had no native core.** Downloaded dry run 3's
  `macos-app` zip and opened it: `Contents/Frameworks` held App,
  FlutterMacOS and objective_c and no `libharbor_ffi.dylib`;
  `Contents/Resources` had no `PrivacyInfo.xcprivacy`. `flutter build
  macos` bundles neither — `scripts/package_apple.sh` does, on the operator
  machine, and the workflow never ran it. The artifact a ring-0 draft
  release would carry was not merely unsigned, it could not open a
  workspace. The job now does that script's bundling half and the zip step
  asserts both files.
- **Both mobile/desktop packages claimed hardware with no core.** The
  macOS executable was universal while the dylib is arm64 — an Intel Mac
  would launch Harbor and find nothing, with no store-side filter to stop
  it because macOS ships as a DMG. `ARCHS = arm64` now makes macOS decline
  to open it. On Android it took three attempts, each one caught by opening
  the next dry run's package: `lib/armeabi-v7a/` and `lib/x86_64/` carried
  the Dart and Flutter runtimes with no `libharbor_ffi.so`;
  `ndk { abiFilters }` did **not** fix it (the Flutter Gradle plugin
  overwrites the ABI list, dry run 4); `--target-platform android-arm64`
  removed Flutter's own libraries but left `libdartjni.so` from a
  dependency AAR in both, which still told Play those devices were
  supported (dry run 6); `packaging.jniLibs.excludes` drops the rest. The
  workflow now **asserts** the packaged ABI list of both the APK and the
  AAB (`base/lib/<abi>/`, which is what Play's splits come from) instead of
  relying on someone opening the artifact. The device matrix gained the
  Intel-macOS row it was missing, and the assets are named
  (`harbor_app-android-<version>-debugkey.{apk,aab}`).
- **Evidence that could not fail.** `evidence_ok` looked for a few boolean
  keys and, finding none, returned true because the JSON parsed — and not
  one of the four inspection files carries those keys. An inspection
  recording a violation, a broken hash chain or a failed verdict read as
  PASS. It now refuses on a non-empty `violations`, any false
  `*_ok`/`*_verified`, or a `verdict`/`result` that is not PASS/OK/N/A
  (verified by flipping both). The status now distinguishes `FAIL`
  (evidence says so) from `FAIL_NO_EVIDENCE` (there is none) — the ring
  rules count them separately — and `--partial` waves through only the
  second.
- **Suites that never ran counted as passes.** `generate_gate_evidence.py`
  looked for the toolchain only under `~/harbor-tools`, so on a runner it
  ran six suites instead of ten and still reported `all_suites_ok: true`;
  the app suite's live-core tests return early without the dylib, so they
  would have counted too. It now finds the toolchain there or on PATH,
  records `skipped_suites` with reasons, and the assembler treats a bundle
  with skipped suites as missing evidence. The evidence job installs
  Flutter, builds the debug core and runs pub get so nothing is skipped.
  The bundle's changelog copy was likewise a silent no-op (`|| true`,
  running before the directory existed) — every bundle so far shipped
  without it.
- **`release_declared` is computed, not asserted.** It was hardcoded
  `false`, so the ring-1 rule ("`release_declared` `true`") could never
  hold. It now reads the table (no `FAIL*`, no `BLOCKED_*`, complete
  bundle), which makes it a GA-level declaration — so a platform Harbor is
  not shipping has to be recorded `N/A_PLATFORM` rather than left blocked.
  The runbook no longer says to flip it by hand.
- **The ring rule is executable**: `tools/check_ring_gate.py` reads a gate
  report, prints every clause with the gate ids that violate it, and exits
  non-zero on NO-GO. Cross-platform gates count against every ring set;
  `--platforms` drops only platform gates; a partial bundle is refused.
  `--ring ga` adds the three clauses no report can answer and never returns
  GO until the operator names them (`--confirmed crashes,checklist,evals`).
  Against today's report: NO-GO, naming the seven operator blockers.
- **Four ways a run could hang silently, all closed.** (1) The worker
  isolate answered `close` with a bare `Isolate.exit()` and never replied,
  so every `HarborService.close()` burned the parent's full 5 s bounded
  wait — the app suite went from 2:18 to 0:24. (2) An isolate that goes
  away without an uncaught error signalled nothing, leaving in-flight
  requests pending for ever; `addOnExitListener` fails them now. (3) The
  four FFI op threads had no `catch_unwind`, so a panic left the op
  `running` for ever and the app's unbounded `op.status` poll with it —
  `spawn_op` gives every op a terminal state (two tests). (4) The run sheet
  showed a static card with no cancel; it now renders the core's own
  snapshot through `OpProgressCard`, with the cancel every other surface
  already had.
- **CI stall: a fake timer in the test, not a core hang.**
  `AutomatedTestWidgetsFlutterBinding.pump()` calls `elapse(duration)`
  only when given one; with no argument it flushes microtasks and leaves
  the fake clock where it is. `settleUntil` pumped with no argument. The
  service's poll loop is started from a tap handler, so the
  `Future.delayed(250ms)` it waits between `op.status` calls is a FAKE
  timer that, in that loop, could never fire. It got away with it because
  the run normally finishes before the first status reply is processed,
  so `_runOp` returns on its first pass and never reaches the delay; when
  the op is slower than that round trip — CPU contention, a loaded runner
  — the first poll returns `running`, the loop reaches the dead timer and
  waits out the 120 s budget. `settleUntil` now pumps 100 ms of fake time
  per iteration, and a test demonstrates the mechanism deterministically
  instead of resting on flake counts: a 250 ms timer scheduled in the
  test's zone survives a full second of real time under `runAsync` + bare
  `pump()`, and fires the moment fake time is elapsed. **A test-harness
  defect; the product was never implicated.**
- **How that took three tries, because it is the kind of mistake worth
  recording.** The `flutter` job failed on `1df79b8` (docs-only; app code
  identical to `6ad9b62`, which passed) — 126 s against a 120 s budget
  where the test takes 7.5 s. It would not reproduce in 15 runs, so the
  waits were changed to report the core's message, or the visible tree on
  a deadline. It recurred on `4627f44` showing `placeholder-fill |
  running`, and I concluded `Executor::start` never returned — inference
  from the UI, stated as fact. Then it reproduced here (iteration 3 of a
  loop running the FULL suite under contention; the earlier attempts ran
  that test alone with `--plain-name`, which is why they missed it), and
  stack samples through the hang showed the core *idle* — one
  `libharbor_ffi` frame in six consecutive samples. A thread blocked in
  the executor would have been in every one. Only then did reading
  flutter_test's binding give the actual answer. The step sink
  (`Host::step`) and the op watchdog were both built while the wrong
  diagnosis stood; they are kept because they are worth having, and are
  now documented as defensive rather than as mitigating a known hang.
- **The refusal works.** Dry run 8's evidence job failed exactly as it
  should: one of the ten suites failed (the widget-test stall again),
  `all_suites_ok` went false, X-01..X-04 and X-09 read `FAIL` — evidence
  exists and says so, not `FAIL_NO_EVIDENCE` — and the assembler refused
  the bundle. What it could not say was WHICH suite: the artifact upload
  had no `if: always()`, so the run that most needed its evidence read
  was the one that threw it away, and `generate_gate_evidence.py` kept
  only pass/fail counts although it had captured the output. Both fixed;
  a failing suite now carries `output_tail` and prints it.
- **Live skill-eval tier re-measured at HEAD: 13/18, unchanged.** The
  recorded number was bound to `b020beb54` (session 34) and the GA rule
  wants one for the candidate, not one inherited four sessions back —
  and the formula content gate (`87922ed`) had landed since, sitting
  directly in the path of two of the five failures. Re-run on the same
  pinned Qwen2.5-1.5B (sha `6a1a2eb6…`, 144 s): **13/18 at `14eb406e`,
  the same five failures in the same five skills** (`formula-audit` ×2,
  `doc-coauthoring`, `meeting-notes`, `second-look`). So the gate did not
  move them, which confirms decision 0006's attribution that they are
  model behaviour upstream of it, and the evidence file is now bound to
  HEAD. Recorded as measured; the ≥9/11 target in the plan is not what
  this is scored against and no number was adjusted toward it.
- **Six platform gates asserted PASS without looking at their evidence.**
  The X-* tier was made evidence-driven in an earlier session; the
  platform tier was not, and hardcoded its statuses. `IOS-01` — "iOS
  production-device native linkage" — read PASS while citing
  `core/target/aarch64-apple-ios/release/libharbor_ffi.a`, an archive
  that did not exist here. The honest table was **14 PASS / 1
  FAIL_NO_EVIDENCE**, not 15. MAC-01, MAC-04, IOS-01, IOS-04, AND-01 and
  AND-02 now go through `asserted()`: every cited path must exist, and an
  `evidence/*.json` must say the check passed. Making them check would
  have failed every release run — on a runner four of them cite files
  that never exist — so they joined `MACHINE_LOCAL_GATES`, where
  `--partial` names their absence instead of hiding it.
  Then the gap was **closed rather than just flagged**: the archive was
  rebuilt (3 min) and what IOS-01 claims was verified directly — all five
  FFI entry points (`harbor_core_open`, `open_ex`, `call`, `close`,
  `string_free`), 1099 llama.cpp symbols, force-load wired in the Xcode
  project. 15 PASS again, this time earned.
- **Two more statuses that were asserted, not derived.** (1) The four
  device-blocked gates (`IOS-02`, `AND-03`, `WIN-01`, `PERF-01`)
  hardcoded `BLOCKED`, so ring 0 could complete in reality — iPhone,
  handsets, Windows host, M1 8 GB all qualified — and the table would go
  on reporting them blocked until someone edited the source;
  `release_declared` could never become true. They derive from the
  device classes `perf_qualification.json` already names
  (`ios_arm64_physical`, `android_arm64_physical`, `windows_x64`,
  `minimum_spec_macos_arm64`), so they close by themselves. Verified
  both ways: today's evidence leaves all four blocked; removing two
  classes flips IOS-02 and PERF-01 to PASS (15 → 17) and leaves the
  others. (2) `signed_artifact_hashes.json` hardcoded its `signed`
  field, so it would have kept saying "ad-hoc (NOT store-distributable)"
  after the operator signed with a Developer ID — in a file that ships
  in the bundle AS the evidence of how the artifact is signed. It is now
  read from `codesign`, `apksigner` (APK) and `keytool` (AAB, which
  apksigner cannot verify). The values match what was asserted for the
  three signed artifacts, and correct the iOS one: `--no-codesign`
  output is **unsigned**, which the hardcoded string called ad-hoc.
- **MAC-02 can close now; IOS-03 and AND-04 honestly cannot.** Once
  signing became observable, the Developer-ID gate stopped needing to be
  a literal: signature authority, notarization and a stapled ticket are
  all properties of the artifact. It derives from `codesign` +
  `xcrun stapler validate`, and states why it is blocked
  ("signing is ad-hoc") instead of only that it is. Verified in four
  directions, including the one that matters — Developer ID signing
  **alone** does not close it, because the gate also requires
  notarization, and a missing `stapler` blocks rather than guesses.
  IOS-03 and AND-04 stay asserted on purpose and now say so: a
  TestFlight or Play upload happens store-side and leaves no local
  artifact to read, so they are operator-attested rather than
  pretending to be measured.
- **The CI bundle was overstating by four gates, and now says so.** The
  consequence of making the platform tier check its evidence shows up
  hardest on a runner: MAC-01, IOS-01, AND-01 and AND-02 cite files that
  exist only on the qualification machine or in a different job, and
  they were asserting `PASS` over them. Every partial bundle CI has ever
  produced claimed 13 PASS including four platform gates it had no
  evidence for whatsoever. A full runner simulation (machine-local
  evidence and build outputs both hidden) now reports **9 PASS with six
  absences named in `absent_machine_local`**, and still exits 0 under
  `--partial` — which is the property that had to hold, or every release
  run would fail.
- **iOS simulator run — and it found a user-facing bug.** Built the
  simulator dylib fresh first (the Xcode phase only builds it when
  missing, so a stale one ships silently), verified the app embedded
  *that* build rather than assuming — by checking for a string literal
  only today's FFI contains — then installed and drove it on an iPhone
  17 Pro. The app launches, the trust chip reads LOCAL, About says
  "Native core: Loaded", and Models → Recommended lists **3 catalog
  packages**, which means the core imported the bundled catalog,
  verified its signature against the pinned root key, persisted trust
  and served `catalog.list` over FFI. The device identity row proves it
  wrote to its store.
  The bug: Settings → About rendered **`value: '1.0.0'` as a hardcoded
  literal** on a 1.1.0+2 build — and disagreed with the diagnostics
  export in the same file, which correctly uses `harborAppVersion`. A
  ring-1 tester reporting a problem would have quoted the wrong build.
  Fixed, and `build_info_test` now asserts the About card reads
  build_info and contains no version literal; the test was checked
  against the old code to confirm it actually catches it.
  **This does not touch IOS-02**, which stays `BLOCKED_DEVICE_EVIDENCE`:
  simulator evidence is not promoted to the device tier, and the
  simulator loads a `.dylib` where a device force-loads the static
  archive — a different linkage path from the one IOS-01 claims.
  Also exercised on device geometry, both clean: **dark mode** (fresh
  install follows the system theme) and **Arabic/RTL**, which holds up
  including the subtle part — layout, nav and segmented controls all
  mirror (الرئيسية moves to the right, the composer's send arrow to the
  left), while `LOCAL ONLY`, `ON DEVICE`, `policy-2026.09` and the brand
  name stay LTR, which is exactly what the settings copy promises about
  identifiers keeping their own direction. No defects found there; worth
  recording as a result rather than leaving untested.
  One loose end, stated rather than explained away: a diagnostics count
  of **4 records** appeared once after I replaced the binary under a live
  container, and I could not reproduce it — a clean install and an
  update-style reinstall both record zero. Unidentified, not dismissed.
- **The macOS release build was sandboxed with no file or network
  access.** `Release.entitlements` has been the unmodified Flutter
  template — `com.apple.security.app-sandbox` alone — since the first
  commit, never revisited as the app gained documents and model
  acquisition. Verified on the built artifact with
  `codesign -d --entitlements`, not just the source plist. Under that
  sandbox Powerbox does not extend access to a user-picked file
  (Attach a document, Save new copy, Overwrite original) and no outbound
  connection is permitted (model acquisition) — every core workflow. The
  macOS tier evidence records "launch verified" and "Metal inference
  verified", both of which pass sandboxed, so nothing ever exercised the
  paths that do not.
  Added the least-privilege pair — `files.user-selected.read-write` and
  `network.client`, no server entitlement — and aligned DebugProfile,
  since a debug build that can open files while the release build cannot
  is the arrangement where the gap only shows up in the build nobody
  tests. Rebuilt and re-read from the artifact to confirm.
  **Not empirically demonstrated**: proving the failure needs GUI
  interaction on a signed build, which is MAC-03's clean-machine run.
  The alternative fix is dropping the sandbox entirely, which is
  defensible for the Developer-ID DMG that `rings.md` describes — that
  is a distribution decision, and the sandbox was kept because it is the
  better default for this product and preserves the Mac App Store
  option.
  The other two platforms were checked rather than assumed to share it,
  and both are correct: Android declares `INTERNET` in the **main**
  manifest (Flutter's template puts it only in debug/profile, so that
  was already fixed deliberately) and reaches files through the Storage
  Access Framework, which needs no permission; iOS allows outbound
  connections by default, HTTPS satisfies ATS, and `UIDocumentPicker`
  needs no usage-description string. macOS was the only platform where
  the sandbox had to be told.
- **And the entitlements were then thrown away by the signing — including
  by a step I added today.** Dry run 11 was dispatched to confirm the
  new entitlements survived a CI build; the artifact came back with
  **none at all**, not even `app-sandbox`. `codesign --force --sign -`
  without `--entitlements` REPLACES the signature and discards them, and
  that is exactly what the "bundle the native core" step (3a9c7ca, mine,
  today) did. `scripts/package_apple.sh` had the same defect twice over,
  pre-existing: on its ad-hoc re-sign AND on the operator's Developer ID
  signing — so the app the operator ships would have carried no
  entitlements whatever the plist said. Fixed in both, and both now read
  the entitlements back **off the bundle** after signing and fail if any
  of the three is missing, because what ships is whatever the last
  `codesign` wrote, not what the plist contains. Demonstrated locally in
  both directions, then confirmed on CI: dry run 12's artifact carries
  `app-sandbox`, `files.user-selected.read-write` and `network.client`,
  with the core and the privacy manifest still in place. Three dry runs
  to get one plist right — the first proved the config was wrong, the
  second that fixing the config was not enough, the third that it holds.
- **Android: supplying the operator upload key did nothing at all.**
  `package_android.sh` writes `android/key.properties` from the
  operator's env vars, prints "release build with OPERATOR signing", and
  builds — while `build.gradle.kts` never read that file and set
  `signingConfig = signingConfigs.getByName("debug")` unconditionally
  (the Flutter template's TODO, untouched). So the operator would follow
  the runbook, hand over a keystore and password, get an artifact the
  script calls operator-signed, and be **rejected at Play upload**. The
  password was written to disk for no benefit whatsoever.
  `key.properties` is now loaded into a real `release` signingConfig,
  used when present and falling back to the debug key when absent so
  `flutter run --release` still works, and the script reads the signer
  **off the built APK** with `apksigner` and fails if the operator branch
  produced a debug-signed artifact. Proven both ways with a throwaway
  keystore: with the file, `CN=Harbor Upload Test`; without it,
  `CN=Android Debug`. The test keystore was deleted;
  `android/key.properties` is gitignored, so credentials cannot be
  committed.
- **Two more defects in code nobody could run**, found by following the
  same thread — everything behind a `BLOCKED_*` gate is unexecuted, so
  it is also untested. **Windows**: neither the CMake nor
  `build_windows.ps1` built or copied `harbor_ffi.dll`, so
  `flutter build windows --release` would have produced an exe with no
  core beside it — the macOS bug again, on the platform nobody here can
  run. The script now builds it, copies it and checks the packaged
  folder; the runbook's launch check ("window opens with the Model Dock
  visible") passed happily with no core, so it now requires the trust
  chip to read LOCAL and About to say *Native core: Loaded*. **Unexecuted
  — no Windows host and no PowerShell here to parse it.**
  **Notarization**: the MAC-02 runbook told the operator to
  `notarytool submit .../harbor_app.zip`, and nothing in the preceding
  steps creates that zip — `package_apple.sh` signs the `.app` and stops
  — with the path written relative to the wrong directory besides. It
  would have failed on the first attempt, with credentials in hand. Now
  it makes the archive with `ditto`, uses repo-root paths, staples the
  app rather than the zip, and ends by proving it with
  `stapler validate` plus an entitlements read — the two things the
  MAC-02 gate itself checks, so the runbook and the gate cannot drift.
- **IOS-03 had nothing to upload.** The runbook said "TestFlight (iOS):
  Xcode → Organizer → upload, or altool", and the line above it described
  `package_apple.sh` as producing an "iOS archive". It does not: it runs
  `flutter build ios --release --no-codesign`, whose output is an
  UNSIGNED `Runner.app` — neither an `.xcarchive` that Organizer can
  list nor an `.ipa` that altool accepts. The operator reaches the
  upload step with no artifact in existence. The script now says plainly
  that the device build is compilation proof, and when the signing
  identity is present runs `flutter build ipa --export-method app-store`
  — which archives *and* exports — failing loudly if no `.ipa` appears;
  the runbook points at that path instead of at Organizer.
  **Reasoned, not executed**: producing an `.ipa` needs an App Store
  Connect record and a matching provisioning profile, neither of which
  exists here. `flutter build ipa` was confirmed present in the pinned
  Flutter and both scripts parse; the rest waits on IOS-03 itself.
- **The derivation I built rested on a constant that could never
  change.** Making the device gates derive from
  `perf_qualification.json`'s `blocked_device_evidence` (5d80e7e) was
  only half a fix: that field was emitted from a hardcoded module-level
  list, verbatim, on every run. The operator could run the tool **on**
  the minimum-spec Mac and the report would still say the class was
  blocked for want of hardware, so the gates still could not close — the
  exact defect I thought I had removed, one layer down.
  A class is blocked now unless `evidence/devices/<class>.json` exists,
  and only `--measured-class` writes it, recording the commit, the perf
  run hash and the host's model, CPU and memory. Verified end to end:
  baseline blocks all four and PERF-01 reads BLOCKED; recording the
  minimum-spec measurement drops that class and PERF-01 **closes**
  (15 → 16 PASS) while the other three stay blocked. The simulated
  measurement was then deleted — and it is worth noting what it had
  recorded: `Mac17,8 / 24 GiB`, this reference machine, not a minimum-spec
  one. There is no min-spec device manifest to check against, so the
  host facts are the audit trail rather than a guard: a class attested
  from the wrong machine is visible in the evidence.
- **Arabic UI: English skill prose rendered with its punctuation on the
  wrong side.** Answering "are graphs implemented?" meant checking the
  claim that the 21 non-graph skills are *labelled* as declarations, so I
  opened the Skills surface on the simulator. The labelling is right —
  "30 مهارة", `إعلان فقط` on prose skills, `مخطط قابل للتشغيل` on graph
  ones — but the skill titles and descriptions are English-only data from
  `builtin_skills.json` that nothing routes through l10n, and they were
  rendered with the ambient direction. In Arabic that makes them RTL
  paragraphs, so each sentence's full stop moved to the left edge:
  `.information from DOCX, PDF and text documents`. Four `Text` widgets
  now carry `textDirection: TextDirection.ltr`, the same treatment
  identifiers already had. Rebuilt and confirmed on device: the stops sit
  at the end, English prose left-aligns inside the RTL card, and the
  Arabic chrome is untouched. Only visible by running the app in Arabic —
  the RTL widget tests assert semantics and overflow, not bidi placement.
  The Run sheet renders the same prose and had the same bug; that is five
  sites in total. Its **title** was deliberately left alone: it is
  interpolated into a localized Arabic template, where an embedded LTR
  run is precisely the case bidi already handles, and forcing the
  paragraph LTR would flip the Arabic around it. Models was checked in
  Arabic too and is clean — package ids, repo paths and licences are
  short values rather than sentences, so nothing trails a full stop.
  Error banners were considered and deliberately left alone:
  `HarborBanner`'s body carries localized Arabic strings as well as
  English core errors, so a blanket direction there would break the
  Arabic ones — and the core's messages do not end in full stops (Rust
  convention, verified by grep), so the bug cannot reach them. The rule
  stays "content with its own direction", applied per call site.
  Swept **all nine product surfaces** in Arabic on the simulator — Home,
  Ask, Work, Agents, Skills, Knowledge, Models, Activity, Settings. One bug, the skill prose, now fixed in five places;
  everything else correct, including the details that are easy to get
  wrong: navigation hints mirror their arrows (`النماذج ← المثبتة`),
  `Hugging Face` and the file-type chips stay LTR, Arabic sentences put
  their own full stops on the left where they belong, and the composer's
  ellipsis sits at the correct end. Running a skill or opening a
  document needs an installed model (a 1.1 GB brokered download) or a
  file in the simulator's storage, so those paths stay unexercised here.
  Agents is worth noting as a positive: it renders an explicit "not
  enabled in this release" banner pointing at Skills, rather than hiding
  the surface or pretending it works — the UI agreeing with `OPT-01`
  and `SYNC-01` reading `N/A_DISABLED` in the gate table.
- **"Overwrite original" would have lied on both phones.** Chasing what
  else IOS-02 covers: the commit sets `destination = pickedFile.path`,
  which is correct on desktop. It is not on mobile.
  `file_selector_ios` presents `UIDocumentPickerViewController(... in:
  .import)`, which copies the selection into the app's temporary
  directory, and `file_selector_android` resolves the SAF URI through
  `getPathFromCopyOfFileFromUri` — both hand back a **copy**. So an
  overwrite on a phone would rewrite a temp file, the commit would
  succeed, the receipt would verify, the UI would report the document
  overwritten, and the user's file would be untouched. For a product
  whose safe-commit exists to make writes honest and verified, that is
  the worst possible failure: a confident false claim about someone's
  document.
  The option is now offered only where the picker returns the user's
  real file, with the reasoning in the source. Save new copy is
  unaffected — it writes where the user chose. The device checklist says
  to confirm the option is **absent** on a phone, since it was the kind
  of item a tester would have ticked by seeing the button work.
  Read from the plugins' source rather than assumed; not demonstrated on
  a device, which is IOS-02 and AND-03.
- **…and "Save new copy" put the result where nobody could reach it.**
  The other half of safe-commit on mobile. The save path already knew
  about copy semantics — its comment says "mobile pickers hand out cached
  copies" — and lands the output in the app's documents directory,
  because neither mobile plugin implements a save picker. But on iOS that
  directory is invisible without `UIFileSharingEnabled`, which was
  absent: the save succeeded, the UI reported the path, and the file
  could not be opened, shared or found by anyone. The core workflow's
  output was effectively discarded. Both that key and
  `LSSupportsOpeningDocumentsInPlace` are set now — safe, because
  Harbor's own data lives in Application Support and the only thing in
  Documents is a copy the user deliberately saved.
  **Android has the same problem and no one-line fix**: app-private
  storage is invisible under scoped storage, and solving it needs a SAF
  create-document channel or a share sheet. Left untouched and flagged,
  because that is a product decision rather than a bug fix.
- **Gates** — `cargo fmt/clippy/test --workspace` green (288 tests),
  gguf-backend 20, `flutter test` 37/37 (app), harbor_native 1/1, dossier
  validator PASS, gate evidence 10/10 suites with `skipped_suites: []`,
  release gate report 15 PASS / 0 FAIL / 8 BLOCKED_* on this machine.

## Session 39 (2026-09-20): Phase C3 floor proxy, Fit Score refusal, release-workflow fixes

- **C3** — `perf_baseline` gained `HARBOR_PERF_BALLAST_GB=<n>`: the
  reference Mac holds n GiB of touched ballast for the whole run as a proxy
  for a smaller device, writing `evidence/perf_baseline_memory_pressure.json`
  (never the reference baseline). Under 14 GiB ballast on the 24 GB
  reference machine: ttft p95 **72 ms** (reference 55; threshold max 100),
  tokens/s p50 **173** (reference 179; floor 100), warm load p95 130 ms,
  RAG 23.5k docs/min — inside the frozen reference thresholds. This is a
  proxy, recorded as such; PERF-01 on the M1 8 GB machine still freezes
  the minimum-spec class. Models → Recommended now **refuses** install
  (button disabled, "Does not fit this device") when the core scores a
  package `too_large` or `unsupported`, instead of warning after download.
- **Release workflow** — first dry run reached the assembler and stopped:
  `assemble_release_evidence.py` loaded machine-local evidence
  unconditionally. It now records absent files as `ABSENT` with the reason
  (the gate table already says `FAIL_NO_EVIDENCE`), reads the app version
  from pubspec, and — a latent bug — gates X-06..X-09 had `status` and
  `evidence` swapped, so the report carried a file name where the ring
  go/no-go rules read a status. Local dry run: 15 PASS, 4
  BLOCKED_EXTERNAL, 4 BLOCKED_DEVICE_EVIDENCE, 2 N/A_DISABLED, 0 FAIL.
- **Gates** — app `flutter test` 32/32, `cargo test --workspace` 50 suites,
  dossier validator PASS.

## Session 38 (2026-09-20): Phase C4 security review + second fuzz finding; Phase D ring plan

- **Security review** (`docs/decisions/0007-security-review-1.1.md`,
  `evidence/security/review-1.1.json`): `/security-review` over
  `harbor-v1.0.0-rc2..HEAD` with an independent false-positive pass per
  candidate. Nothing reportable at ≥ 8/10; two candidates fixed anyway:
  a deterministic **formula content gate** (`check_formula_allowed`:
  qualified functions only, no external-workbook/UNC/URL/DDE syntax, sheet
  references must exist; applied in `formula.build_operations` and again in
  `apply_xlsx`), and **redaction of spaced filenames** in the diagnostics
  log (inspection seeds a spaced path). None open at "high".
- **Fuzzing, second finding** (from the CI `fuzz` job's first run): the
  upstream XLSX reader also aborts on malformed XML (an unclosed attribute
  in `[Content_Types].xml`). `inflate_probe` now requires every `.xml` /
  `.rels` part to be well-formed before any reader runs; regression input
  kept under `harbor_artifacts/tests/regressions/`.
- **CI** — `windows-core` compiles the modelhub test on Windows again (the
  read-only-root case is `cfg(unix)`).
- **Phase D** — `docs/release/rings.md`: ring 0/1/2 with go/no-go rules
  read from the gate report fields, the tester brief (diagnostics export as
  the feedback channel), the fix-only rule, staged Play rollout. Operator
  resources remain the prerequisite (`closeout_runbook.md`).
- **Release workflow** — dry run dispatched on `da12d95`
  (`gh workflow run release.yml`); result recorded in the next session note.
- **Gates** — `cargo fmt/clippy/test --workspace` (50 suites), replay 23/23,
  plaintext inspection PASS, dossier validator PASS.

## Session 37 (2026-09-20): Phase C2/C4/C5 — first run, fuzzing, release workflow

- **C2 first run and recovery** — Home shows a first-run card when no
  model is installed (Local Only stated, "Choose a model" → Models);
  Models → Recommended now lists the **signed catalog** offline: the app
  bundles `assets/catalog/{signed_catalog.json,root_public.hex}` (a test
  keeps them byte-identical to `fixtures/catalog`) and imports them on
  first open through `catalog.import`; `catalog.list` (new FFI) returns
  packages with tier/quantization/context/license and installed state;
  "Check size & fit" reads the repo listing through the broker and asks
  `model.fit_estimate` (new FFI; same device-profile code as installed
  packages); "Install" runs the real acquisition with the catalog's pinned
  sha256. Recovery: an acquisition that fails for any reason (transfer,
  local write, hash/size mismatch, validation, cancel) removes its staging
  directory; a local write failure aborts at once instead of the 5/15/30 s
  transport backoff; orphaned `.staging-*` residue is swept at open;
  listing paths that escape the staging directory are refused;
  `catalog.import` with a missing or malformed root key is a typed error
  (was a panic behind the boundary).
- **C4 fuzzing** — `core/fuzz` (cargo-fuzz, nightly, no sanitizer):
  `ffi_dispatch` (the JSON boundary; handle must survive every input),
  `jsonschema`, `batch_from_value` (+ apply/diff), `graph_from_value`;
  `tools/fuzz.sh <seconds> [targets]`; seeds from `core/fuzz/seed_corpus.py`,
  grown corpus machine-local. Local runs: ffi_dispatch 9.6k runs/240 s,
  jsonschema 4.4 M/240 s, graph_from_value 4.2 M/240 s, batch_from_value
  3.5 M/300 s after the fix below — no findings remaining. **Finding:** the
  upstream XLSX reader aborted the process on a corrupt deflate stream;
  every OOXML loader now inflates each package entry once up front
  (`inflate_probe`) and the workbook reader call is contained, with the
  crash input kept as `harbor_artifacts/tests/regressions/corrupt_deflate.xlsx`.
  Both FFI entry points run under `catch_unwind`: a panic becomes an error
  envelope plus a diagnostics record and the handle stays usable (proved by
  a debug-only `_debug.panic` method). CI job `fuzz` runs 60 s per target.
- **C5 release workflow** — `.github/workflows/release.yml` on `harbor-v*`
  tags (or dispatch dry run): regenerates gate evidence, plaintext
  inspection, SBOM and notices at the tagged commit, assembles
  `evidence/releases/<version>/`, builds the unsigned macOS bundle and the
  debug-key Android APK/AAB (native core cross-compiled with the NDK), and
  attaches everything to a draft GitHub release. Signing stays on the
  operator machine. Not yet exercised end to end (needs a tag); the Android
  cross-compile step is the least certain and is reported by the run.
- **Gates** — `cargo fmt/clippy/test --workspace` (50 suites), `cargo audit`,
  `cargo deny`, app `flutter test` 32/32 (new: first-run journey, catalog
  assets, build info), dossier validator PASS.

## Session 36 (2026-09-20): Phase C1 — diagnostics without telemetry

- **Core** — `harbor_core::diagnostics`: rolling (500 records), AEAD-sealed
  crash/error log under a workspace-derived key at
  `<data_root>/diagnostics/log.hdiag`; records are redacted on entry
  (absolute paths → `<path:.ext>`, quoted strings > 48 chars, 800-char cap);
  process-wide panic hook; `export()` writes a zip (`diagnostics.json` with
  build/runtime/device-class facts, installed model ids, run counts and the
  redaction policy; `records.jsonl`) that never overwrites and never
  contains document content, chunks or prompts.
- **FFI** — every boundary error is recorded with its method as context;
  `diag.record` (app errors), `diag.list`, `diag.export {destination,
  app_version}`; the panic hook is installed at open.
- **App** — `DiagnosticsSink` hooks `FlutterError.onError` and
  `PlatformDispatcher.onError` (bounded buffer before the core opens);
  Settings → Diagnostics → **Export diagnostics** (native save dialog on
  desktop, app documents folder on mobile; states what the bundle contains
  and does not). `build_info.dart` pins the app version (1.1.0+2) with a test
  against pubspec. "Diagnostics upload" stays N/A_DISABLED (M4).
- **Inspection** — `plaintext_at_rest_inspection` now seeds a diagnostics
  record carrying a sentinel and a document path, scans the data root
  (sealed) and the export bundle (redacted record present; no run,
  document, temp or knowledge sentinel; no path).
- **Tests** — core unit tests (redaction, sealed round trip + roll, export,
  panic hook), FFI `diagnostics` test, shell test "diagnostics export writes
  a bundle from the live core"; the accessibility audit caught and fixed a
  320 px / 200 % overflow in the new card. The live-core commit journey test
  gets a CI-sized budget (it timed out once on a shared runner).
- **Gates** — `cargo fmt/clippy/test --workspace` (48 suites), app `flutter
  test` 30/30, packages green, dossier validator PASS.

## Session 35 (2026-09-20): Phase B3–B4 — nine runnable skills, live tier 13/18, GGUF long-prompt fix

Five more built-ins decomposed into graphs (Deck Review & QA, Financial Model
Review, Document Style Review, Team Update (3P), Document Co-Authoring (cold
reader test)); runnable skills 4 → 9 of 30. Decision 0006 addendum has the
design: judgement in deterministic tools, one structured model node, every
model output verified below it, `not_checked` reported for what the IR cannot see.

- **Tools** — `harbor_core::tools::review`: `deck.inspect`,
  `workbook.conventions` (shifted neighbour formulas verified through the
  pinned engine; mechanical fix/review split), `docx.inspect` (hierarchy,
  direct-formatting overrides, direction, margins, heading scale from
  styles.xml), `text.verify_numbers`. `formula.build_operations` accepts a
  flat `findings` array and attaches an engine-verified `suggested_formula`
  so the model never spells a formula. `artifact.read` returns a flat `text`
  for DOCX/PPTX/PDF.
- **Fixtures** — `board_deck.pptx`, `dcf_model.xlsx`, `report_styles.docx`
  (deterministic, `tools/make_skill_eval_fixtures.py`).
- **Evals** — 23 replay cases (all pass in CI, no weights); `tiers` on a
  case marks replay-only guard cases. Live tier on Qwen2.5-1.5B: **13/18**
  (`evidence/skill_evals/live-6a1a2eb6d156.json`, commit-bound; recorded
  cassettes under `evals/skills/*/cassettes/live/`, machine-local).
  Financial Model Review runs with no model call (like Placeholder Fill).
- **Runtime fix** — GGUF provider: chunked prefill (a prompt longer than
  `n_batch` used to abort the process), correct logits index after chunked
  prefill, typed refusal of prompts that cannot fit the model context, sized
  embedding batches. Regression test on the real runtime
  (`gguf_provider::long_prompts_are_prefilled_in_chunks…`).
- **App** — Skills surface shows nine runnable graphs; the Run sheet form
  covers the new inputs (team/period/notes; artifact attach for the rest).
- **Gates** — `cargo fmt/clippy/test --workspace` (47 suites, 0 failures),
  replay 23/23, grammar probe over all nine graphs, `flutter analyze` clean,
  app `flutter test` 28/28, dossier validator PASS.

## Session 34 (2026-09-20): Phase B1–B2 — safe-commit through FFI and app, proposal diff

The skill loop no longer stops at "proposal": a user can run a skill against
their own document, review the before/after diff, and get a safely committed
file. Default is **Save New Copy**; Overwrite is a confirmed secondary action.

- **Core** — `Executor::decide_and_commit(run_id, CommitTarget)`: pre-flight
  (receipt age ≤ 15 min, base bytes hash to the approved base, batch
  re-applied and hashed against the approved output, Save New Copy never
  overwrites), then durable `run.approval_decided` → `run.effect_dispatched`
  → `SafeCommitter::{commit_new_copy, commit_external}` →
  `run.effect_resolved{committed|conflict|outcome_unknown|failed}`; a refused
  write fails the run with the reason and leaves the original untouched.
  `PendingApproval` gains `diff` (before/after per op) and `requested_at`.
  `tools::builtin::{batch_from_value, apply_batch, proposal_diff}`. Batch ids
  are bound to base + operations; `SafeCommitter::commit_external` replay
  verifies the destination (a third version is a conflict).
- **FFI** — `run.commit_proposal { run_id, destination, target:
  save_new_copy|overwrite, artifacts }` → `{report, commit | commit_error}`;
  commit journal at `<data_root>/db/commit_journal.db`.
- **App** — `HarborService.commitProposal`, `CommitTarget`; Run sheet shows
  the proposal with `ArtifactDiffView` (base/proposed hashes, `-`/`+` lines
  per op) and the actions Save new copy / Overwrite original… / Reject;
  desktop uses the native save dialog, mobile lands the copy in the app
  documents folder and says where. l10n en/ar.
- **Tests** — `harbor_core/tests/commit_proposal.rs` (5: diff carried,
  new-copy happy path with events/journal/replay, pre-dispatch refusals,
  expired receipt, overwrite conflict inside the protected interval);
  `harbor_artifacts` replay-conflict unit test; FFI `skill_runs` commit test;
  `shell_test.dart` "run sheet reviews the diff and saves a new copy on the
  live core" (open fixture → run → diff → Save New Copy → reopen the copy is
  the approved output, original unchanged).
- **Gates** — `cargo fmt/clippy/test --workspace` (47 suites, 0 failures),
  `cargo audit` clean, `cargo deny check` ok, `flutter analyze` clean, app
  `flutter test` 28/28, packages green, dossier validator PASS.

## Session 33 (2026-09-19): Phase A — landed session 32, CI hygiene gates, kill/restart test

Executes `docs/PRODUCTION_PLAN.md` phase A. Session 32 is committed in six
bisectable slices (contracts → inference → core → skills → app → docs; the
inference slice precedes core because the executor tests replay through the
cassette provider), each slice carrying a regenerated dossier seal and green
`cargo test --workspace` / `flutter test` at that commit.

- **A2 stale-seal detector** — `tools/check_dossier_seal.py` runs first in the
  `dossier` job: it regenerates 19/20/24 in place and fails on a non-empty
  `git diff` with an explicit "STALE DOSSIER SEAL — run `python3
  tools/validate_dossier.py --write` and commit …" annotation instead of the
  validator's "Package manifest differs".
- **A3 supply-chain gates** — new `supply-chain` CI job: `cargo audit`
  (`core/.cargo/audit.toml`) and `cargo deny check` (`core/deny.toml`: license
  allow-list matching `docs/release/THIRD_PARTY_NOTICES.md`, no telemetry /
  OpenSSL crates, crates.io only, wildcard and git sources denied);
  `tools/check_advisory_ignores.py` fails when the two accepted-advisory lists
  drift. Workspace crates are `publish = false`. Findings at this commit:
  rustls 0.23.44 → 0.23.45 (RUSTSEC-2026-0285, fixed); the unused direct
  `quick-xml 0.38` dependency removed; RUSTSEC-2026-0194/0195 (quick-xml
  0.37/0.39 DoS, reachable only through the formualizer 0.9.3 pin,
  availability-only on a locally chosen workbook) and RUSTSEC-2026-0192
  (ttf-parser unmaintained via pdf-extract 0.12.1) accepted with reasons and
  re-check triggers in `deny.toml`. Non-blocking `flutter pub outdated`
  report added to the flutter job.
- **A4 kill/restart test** — `core/harbor_core/tests/executor_resume.rs`
  (see decision 0006, "Not done" closed). Executor fix: `decide` and `resume`
  now emit `run.lease_acquired` for their generation.
- **Gates** — `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -D warnings`, `cargo test --workspace` (46 suites, 0 failures), `cargo
  audit` clean, `cargo deny check` advisories/bans/licenses/sources ok,
  `flutter analyze` clean, `dart format` clean, app `flutter test` 27/27,
  dossier validator PASS, seal fresh.

## Session 32 (2026-09-18): skill graphs, tool layer, executor and eval harness (decision 0006)

Skills went from inert catalog entries to runnable, replayable graphs.
Details, measurements and the explicit not-done list are in
`docs/decisions/0006-skill-graphs-tools-and-eval-harness.md`.

- **Schema** — `schemas/graph.schema.json` (`harbor.graph/v1`): control flow
  as data; allowlist = tool nodes; bounded cycles; grammar-safe structured
  output schemas. `harbor.skill/v2` manifests carry a graph; v1 stay
  declarations. Run-event schema gains step branches with node ids and io
  hashes (additive; see `28_Correction_Log.md`).
- **Core** — `harbor_core::{graph, jsonschema, pointer, tools, executor,
  harness}`: minimal JSON-Schema validator, tool registry with 13 built-in
  read/propose tools, graph executor over `harbor_agent` (lease, events,
  budgets, encrypted snapshots, approvals, cancellation), eval harness with
  replay/live/record tiers. `harbor_inference`: cassette record/replay
  provider; grammar-constrained structured output on the GGUF path
  (`StructuredOutput` declared and proven on the real runtime).
- **Skills** — Formula Audit & Repair, Placeholder & Form Fill, Second Look,
  Meeting Notes decomposed into graphs (`core/harbor_core/src/graphs/`),
  each with an eval suite under `evals/skills/`. Fixtures generated by
  `tools/make_skill_eval_fixtures.py` (deterministic OOXML).
- **Invocation** — FFI `skills.list` (graph facts), `tools.list`,
  `op.start_skill_run`, `run.decide`, `run.snapshot`, `eval.run_skill`;
  Flutter Skills surface now honest (Runnable graph vs Declaration only),
  Run sheet generated from the input schema with node trail and approval.
- **Measured** — replay tier 11/11 in CI with no weights; live tier on
  Qwen2.5-1.5B 6/11 (`evidence/skill_evals/`), every failure a pinned model
  fact contained by a deterministic node.
- **Gates** — cargo fmt/clippy/test workspace green (new: graph_executor 7,
  skill_evals 3, ffi skill_runs 2, schema_grammar_probe 3); app `flutter
  test` 27/27; `flutter analyze` clean; dossier validator PASS.

## Session 31 (2026-09-17): nine community-derived skills adopted as Skill v1 data

Surveyed `anthropics/skills` (19 skills) and `MiniMax-AI/skills` (17 skills)
for methodology that survives Harbor's narrower skill contract (data only,
closed tool catalog, no privacy widening). Nine were rewritten as original
`harbor.skill/v1` manifests in `core/harbor_core/src/builtin_skills.json`
(now 30 built-ins): Document Co-Authoring, Team Update, Second Look, Skill
Author, Financial Model Review, Formula Audit & Repair, Placeholder & Form
Fill, Deck Review & QA, Document Style Review. The rest were rejected for
license (Anthropic document skills are no-derivatives), egress (MiniMax
cloud media APIs), missing capability (vision, PDF forms, style ops) or scope
(developer tooling). Full table, license review and follow-up (SKILL.md
frontmatter importer for HBR-072) in
`docs/decisions/0005-adopted-community-skills.md`.

- **Gates** — `cargo test -p harbor_core skills` 7/7 (two new tests: unique
  ids + eval coverage, adopted set present); `cargo test -p harbor_ffi` 4/4;
  app `flutter test` 25/25 against the rebuilt debug core; `flutter analyze`
  clean. No runtime, schema, catalog or FFI change.

## Session 30 (2026-09-17): complete frontend overhaul (iOS · Android · desktop)

The Flutter shell and the `harbor_ui` design system were rebuilt end to
end against the UI/UX authority (§4 responsive contract, §5–§6 tokens and
typography, §7–§13 surfaces, §15 accessibility). Nothing in the core
changed; every surface still renders live core facts or an honest
degraded state.

- **Design system (`packages/harbor_ui`)** — split into tokens /
  foundation / status / navigation / trail / sheet / model dock / fit
  score / states / diff / progress. Typography now carries real
  platform fallback chains (monospace identifiers previously fell back
  to a proportional face) and exact token weights via variable-font
  axes; a motion scale from the tokens collapses under reduce-motion;
  every stock Material widget is themed from the tokens for both modes.
- **Shell** — compact: top bar + Trust chip + five-item bottom bar with a
  More sheet (was nine cramped tabs); medium: icon+label rail;
  expanded: 72 px rail with tooltips; wide: 220 px rail; ≥1280: docked,
  toggleable 320 px Lens. Surfaces keep their state across navigation
  (cross-fade stack). Desktop: ⌘/Ctrl+1…9, ⌘K command palette, ⌘L, ⌘O,
  ⌘,. Language/theme/Lens choices persist (`harbor-prefs.json`);
  ThemeMode.system is honoured; Android is edge-to-edge with adaptive
  system-bar icons and predictive back opt-in.
- **Surfaces** — Home (greeting, composer with Enter-to-send on desktop,
  quick actions, live model dock, in-progress ops, recent runs,
  knowledge status); Ask (session conversation, model picker chip,
  grounded/not-grounded badges, citation score bars, abstention banner);
  Work (real spreadsheet grid with frozen headers + formula bar + sheet
  tabs, typographic document view with outline, deck filmstrip + 16:9
  canvas, PDF page cards, compatibility banner from the Office matrix,
  surfaced preview errors); Models (Import GGUF is now a real local
  install, installed cards with Fit Score + "Use for Ask"); Agents
  (honest not-enabled state); Skills (filter, family grouping, detail
  sheet); Knowledge (index metrics, confirmed removal); Activity (Runs +
  Operations tabs, run detail with counters and trail); Settings
  (system/light/dark, language, Trust Pulse, identity with copy,
  shortcuts, about). 169 new EN/AR strings with ICU plurals.
- **Gates** — `flutter analyze` + `dart format` clean in all four
  packages; harbor_ui 15 tests, app 25 tests PASS against the real core
  (was 19). The accessibility gate now runs the real layouts (its
  MediaQuery override used to zero the size and always test compact),
  covers 320/390/1280 at 200 % text, and names the overflowing widget.
- **Verified on device** — iPhone 17 Pro simulator with the live core:
  composer → durable run → Activity → run detail; More sheet; Lens
  sheet; Arabic RTL in light mode; preferences restored after relaunch.
- **Gotchas recorded** — the Xcode "Embed Harbor Native Core" phase only
  builds `libharbor_ffi.dylib` when it is *missing*, so a stale
  simulator dylib (pre-`openEx`) loads and the app reports the missing
  symbol in its degraded banner; rebuild with `~/.cargo/bin` FIRST on
  PATH (Homebrew's `rustc` shadows rustup's and lacks the iOS targets).
  Bundled fonts (Inter / Noto Sans Arabic / JetBrains Mono) remain a
  follow-up decision: the notices fixture still declares system fonts
  only, and the fallback chains cover every platform meanwhile.

---

## Authoritative snapshot — session 29 (2026-09-14; post-RC hardening
## round; HEAD binds machine evidence unless stated otherwise — run
## `git log --oneline -3`)

### Session 29: the ten-item post-RC work order (user-directed)

The user issued a ten-item recommended order of work (persistent storage,
knowledge encryption, OS keystores, full journey, no-op removal, FFI off
the UI isolate, knowledge lifecycle, evidence repair, CI gates, external
qualification). All machine-completable items are DONE and requalified at
the session-29 commit; the external item remains blocked on hardware /
operator credentials, unchanged from rc1.

1. **Persistent app storage, service disposal, unique run IDs — FIXED.**
   - Storage: the app data root moved from a per-launch temp dir to the
     platform app-support dir (`path_provider`):
     `~/Library/Application Support/dev.harbor.harborApp/harbor-data`
     (macOS) / `/data/data/<pkg>/files/harbor-data` (Android). Verified in
     the PACKAGED macOS app (agent.db + network_audit.db + store.db created
     there; dylib mapped) and on the Android emulator (below). Durable
     device identity persists in `store.db device_meta` and is stable
     across restarts on both platforms.
   - Disposal: `HarborService.close()` actually existed but was never
     called; the app now closes the native core on widget dispose AND on
     `AppLifecycleState.detached` (WidgetsBindingObserver).
   - Run IDs: `submitRequest` used the LITERAL string
     `run-\${DateTime.now()...}` (an escaped `$` — a constant id!), so the
     second submission in a workspace always failed with RunExists and was
     silently swallowed. IDs are now 96-bit random hex (`Random.secure`)
     Dart-side, and `run.create` mints `HarborId::generate("run")` core-side
     when the caller omits one.
2. **Knowledge database encryption — DONE + qualified.** Chunks and
   embedding vectors are sealed with ChaCha20-Poly1305 under a
   workspace-derived key (`harbor.knowledge.chunk/v1`), AAD-bound to each
   chunk identity; at rest `knowledge.db` holds no text and no vectors.
   Legacy plaintext DBs (dev-era format) are migrated by re-sealing.
   The plaintext-at-rest inspection (`plaintext_at_rest_inspection.rs`)
   now drives the REAL persistence layer (`harbor_ffi::knowledge::
   KnowledgeStore`) with a knowledge sentinel and byte-scans the whole
   data root — PASS at the session commit (surfaces list includes
   knowledge_db). Unit tests cover round-trip, tamper, AAD-binding and
   key-separation; the flutter e2e knowledge/RAG tests cover the live
   sealed path.
3. **Real OS keystore adapters — DONE.** `harbor_store::native_keystore`:
   - macOS/iOS: `KeychainKeyStore` (Security framework generic-password
     items; verified live in the packaged macOS app — item
     `dev.harbor.core/harbor.device` created and reused).
   - Windows: `DpapiKeyStore` (CryptProtectData, user scope) —
     compile-gated by the new `windows-core` CI job (cargo check + clippy
     for the whole workspace on windows-latest).
   - Android: the NDK side of a dlopened lib has no JavaVM, so the
     embedding is the adapter: Kotlin `MainActivity` seals a random
     32-byte root under a non-exportable AndroidKeyStore AES-256-GCM key
     (`files/harbor-device-root.bin`, 60-byte nonce||ct blob) and injects
     the root at open via the new `harbor_core_open_ex(..., root_hex)`.
   - Legacy file-root data roots are rotated to the native keystore at
     open (`rotate_file_root_to_native`, tested incl. data survival).
   - Caveat recorded: ad-hoc rebuilds change the binary hash; the
     keychain item ACL follows the creating binary, so a rebuild needs one
     user Allow (or a reset). Signed releases have a stable identity.
4. **Complete user journey — WIRED and TESTED.** Models install (HF
   acquisition or local GGUF import) -> Knowledge ingestion (file picker
   via `file_selector`, or pasted text; docx/pdf text extracted through
   the qualified preview paths; everything else honestly refused) ->
   Ask (grounded generation with a selected chat model; answer card with
   citations, executed-on and token usage; INSUFFICIENT_EVIDENCE renders
   the abstention note) -> durable activity (`op.start_generate` logs the
   question as StepStarted and the answer as StepCompleted into the run;
   Activity lists the run and Run Trail replay shows request + answer
   summaries). Covered by `shell_test.dart` knowledge + RAG journey tests
   against the real core (install via staged path, ingest, generate,
   replay).
5. **No-op / misleading actions — REPLACED with real behavior or honest
   disabled states.** Home quick chips prefill the composer; the attach
   button routes files into the knowledge index; ModelDock navigates to
   Models; HF rows run the REAL brokered acquisition (the fake
   `setState(_installedId)` button is gone) with live progress + cancel;
   Models > Installed empty state navigates to Recommended; Work > Open
   file opens a real picker; Ask no longer has a permanently-disabled
   search (retrieval-only search + generate actions); Agents surface
   states plainly that agent orchestration is not enabled in this
   release (no fake button); Knowledge surface is a full management UI
   (add files/paste text/remove/identity); Settings shows the durable
   device identity. `models.search_hf` envelope fixed (was a bare JSON
   array the Dart client could never parse — every HF search crashed).
6. **FFI off the UI isolate + progress/cancellation — DONE.**
   `harbor_native/harbor_worker.dart`: a long-lived background isolate
   owns the native handle; every core call crosses it (request/reply
   protocol over ports), so the UI isolate never blocks on FFI.
   Long-running work (acquisition, ingestion, generation) additionally
   runs on native threads behind an op registry: `op.start_acquire` /
   `op.start_generate` / `op.start_ingest` / `op.status` / `op.cancel` /
   `op.list`, with real progress (bytes via the brokered download loop,
   chunks during ingest, tokens during generation — `AcquireProgress`
   plumbed through `HfAcquirer` and `generate_cancellable`) and
   cooperative cancellation at chunk/token/file boundaries. The service
   polls status, exposes kind-tracked snapshots, and the UI renders
   progress bars + cancel buttons.
7. **Knowledge replacement, removal, identity — FIXED.** Ingesting an
   existing source id now REPLACES it wholesale (transactional delete +
   reinsert; the live index is replaced too — stale higher-ordinal
   chunks of a longer previous version can no longer survive; regression
   tested). `knowledge.remove_source` + `knowledge.sources` exposed over
   FFI and managed in the UI (removed sources stay revoked so past
   citations report Removed). Device identity persists (store.db);
   `identity.get` exposes device + workspace identity.
8. **Release evidence — REGENERATED at the session-29 commit.** All
   machine suites re-run: `cargo test --workspace` 214 passed / 0 failed;
   gguf-backend suite; plaintext-at-rest inspection (now incl.
   knowledge.db) PASS; flutter suites (shell 13, a11y 5, l10n 2, plus
   harbor_ui/domain/native packages) PASS; `dart format` + `flutter
   analyze` clean in all four Dart packages; `cargo fmt --check` and
   `cargo clippy --workspace --all-targets -- -D warnings` clean.
   Performance qualification re-run (reference device). Artifacts
   repackaged (APK v1.0.0(1), AAB, macOS app, iOS static-archive path
   verified at link level with Security.framework). Evidence bundle
   reassembled as `1.0.0-rc2` (tag `harbor-v1.0.0-rc2`).
9. **CI gates — ADDED and GREEN.** `.github/workflows/ci.yml` now gates:
   rustfmt, clippy `-D warnings` (plus a windows-latest job for the
   DPAPI/cdylib paths), dart format, flutter analyze, package tests, and
   the app suite against the LIVE core (the job builds `libharbor_ffi`
   first). Fully green on the session-29 HEAD (`run 34877254045`: rust ✓
   flutter ✓ dossier ✓ windows-core ✓). Two CI findings fixed along the
   way: the dossier manifest now seals exactly the git-tracked file set
   (gitignored generated files previously made the seal irreproducible),
   and the real-network test tier is qualification-machine-local — HF's
   edge resets shared runner IPs, so runner runs are not meaningful
   evidence; the tier passes on this machine and is recorded
   commit-bound in evidence/.
10. **External qualification — UNCHANGED blockers, machine part done.**
    Android emulator tier requalified on the NEW release APK (see below);
    physical iPhone/Android hardware, Apple identity + notarization, Play
    Console upload key, a Windows machine and the min-spec Apple-silicon
    device remain BLOCKED_EXTERNAL / BLOCKED_DEVICE_EVIDENCE exactly as in
    rc1 (`resume_with` paths unchanged in the gate report).

### Requalification record (evidence/device_qualification.json →
### session_29_persistence_and_keystore_requalification, commit-bound)

- **macOS packaged app** (ad-hoc, release): launch live, dylib mapped;
  persistent data root in Application Support; keychain-held device root
  (no file keys); device identity stable across quit + relaunch.
- **Android emulator tier** (cvbase_test API 34 arm64, release APK
  v1.0.0(1)): AndroidKeyStore-sealed root (60-byte blob) injected via
  `harbor_core_open_ex`; persistent workspace (agent.db + WAL/SHM +
  network_audit.db + store.db); device identity
  `device-00d53bcb64dab31b41a8c5b3` stable across `am force-stop` +
  relaunch. extractNativeLibs=false caveat as before.

### Test results (regenerated at the session-29 commit)

| Suite | Result |
| --- | --- |
| `cargo test --workspace` | **214 passed / 0 failed** |
| gguf-backend inference suite | PASS (component of the workspace run) |
| Plaintext-at-rest inspection | PASS — now covers knowledge.db (sealed chunk sentinel) |
| Flutter: app shell (13), a11y (5), l10n (2), packages (9) | PASS; `flutter analyze` + `dart format` clean |
| Clippy / rustfmt | `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean |
| Performance qualification | re-run on the reference device (see evidence/perf_qualification.json) |
| Office / network / optional-disabled / SBOM / notices | PASS (regenerated; notices 408 entries) |

### External-prerequisite audit (2026-09-14)

The absence of every operator-supplied prerequisite was VERIFIED on this
machine, not assumed — `security find-identity -p codesigning -v` → 0
identities; `xcrun devicectl list devices` → none; `adb devices` →
emulator only; `~/.harbor-keys/` holds just the catalog root key and no
`HARBOR_ANDROID_KEYSTORE*` env exists; no Windows VM is installed;
notarytool has no stored profile. The full audit is recorded
commit-bound in `evidence/device_qualification.json`
(`session_29_external_prerequisite_audit`). Closing the eight blocked
gates requires a human to supply the resources and run the resume_with
paths; nothing machine-completable remains.

### Remaining external blockers (unchanged from rc1)

- Apple Developer identity + notarization (MAC-02, IOS-03)
- Physical iPhone/iPad (IOS-02), physical Android device(s) (AND-03)
- Play Console + upload key (AND-04)
- Windows machine (WIN-01) — compile-gated in CI in the meantime
- Minimum-spec Apple-silicon device (PERF-01)

### Reproduce the evidence

```bash
cd core && cargo test --workspace
cargo test -p harbor_inference --features gguf-backend
cargo test -p harbor_artifacts --test office_conformance
cargo test -p harbor_integration --test plaintext_at_rest_inspection
cargo run --release -p harbor_integration --example perf_baseline --features gguf-backend -- .. ../fixtures/models-store
cd ..
python3 tools/run_performance_qualification.py --write
python3 tools/generate_gate_evidence.py --write
python3 tools/validate_dossier.py --write
python3 tools/check_optional_disabled.py --write
python3 tools/generate_sbom.py --write
python3 tools/generate_third_party_notices.py --check
python3 tools/check_contrast.py --write-evidence
# No --partial here: that flag is for a CI runner, which cannot produce
# the network capture or the performance run. This machine can, so the
# bundle must be complete (see docs/release/rings.md).
python3 tools/assemble_release_evidence.py --version 1.0.0-rc2 --write
python3 tools/check_ring_gate.py \
    --report evidence/releases/1.0.0-rc2/release_gate_report.json \
    --ring 1 --platforms mac,ios,android
cd apps/harbor_app && ~/harbor-tools/flutter/bin/flutter test
# real-network (unchanged since session 27):
cargo test -p harbor_modelhub --lib -- --ignored real_hf_capture --nocapture
```

### Environment notes (additions)

- The Flutter app now depends on `path_provider` + `file_selector`
  (pulled in via pub; plugins compile into the bundles automatically).
- Android cross-compile: stage the rebuilt
  `target/aarch64-linux-android/release/libharbor_ffi.so` into
  `apps/harbor_app/android/app/src/main/jniLibs/arm64-v8a/` BEFORE
  `flutter build` (recipe in the session-27 notes below).
- `dart`/`flutter` are NOT on PATH: use `~/harbor-tools/flutter/bin/...`.

---

## Historical session notes

*(ordered oldest → newest; the authoritative snapshot above supersedes
anything below)*

### Session 28 (2026-09-13; RELEASE CANDIDATE harbor-v1.0.0-rc1)

Superseded by the session-29 snapshot above; the rc1 state was: frozen RC
tag `harbor-v1.0.0-rc1` = 3b50001, gate report 11 PASS / 4
BLOCKED_EXTERNAL / 4 BLOCKED_DEVICE_EVIDENCE / 2 N/A_DISABLED / 0 FAIL,
iOS production-device static linkage, Android AAB + INTERNET permission,
Apple privacy manifests, store collateral, sealed evidence bundle,
Android §10 reduced suite + EN/AR screenshots, network capture rebound.
Only external blockers remained.

### Sessions 1–8 (2026-09-12, early)

- llama.cpp GGUF provider is real: `harbor_inference` feature `gguf-backend`
  pins `llama-cpp-2 =0.1.156` (vendored llama.cpp snapshot; decision 0002).
  Greedy deterministic decoding, cooperative cancellation, Metal execution.
- Flutter 3.47.4 at `~/harbor-tools/flutter`. `packages/harbor_ui` (Harbor
  Current 2 tokens, RTL parity, breakpoints §20, Harbor Rail/Trust Pulse/
  Run Trail/Harbor Sheet/Model Dock/Fit Score patterns); `apps/harbor_app`
  (9 surfaces, adaptive shell, EN/AR); `core/harbor_ffi` +
  `packages/harbor_native` single JSON-dispatch C ABI verified against the
  real dylib.
- Git initialized at `4f90f73`; CI workflow in place.
- Accessibility audit became a permanent test gate (semantic labels, 200%
  text scale, 44px targets, keyboard traversal); CycloneDX 1.5 SBOM tool;
  migration/rollback docs.
- Real-network acquisition through the Egress Broker (ureq+rustls, redirects
  DISABLED — every hop re-authorized): HfAcquirer with staged installs,
  hash verification, first-download identity; `models.search_hf` /
  `models.acquire_hf` FFI; signed catalog drives acquisition
  (epoch-protected per-file hashes; signed-but-wrong hash blocks install);
  streaming downloads with incremental SHA-256; retry-once for 429/503.

### Sessions 9–12 (2026-09-12)

- Qualified reference device bound:
  `fixtures/qualification/reference_device_macos_arm64.json` (Mac17,8 /
  M5 Pro / 24 GB / macOS 26.5.1, Metal 4);
  `qualified_device_manifest_sha256` bound in 26_Qualification_Profiles.
- Release-mode builds: macOS release app (ad-hoc signed) launched and quit
  cleanly; Android release APK (debug-key signed — store signing still
  requires credentials). Home composer drives the runtime
  (`run.log_request` under lease authority).
- Test tiers split: `cargo test --workspace` fully offline; real-network
  tests `#[ignore]`-gated.
- Key ceremony executed: release root key seed stored OUTSIDE the repo at
  `~/.harbor-keys/catalog-root.key` (0600); signed production catalog
  (epoch 1) binds qwen2.5-1.5b-instruct, bge-small-en-v1.5, stories260k
  with pinned hashes; permanent test verifies the committed signature.
- Performance baselines measured on the reference device (M5 Pro, Metal,
  Qwen2.5-1.5B Q4_K_M): load 153 ms p50, TTFT 53 ms p50, 148 tok/s, RAG
  ~21k docs/min; thermal 10-min sustained generation with no throttling.

### Sessions 13–16 (2026-09-12)

- harbor_sync: full 11_Sync_Protocol core (record envelopes, AEAD, hash
  chains, per-device sequence contiguity, epoch/revocation, type-enforced
  LWW, transfer handshake, 90-day horizon, signed snapshots, bundle
  transport with latest-per-object dedupe + tombstone propagation). Sync
  stays DISABLED; activation requires ACC-057.
- Office conformance deepening: DOCX headings/numbered lists/tables with
  gridSpan; XLSX merged-cell round trips; bar-chart machinery; DOCX
  TableCellSet merge-aware typed op; formatting-preserving same-length
  replacement across runs; XLSX chart round trip; PPTX DrawingML chart
  embedding with cached values + embedded workbook.
- SBOM commit versioning (git commit + build profile properties).
- PDF text extraction with page mapping (corrupt-input rejection tested);
  iOS simulator launch evidence (session 16).

### Session 26 (commit d3cdd61)

- `artifact.preview` DOCX dispatch fixed (was misrouted into the workbook
  branch); DocxPreview IR + Work Canvas rendering branches for docx/pdf;
  Dart FFI e2e test previews the real structured.docx fixture.

### Session 27 (commits de09abc → a8447cb)

- Reconciled stale dossier state (derived reports regenerated; manifest
  PASS restored at de09abc).
- Eight release-gap workstreams: Office matrix full fixture coverage;
  network-capture qualification; AEAD-sealed run events + plaintext
  inspection; performance thresholds v2 (cold/warm split); Windows build
  script + docs; packaging-to-credential-boundary scripts; optional-
  capability zero-surface proof. Office/eval clock fix (14367ae), corpus
  expansion to 464 cases (5e5a6bd).
- App bundles carry the live native core: macOS dylib in
  Contents/Frameworks (5468ca0), iOS simulator embedding build phase with
  live verified launch (d323bde).

### Session 28 (2026-09-13; this session)

- Release-candidate freeze work: iOS production-device static-archive
  embedding (symbol-verified), Android AAB + INTERNET permission review,
  Apple privacy manifests + export-compliance analysis, store collateral
  + third-party notices, §18 sealed evidence bundle + §20 gate report,
  post-packaging requalification (macOS live workspace, iOS simulator
  live core), all machine evidence regenerated at 1415e79. RC tagged
  `harbor-v1.0.0-rc1`. See the authoritative snapshot for full detail.
