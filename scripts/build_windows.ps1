# Harbor Windows build + qualification script (complete; execution evidence
# is BLOCKED only because no Windows machine is available in this
# environment — the shared implementation is done).
#
# Prerequisites (Windows 10+ x64):
#   - Visual Studio 2022 with "Desktop development with C++" workload
#   - Flutter SDK 3.24+ on PATH (https://docs.flutter.dev/get-started/install/windows)
#   - Rust toolchain (rustup) with the MSVC target
#   - Git
#
# Usage (PowerShell, from repo root):
#   powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1
#
# The script is idempotent and fails loudly on the first broken step so a
# Windows qualification run produces honest evidence or an explicit error.

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

Write-Host "== Harbor Windows build + qualification ==" -ForegroundColor Cyan
Write-Host "repo: $repo"

function Step($name) { Write-Host "`n== $name ==" -ForegroundColor Yellow }

# 0. Toolchain sanity.
Step "Toolchain check"
flutter --version
rustc --version
cargo --version

# 1. Rust workspace tests (offline suite).
Step "Rust workspace tests"
Push-Location "$repo\core"
cargo test --workspace
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "Rust workspace tests failed" }
Pop-Location

# 2. Contract + dossier suites (Python 3.9+ with requirements-validation.txt).
Step "Contract + dossier suites"
python -m pip install -r "$repo\requirements-validation.txt"
python "$repo\tools\validate_dossier.py"
if ($LASTEXITCODE -ne 0) { throw "dossier validation failed" }
python "$repo\tools\test_contracts.py"
if ($LASTEXITCODE -ne 0) { throw "contract suite failed" }

# 3. Flutter tests (design system + app incl. a11y/contrast gates).
Step "Flutter tests"
Push-Location "$repo\packages\harbor_ui"
flutter test
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "harbor_ui tests failed" }
Pop-Location
Push-Location "$repo\apps\harbor_app"
flutter test
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "harbor_app tests failed" }
Pop-Location

# 4. Windows release build.
Step "Windows release build"
Push-Location "$repo\apps\harbor_app"
flutter build windows --release
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "windows build failed" }
Pop-Location

# 5. Bundle the native core beside the executable.
#    `flutter build windows` does NOT do this and neither did this
#    script: nothing built harbor_ffi.dll and nothing copied it, so the
#    packaged app would have launched into the degraded state with no
#    core at all. The Dart FFI opens "harbor_ffi.dll" by bare name, which
#    Windows resolves from the executable's own directory first. The same
#    omission shipped a coreless macOS bundle until it was caught by
#    opening the artifact.
Step "Native core (release)"
$buildDir = "$repo\apps\harbor_app\build\windows\x64\runner\Release"
if (-not (Test-Path $buildDir)) { throw "expected build output missing: $buildDir" }
Push-Location "$repo\core"
cargo build --release -p harbor_ffi
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "harbor_ffi build failed" }
Pop-Location
$coreDll = "$repo\core\target\release\harbor_ffi.dll"
if (-not (Test-Path $coreDll)) { throw "harbor_ffi.dll not produced at $coreDll" }
Copy-Item -Force $coreDll $buildDir
# Read it back off the packaged folder: what ships is what is there, not
# what the copy above intended.
if (-not (Test-Path "$buildDir\harbor_ffi.dll")) {
  throw "harbor_ffi.dll missing from $buildDir after copy"
}
Write-Host "   native core bundled: harbor_ffi.dll" -ForegroundColor Green

# 6. Package the release folder (portable zip; installer is a later,
#    credential-gated step).
Step "Package"
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$outZip = "$repo\outputs\harbor_app_windows_x64_$stamp.zip"
New-Item -ItemType Directory -Force -Path "$repo\outputs" | Out-Null
Compress-Archive -Path "$buildDir\*" -DestinationPath $outZip
Write-Host "`nPackaged: $outZip" -ForegroundColor Green

# 7. Evidence to record (manual, on the Windows machine):
#    - perf: cargo run --release -p harbor_integration --example perf_baseline `
#        --features gguf-backend -- <repo> <models-store>
#      then: python tools\run_performance_qualification.py --write
#    - launch: start the built harbor_app.exe, confirm window opens, quit.
#    Record results in evidence/windows/ and mark the windows_x64 device
#    class in tools/run_performance_qualification.py accordingly.
Write-Host "`nDONE. Record the evidence items above under evidence/windows/." -ForegroundColor Cyan
