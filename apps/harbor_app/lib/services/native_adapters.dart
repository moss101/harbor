import 'dart:io' show Platform;

import 'package:flutter/services.dart';

/// Addresses of the OS-engine adapters compiled into the Apple Runners (Apple
/// Vision OCR, on-device speech), for the core to register. Empty on
/// platforms without them, and when the channel is missing (tests, older
/// Runners): the core then reports "no OCR/speech engine" as a typed error
/// instead of pretending.
const _channel = MethodChannel('dev.harbor/native_adapters');

Future<Map<String, List<int>>> nativeAdapterAddresses() async {
  if (!(Platform.isIOS || Platform.isMacOS)) return const {};
  try {
    final raw =
        await _channel.invokeMapMethod<String, List<Object?>>('addresses');
    if (raw == null) return const {};
    return {
      for (final e in raw.entries)
        e.key: [for (final a in e.value) (a as num).toInt()],
    };
  } on MissingPluginException {
    return const {};
  } on PlatformException {
    return const {};
  }
}
