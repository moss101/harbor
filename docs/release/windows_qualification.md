# Windows build & qualification — complete instructions

Status: **implementation complete; execution evidence BLOCKED_DEVICE_EVIDENCE**
(no Windows machine available in the build environment). Everything below is
runnable on any Windows 10+ x64 host with the prerequisites installed.

## Prerequisites
1. Visual Studio 2022 (or Build Tools) with the *Desktop development with C++* workload.
2. Flutter SDK 3.24+ on PATH.
3. Rust (MSVC toolchain) via rustup.
4. Python 3.9+ for the contract/dossier suites (`pip install -r requirements-validation.txt`).

## One-command build + qualification
```powershell
powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1
```
The script runs, in order: Rust workspace tests → dossier + contract suites →
harbor_ui / harbor_app Flutter tests (incl. a11y and l10n gates) →
`flutter build windows --release` → portable zip under `outputs/`.

## Performance qualification on the Windows host
```powershell
cd core
cargo run --release -p harbor_integration --example perf_baseline --features gguf-backend -- .. <models-store>
cd ..
python tools\run_performance_qualification.py --write
```
The first Windows run is a CALIBRATION run: record `evidence/perf_baseline.json`
output, freeze `performance_thresholds_windows_x64.json` from it (copy the
reference file, replace measured values + rationale, date it), then the
independent qualification run compares against the frozen file. Then flip the
`windows_x64` entry in `tools/run_performance_qualification.py`
`BLOCKED_DEVICE_CLASSES` off and record the measured values.

## Launch evidence
Start `build\windows\x64\runner\Release\harbor_app.exe`, confirm the window
opens with the Model Dock visible, quit cleanly. Record a screenshot under
`evidence/windows/launch/`.

**A window is not enough.** The app opens and renders its shell with no
native core at all — that is the honest degraded state, by design — so a
screenshot of a window proves only that Flutter started. Confirm in the
same screenshot that:

- the trust chip in the header reads **LOCAL**, not `OFFLINE`; and
- **Settings → About** shows *Native core: Loaded*.

If either says otherwise, `harbor_ffi.dll` is not beside `harbor_app.exe`
and the build is coreless. `build_windows.ps1` now builds and copies it
and fails if it is missing, but check the running app rather than
trusting the script: a coreless macOS bundle shipped for months behind a
"launch verified" note that was true and meaningless.

## Notes
- `harbor_inference` `gguf-backend` builds with the llama.cpp CPU (or CUDA,
  if available) backend on Windows — no Metal. Model-load and tok/s
  thresholds are per-device-class and must be frozen from a Windows
  calibration run, never copied from Apple Silicon numbers.
- The Windows runner scaffolding lives in `apps/harbor_app/windows/` and is
  built by the standard Flutter toolchain; no Harbor-specific patches are
  required.
