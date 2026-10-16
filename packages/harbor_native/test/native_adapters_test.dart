import 'dart:ffi';
import 'dart:io';

import 'package:harbor_native/harbor_worker.dart';
import 'package:test/test.dart';

final repoRoot = Directory.current.parent.parent.path;
final coreLib = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final ocrLib = '$repoRoot/native/apple/ocr_host/build/libharbor_ocr_host.dylib';
final asrLib = '$repoRoot/native/apple/asr_host/build/libharbor_asr_host.dylib';

List<int> addrs(DynamicLibrary lib, String prefix, String run) => [
      for (final n in [
        '${prefix}_descriptor',
        '${prefix}_$run',
        '${prefix}_free_string'
      ])
        lib.lookup<NativeFunction<Void Function()>>(n).address,
    ];

void main() {
  test('missing, short or null adapter addresses register nothing', () {
    if (!File(coreLib).existsSync()) fail('cargo build -p harbor_ffi first');
    final core = DynamicLibrary.open(coreLib);
    expect(registerNativeAdapters(core, const {}), isEmpty);
    expect(
        registerNativeAdapters(core, {
          'ocr': [1, 2],
          'asr': [0, 0, 0],
          'other': [1, 2, 3],
        }),
        isEmpty);
  });

  test('the Apple adapters register with the core exactly as the Runner does',
      () {
    if (!Platform.isMacOS ||
        !File(coreLib).existsSync() ||
        !File(ocrLib).existsSync() ||
        !File(asrLib).existsSync()) {
      markTestSkipped('needs macOS and the built host dylibs');
      return;
    }
    final core = DynamicLibrary.open(coreLib);
    final ocr = DynamicLibrary.open(ocrLib);
    final asr = DynamicLibrary.open(asrLib);
    final registered = registerNativeAdapters(core, {
      'ocr': addrs(ocr, 'harbor_ocr_host', 'recognize'),
      'asr': addrs(asr, 'harbor_asr_host', 'transcribe'),
    });
    expect(registered, contains('ocr'));
    // Speech needs the OS 26 model; on an older macOS it is refused, which is
    // the intended, non-fatal outcome.
    final v = Platform.operatingSystemVersion;
    if (RegExp(r'Version (2[6-9]|[3-9]\d)\.').hasMatch(v)) {
      expect(registered, contains('asr'));
    }
  });
}
