import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

/// Video -> sampled JPEG frames through an installed FFmpeg (Windows and
/// Linux, where the app has no platform decoder of its own).
///
/// Harbor does not bundle FFmpeg: its codec set carries patent and licence
/// obligations that belong to whoever ships the decoder. The user's own
/// install is used instead, found via `HARBOR_FFMPEG` / `HARBOR_FFPROBE`,
/// next to the app executable, or on PATH.
///
/// Safety: the video path is only ever a single argv element after `-i`
/// (no shell, so no quoting hazards, and a path starting with `-` cannot be
/// read as an option); `-protocol_whitelist file` stops a crafted file from
/// making FFmpeg open a network URL; every call has a timeout.

/// Result of one process run (injectable for tests).
class FfmpegProcessResult {
  const FfmpegProcessResult(this.exitCode, this.stdout, this.stderr);
  final int exitCode;
  final List<int> stdout;
  final String stderr;
}

typedef FfmpegRunner = Future<FfmpegProcessResult> Function(
    String executable, List<String> args, Duration timeout);

/// Default runner: no shell, binary stdout, hard timeout.
Future<FfmpegProcessResult> runProcess(
    String executable, List<String> args, Duration timeout) async {
  final p = await Process.start(executable, args, runInShell: false);
  final out = <int>[];
  final err = StringBuffer();
  final outDone = p.stdout.listen(out.addAll).asFuture<void>();
  final errDone = p.stderr
      .transform(const Utf8Decoder(allowMalformed: true))
      .listen(err.write)
      .asFuture<void>();
  try {
    final code = await p.exitCode.timeout(timeout);
    await outDone;
    await errDone;
    return FfmpegProcessResult(code, out, err.toString());
  } on Exception {
    p.kill(ProcessSignal.sigkill);
    rethrow;
  }
}

class FfmpegUnavailable implements Exception {
  @override
  String toString() =>
      'FFmpeg was not found (install it, or set HARBOR_FFMPEG)';
}

class FfmpegSampler {
  FfmpegSampler({
    FfmpegRunner runner = runProcess,
    Map<String, String>? environment,
    String? executableDir,
    bool? isWindows,
  })  : _run = runner,
        _env = environment ?? Platform.environment,
        _exeDir =
            executableDir ?? File(Platform.resolvedExecutable).parent.path,
        _windows = isWindows ?? Platform.isWindows;

  final FfmpegRunner _run;
  final Map<String, String> _env;
  final String _exeDir;
  final bool _windows;

  static const _probeTimeout = Duration(seconds: 20);
  static const _frameTimeout = Duration(seconds: 60);

  String get _suffix => _windows ? '.exe' : '';

  /// Candidate executables for [tool] in priority order.
  List<String> candidates(String tool) {
    final sep = _windows ? '\\' : '/';
    final override = _env['HARBOR_${tool.toUpperCase()}'];
    return [
      if (override != null && override.isNotEmpty) override,
      '$_exeDir$sep$tool$_suffix',
      tool, // PATH lookup by the OS
    ];
  }

  Future<String?> _find(String tool) async {
    for (final c in candidates(tool)) {
      try {
        final r =
            await _run(c, const ['-version'], const Duration(seconds: 10));
        if (r.exitCode == 0) return c;
      } on Exception {
        // Not there; try the next candidate.
      }
    }
    return null;
  }

  /// Whether both ffmpeg and ffprobe are usable.
  Future<bool> available() async =>
      await _find('ffmpeg') != null && await _find('ffprobe') != null;

  /// Up to [maxFrames] JPEG frames, evenly spaced across the video (about
  /// one per second, like the native samplers). Throws [FfmpegUnavailable]
  /// when FFmpeg is missing and [StateError] when the file is unreadable.
  Future<List<Uint8List>> sample(String path, {int maxFrames = 24}) async {
    final ffprobe = await _find('ffprobe');
    final ffmpeg = await _find('ffmpeg');
    if (ffprobe == null || ffmpeg == null) throw FfmpegUnavailable();

    final probe = await _run(
        ffprobe,
        [
          '-v',
          'error',
          '-protocol_whitelist',
          'file',
          '-show_entries',
          'format=duration',
          '-of',
          'default=noprint_wrappers=1:nokey=1',
          '-i',
          path,
        ],
        _probeTimeout);
    final duration = double.tryParse(utf8.decode(probe.stdout).trim());
    if (probe.exitCode != 0 ||
        duration == null ||
        !duration.isFinite ||
        duration <= 0) {
      throw StateError('unreadable video');
    }
    final count = maxFrames.clamp(1, 24).clamp(1, duration.ceil().clamp(1, 24));
    final frames = <Uint8List>[];
    for (var i = 0; i < count; i++) {
      final at = duration * (i + 0.5) / count;
      final r = await _run(
          ffmpeg,
          [
            '-nostdin', '-hide_banner', '-loglevel', 'error',
            '-protocol_whitelist', 'file',
            // -ss BEFORE -i seeks without decoding everything before it;
            // -i makes the path a plain argument, never an option.
            '-ss', at.toStringAsFixed(3),
            '-i', path,
            '-frames:v', '1',
            '-vf', 'scale=512:512:force_original_aspect_ratio=decrease',
            '-q:v', '3',
            '-f', 'image2pipe', '-vcodec', 'mjpeg', 'pipe:1',
          ],
          _frameTimeout);
      if (r.exitCode != 0 || r.stdout.isEmpty) {
        throw StateError(
            'no frame at ${at.toStringAsFixed(3)}s: ${r.stderr.trim()}');
      }
      frames.add(Uint8List.fromList(r.stdout));
    }
    return frames;
  }
}
