import 'dart:io' show Platform;

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/native_adapters.dart';

/// The Dart side of the Runner's "dev.harbor/native_adapters" channel. The
/// Swift side is exercised by the iOS / macOS builds; this pins the contract
/// the core worker relies on.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('dev.harbor/native_adapters');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final apple = Platform.isMacOS || Platform.isIOS;

  tearDown(() => messenger.setMockMethodCallHandler(channel, null));

  test('addresses come back as plain ints per engine', () async {
    messenger.setMockMethodCallHandler(channel, (call) async {
      expect(call.method, 'addresses');
      return {
        'ocr': [10, 11, 12],
        'asr': [20, 21, 22],
      };
    });
    final got = await nativeAdapterAddresses();
    if (apple) {
      expect(got, {
        'ocr': [10, 11, 12],
        'asr': [20, 21, 22],
      });
    } else {
      expect(got, isEmpty, reason: 'only the Apple Runners carry adapters');
    }
  });

  test('a Runner without the channel or a failing channel yields no adapters',
      () async {
    expect(await nativeAdapterAddresses(), isEmpty); // MissingPlugin
    messenger.setMockMethodCallHandler(
        channel, (call) async => throw PlatformException(code: 'x'));
    expect(await nativeAdapterAddresses(), isEmpty);
  });
}
