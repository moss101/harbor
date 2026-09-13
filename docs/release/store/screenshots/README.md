# Store Screenshots

Captured from the RC release builds with the live native core
(LOCAL ONLY badge, real model state — no degraded banners).

| File | Source |
| --- | --- |
| `android_en_home.png` | RC release APK v1.0.0(1) on arm64 emulator, first launch |
| `android_en_models.png` | Models surface (Model Dock) |
| `android_en_settings.png` | Settings: language/theme + core-reported privacy policy (LOCAL ONLY / ON DEVICE) |
| `android_ar_home_rtl.png` | Full Arabic RTL home (in-app locale toggle) |
| `android_ar_settings_rtl.png` | Arabic RTL settings |
| `ios_en_home.png` | iOS simulator build (debug — release device build verified by link/symbol evidence; see gate report IOS-01/IOS-02) |

## Remaining manual step (one tap)

The app takes its locale from the in-app Settings toggle only (by design —
it ignores the OS locale), and the iOS simulator has no command-line tap
injection on this machine. The iOS **Arabic** listing screenshot therefore
needs one manual tap on the running simulator: Settings → اللغة → العربية,
then capture. Android AR screenshots above are from the release APK.
