import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/video_frames.dart';

/// The Dart side of the native video sampler (AVFoundation on macOS/iOS,
/// MediaMetadataRetriever on Android). The native code is exercised by the
/// app builds; this pins the channel contract the Knowledge surface relies
/// on.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('dev.harbor.video_frames');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;

  tearDown(() => messenger.setMockMethodCallHandler(channel, null));

  test('requests a bounded sample and returns the frames as bytes', () async {
    Map? seen;
    messenger.setMockMethodCallHandler(channel, (call) async {
      expect(call.method, 'sample');
      seen = call.arguments as Map;
      return [
        Uint8List.fromList([1, 2, 3]),
        Uint8List.fromList([4, 5])
      ];
    });
    final frames = await sampleVideoFrames('/tmp/clip.mp4', maxFrames: 999);
    expect(frames, hasLength(2));
    expect(frames!.first, [1, 2, 3]);
    expect(seen!['path'], '/tmp/clip.mp4');
    // The core accepts at most maxVideoFrames; the request is clamped so a
    // caller can never ask for more than will be accepted.
    expect(seen!['maxFrames'], maxVideoFrames);
  });

  test('a platform without a sampler reports null, not an exception', () async {
    // No handler registered = MissingPluginException (Windows / Linux).
    expect(await sampleVideoFrames('/tmp/clip.mp4'), isNull);
  });

  test('an unreadable video surfaces as a PlatformException', () async {
    messenger.setMockMethodCallHandler(channel, (call) async {
      throw PlatformException(code: 'video', message: 'unreadable video');
    });
    expect(
        sampleVideoFrames('/tmp/bad.mp4'), throwsA(isA<PlatformException>()));
  });
}
