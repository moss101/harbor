import 'dart:io' show Platform;

import 'package:flutter/services.dart';

import 'ffmpeg_frames.dart';

/// Video -> sampled JPEG frames, for multimodal indexing.
///
/// The Rust core carries no video decoder, and the EmbeddingGemma 2 model
/// takes video as sampled frames (about 1 per second), so the platform
/// decodes. macOS / iOS use AVFoundation, Android uses
/// MediaMetadataRetriever (no third-party dependency). Windows and Linux use
/// the user's own FFmpeg install when there is one (see ffmpeg_frames.dart);
/// without it video is reported as unavailable, not silently skipped.
const _channel = MethodChannel('dev.harbor.video_frames');

/// Largest sample the core accepts per video (see MAX_VIDEO_FRAMES).
const maxVideoFrames = 24;

bool get videoSamplingSupported =>
    Platform.isMacOS ||
    Platform.isIOS ||
    Platform.isAndroid ||
    Platform.isWindows ||
    Platform.isLinux;

/// Whether video can be sampled RIGHT NOW. On Windows / Linux that depends
/// on FFmpeg being installed, so it is checked, not assumed.
Future<bool> videoSamplingAvailable({FfmpegSampler? ffmpeg}) async {
  if (Platform.isWindows || Platform.isLinux) {
    return (ffmpeg ?? FfmpegSampler()).available();
  }
  return videoSamplingSupported;
}

/// Up to [maxFrames] evenly spaced JPEG frames, or null when this
/// platform cannot sample video. Throws [PlatformException] when the file
/// is unreadable.
Future<List<Uint8List>?> sampleVideoFrames(
  String path, {
  int maxFrames = maxVideoFrames,
  FfmpegSampler? ffmpeg,
}) async {
  try {
    final frames = await _channel.invokeListMethod<Uint8List>('sample', {
      'path': path,
      'maxFrames': maxFrames.clamp(1, maxVideoFrames),
    });
    return frames;
  } on MissingPluginException {
    // No native sampler on this platform. Windows / Linux fall back to the
    // user's FFmpeg; anywhere else (or without FFmpeg) video is unavailable.
    if (Platform.isWindows || Platform.isLinux) {
      try {
        return await (ffmpeg ?? FfmpegSampler())
            .sample(path, maxFrames: maxFrames.clamp(1, maxVideoFrames));
      } on FfmpegUnavailable {
        return null;
      }
    }
    return null;
  }
}
