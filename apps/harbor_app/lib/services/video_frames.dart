import 'dart:io' show Platform;

import 'package:flutter/services.dart';

/// Video -> sampled JPEG frames, for multimodal indexing.
///
/// The Rust core carries no video decoder, and the EmbeddingGemma 2 model
/// takes video as sampled frames (about 1 per second), so the platform
/// decodes. macOS / iOS use AVFoundation, Android uses
/// MediaMetadataRetriever (no third-party dependency). Windows and Linux
/// have no sampler: [videoSamplingSupported] is false there and the UI
/// does not offer video.
const _channel = MethodChannel('dev.harbor.video_frames');

/// Largest sample the core accepts per video (see MAX_VIDEO_FRAMES).
const maxVideoFrames = 24;

bool get videoSamplingSupported =>
    Platform.isMacOS || Platform.isIOS || Platform.isAndroid;

/// Up to [maxFrames] evenly spaced JPEG frames, or null when this
/// platform cannot sample video. Throws [PlatformException] when the file
/// is unreadable.
Future<List<Uint8List>?> sampleVideoFrames(
  String path, {
  int maxFrames = maxVideoFrames,
}) async {
  try {
    final frames = await _channel.invokeListMethod<Uint8List>('sample', {
      'path': path,
      'maxFrames': maxFrames.clamp(1, maxVideoFrames),
    });
    return frames;
  } on MissingPluginException {
    return null;
  }
}
