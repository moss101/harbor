import 'dart:async';

import 'package:flutter/services.dart';

/// Live capture from the device: record the microphone, take a photo.
///
/// Native side: `dev.harbor/capture` in the iOS / macOS Runners (Swift) and in
/// MainActivity (Kotlin). Everything stays on the device; the files it returns
/// live in the app's temporary directory and are handed straight to the core's
/// OCR / speech reads. A platform without the channel (Windows, Linux, tests)
/// simply reports nothing is supported - the UI then offers files only.
class CaptureSupport {
  const CaptureSupport(
      {this.record = false, this.photo = false, this.transcribe = false});
  final bool record;
  final bool photo;

  /// The platform transcribes a recording made here itself (Android's
  /// on-device recognizer). Apple platforms and desktops use the core's own
  /// speech engine instead.
  final bool transcribe;
}

/// The person refused (or the OS blocked) microphone / camera access.
class CapturePermissionDenied implements Exception {
  const CapturePermissionDenied(this.what);
  final String what;
  @override
  String toString() => 'permission denied: $what';
}

/// The platform did not answer in time.
class CaptureUnresponsive implements Exception {
  const CaptureUnresponsive(this.what);
  final String what;
  @override
  String toString() => '$what did not respond';
}

class HarborCapture {
  HarborCapture([MethodChannel? channel])
      : _channel = channel ?? const MethodChannel('dev.harbor/capture');
  final MethodChannel _channel;

  Future<CaptureSupport> support() async {
    try {
      final m = await _channel.invokeMapMethod<String, Object?>('support');
      return CaptureSupport(
          record: m?['record'] == true,
          photo: m?['photo'] == true,
          transcribe: m?['transcribe'] == true);
    } on MissingPluginException {
      return const CaptureSupport();
    } on PlatformException {
      return const CaptureSupport();
    }
  }

  /// Start recording. Throws [CapturePermissionDenied] when microphone access
  /// is refused.
  Future<void> recordStart() => _guard('microphone', () async {
        // A native call that never answers must not leave the flow silent;
        // on timeout the pending start is cancelled so a late success cannot
        // leave the microphone open.
        try {
          await _channel
              .invokeMethod<void>('recordStart')
              .timeout(const Duration(seconds: 20));
        } on TimeoutException {
          await recordCancel();
          throw const CaptureUnresponsive('microphone');
        }
      });

  /// Stop and return the path of the recorded file.
  Future<String> recordStop() => _guard('microphone', () async {
        final path = await _channel.invokeMethod<String>('recordStop');
        if (path == null || path.isEmpty) {
          throw PlatformException(code: 'empty', message: 'no recording');
        }
        return path;
      });

  /// Discard an in-progress recording (the dialog was cancelled).
  Future<void> recordCancel() async {
    try {
      await _channel.invokeMethod<void>('recordCancel');
    } on PlatformException {
      // Nothing was recording.
    } on MissingPluginException {
      // No native side.
    }
  }

  /// Transcribe a recording made by [recordStop] with the platform's
  /// on-device recognizer: `{engine, locale, text}`.
  Future<Map<String, dynamic>> transcribe(String path, String locale) =>
      _guard('speech', () async {
        final m = await _channel.invokeMapMethod<String, Object?>(
            'transcribe', {'path': path, 'locale': locale});
        return {...?m};
      });

  Future<void> transcribeCancel() async {
    try {
      await _channel.invokeMethod<void>('transcribeCancel');
    } on PlatformException {
      // Nothing was running.
    } on MissingPluginException {
      // No native side.
    }
  }

  /// A photo path, or null when the person cancelled the camera.
  Future<String?> takePhoto() => _guard('camera', () async {
        return _channel.invokeMethod<String>('takePhoto');
      });

  Future<T> _guard<T>(String what, Future<T> Function() body) async {
    try {
      return await body();
    } on PlatformException catch (e) {
      if (e.code == 'permission') throw CapturePermissionDenied(what);
      rethrow;
    }
  }
}
