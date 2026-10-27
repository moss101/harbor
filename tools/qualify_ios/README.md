# Physical-device formula qualification (iOS)

A minimal app that runs `harbor_formula::run_qualification()` on an iPhone/iPad and
writes the JSON report to its Documents folder. The Rust part is a static library; the
Swift part is a stub that calls it.

```
cd tools/qualify_ios/rust
cp ../../../core/Cargo.lock .        # same dependency versions as the real build
cargo build --release --target aarch64-apple-ios   # PATH: ~/.cargo/bin first
cd ..
# set DEVELOPMENT_TEAM and PRODUCT_BUNDLE_IDENTIFIER in App.xcodeproj for your team
xcodebuild -project App.xcodeproj -scheme QualifyIOS -configuration Release \
  -destination 'platform=iOS,id=<udid>' -allowProvisioningUpdates CONFIGURATION_BUILD_DIR=$PWD/out build
xcrun devicectl device install app --device <id> out/QualifyIOS.app
xcrun devicectl device process launch --device <id> dev.harbor.qualify.<you>
xcrun devicectl device copy from --device <id> --domain-type appDataContainer \
  --domain-identifier dev.harbor.qualify.<you> --source Documents/qualify.json --destination qualify.json
```
Needs Developer Mode on the device and, for a free team, trusting the developer profile
(Settings > General > VPN & Device Management). The `commit` string is set in `App/main.swift`.

Trap found the first time: the `[patch.crates-io]` entry in `rust/Cargo.toml` is
required. Without it the runner links upstream formualizer-eval, reports the pinned
engine identity (a constant) and fails every case the engine patches fixed.
