# On-device qualification

Runs the EmbeddingGemma 2 checks on a physical iPhone inside the real app.

Why a release build and not `flutter test`: `flutter test` cannot start an app
on a wirelessly connected iOS device, and release apps launch without Flutter
tooling.

1. Developer Mode on, phone unlocked, trusted; a development provisioning
   profile for the bundle id you use.
2. `cp tools/device_qualification/qualify_main.dart apps/harbor_app/tool_device/`
3. Temporarily set `PRODUCT_BUNDLE_IDENTIFIER` / `DEVELOPMENT_TEAM` in
   `ios/Runner.xcodeproj/project.pbxproj` (revert with `git checkout` after).
4. `flutter build ios --release -t tool_device/qualify_main.dart`
5. `xcrun devicectl device install app --device <id> build/ios/iphoneos/Runner.app`
6. `xcrun devicectl device copy to --device <id> --domain-type appDataContainer
   --domain-identifier <bundle id> --source fixtures/models/embeddinggemma-2-Q8_0.gguf
   --destination Documents/qualify/embeddinggemma-2-Q8_0.gguf`
7. `xcrun devicectl device process launch --device <id> --console <bundle id>`
8. Read the result: `xcrun devicectl device copy from ... --source
   Documents/qualify/result.txt --destination /tmp/result.txt`
