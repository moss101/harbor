# Harbor — Supported-Device Matrix (release candidate 1.0.0)

Status tiers: **Qualified** (production evidence on that class),
**In qualification** (builds run; hardware-bound evidence pending),
**Not in RC** (no distribution in this release).

| Platform | Class | Status | Notes |
| --- | --- | --- | --- |
| macOS | Apple silicon (M-series), 16 GB+ RAM | **Qualified** (reference device: M5 Pro, 24 GB) | Release build, ad-hoc signed; Developer ID + notarization pending operator credentials |
| macOS | Apple silicon, 8 GB | In qualification | Minimum-device performance profile pending min-spec hardware; Fit Score gates model suggestions |
| iOS / iPadOS | iPhone/iPad with A12 or newer | In qualification | Release-style build links Harbor core statically and passes symbol/link verification; install/launch evidence requires a physical device |
| Android | arm64-v8a phones/tablets, Android 10+ | In qualification | Release APK with live native core verified on arm64 emulator; physical-device tier pending hardware |
| Android | x86_64 | Not in RC | Emulator-class only |
| Windows | x64 | Not in RC | Complete build + qualification tooling ships in-repo; execution requires a Windows machine |

## Performance floor

Frozen thresholds live in
`fixtures/qualification/performance_thresholds_reference_macos_arm64.json`
(v2). They are device-class specific; the reference-class numbers (e.g.
warm model load p95 ≤ 250 ms, TTFT p95 ≤ 100 ms, ≥ 100 tok/s generation
with the qualified 1.5B Q4_K_M model) were measured on the reference
device and are **not** universal. Minimum-device classes are recorded as
`BLOCKED_DEVICE_EVIDENCE` until measured, and thresholds for those classes
will be frozen from measurement, not projection.

## Model fit

Model availability is gated per device by Fit Score against measured
device memory — see `model_compatibility.md`.
