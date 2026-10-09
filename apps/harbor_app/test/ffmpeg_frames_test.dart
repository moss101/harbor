import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/ffmpeg_frames.dart';

/// The Windows / Linux video sampler. FFmpeg itself is not bundled, so these
/// tests pin the exact commands Harbor issues (and every failure path) with a
/// fake runner; the live test at the bottom runs the real thing wherever
/// FFmpeg happens to be installed.
class RecordedCall {
  RecordedCall(this.exe, this.args);
  final String exe;
  final List<String> args;
}

FfmpegSampler sampler(
  List<RecordedCall> calls, {
  double? duration = 4.2,
  int probeExit = 0,
  bool frameOk = true,
  bool toolsPresent = true,
  Map<String, String> env = const {},
  bool windows = false,
}) {
  return FfmpegSampler(
    environment: env,
    executableDir: windows ? r'C:\Harbor' : '/opt/harbor',
    isWindows: windows,
    runner: (exe, args, timeout) async {
      calls.add(RecordedCall(exe, args));
      if (!toolsPresent) throw Exception('not found');
      if (args.length == 1 && args.first == '-version') {
        return const FfmpegProcessResult(0, [], '');
      }
      if (exe.contains('ffprobe')) {
        return FfmpegProcessResult(probeExit,
            utf8.encode(duration == null ? 'N/A\n' : '$duration\n'), '');
      }
      return frameOk
          ? const FfmpegProcessResult(0, [0xFF, 0xD8, 1, 2, 0xFF, 0xD9], '')
          : const FfmpegProcessResult(1, [], 'Invalid data found');
    },
  );
}

void main() {
  test('tool lookup order: override, next to the app, then PATH', () {
    final s = sampler([], env: {'HARBOR_FFMPEG': '/custom/ffmpeg'});
    expect(s.candidates('ffmpeg'),
        ['/custom/ffmpeg', '/opt/harbor/ffmpeg', 'ffmpeg']);
    // No override set for ffprobe: the other two only.
    expect(s.candidates('ffprobe'), ['/opt/harbor/ffprobe', 'ffprobe']);
    final w = sampler([], windows: true);
    expect(w.candidates('ffmpeg'), [r'C:\Harbor\ffmpeg.exe', 'ffmpeg']);
  });

  test('samples ~1 frame per second, evenly spaced, with safe arguments',
      () async {
    final calls = <RecordedCall>[];
    final frames = await sampler(calls).sample('/videos/clip.mp4');
    expect(frames, hasLength(5)); // ceil(4.2 s)
    expect(frames.first.take(2), [0xFF, 0xD8]);

    final probe = calls.firstWhere(
        (c) => c.exe.contains('ffprobe') && c.args.contains('-show_entries'));
    expect(probe.args, containsAllInOrder(['-i', '/videos/clip.mp4']));
    expect(probe.args, containsAllInOrder(['-protocol_whitelist', 'file']));

    final grabs = calls.where((c) => c.args.contains('image2pipe')).toList();
    expect(grabs, hasLength(5));
    final times = [
      for (final g in grabs) double.parse(g.args[g.args.indexOf('-ss') + 1])
    ];
    // Midpoints of five equal slices of 4.2 s.
    for (var i = 0; i < 5; i++) {
      expect(times[i], closeTo(4.2 * (i + 0.5) / 5, 0.001));
    }
    for (final g in grabs) {
      // Seek BEFORE input (no decode-from-start), path only after -i,
      // no shell, no network protocols, no interactive stdin.
      expect(g.args.indexOf('-ss'), lessThan(g.args.indexOf('-i')));
      expect(g.args[g.args.indexOf('-i') + 1], '/videos/clip.mp4');
      expect(g.args, containsAllInOrder(['-protocol_whitelist', 'file']));
      expect(g.args, contains('-nostdin'));
      expect(g.args.last, 'pipe:1');
    }
  });

  test('a path that looks like an option stays a path', () async {
    final calls = <RecordedCall>[];
    await sampler(calls, duration: 1.0).sample('-rf /etc/passwd.mp4');
    final grab = calls.firstWhere((c) => c.args.contains('image2pipe'));
    final i = grab.args.indexOf('-i');
    expect(grab.args[i + 1], '-rf /etc/passwd.mp4');
    // It is exactly one argv element: never split, never reordered.
    expect(grab.args.where((a) => a.contains('passwd')), hasLength(1));
  });

  test('frame count is bounded by the request, the core limit and duration',
      () async {
    expect(await sampler([], duration: 600).sample('v.mp4'), hasLength(24));
    expect(await sampler([], duration: 600).sample('v.mp4', maxFrames: 6),
        hasLength(6));
    expect(await sampler([], duration: 0.3).sample('v.mp4'), hasLength(1));
  });

  test('failures are typed, never partial results', () async {
    expect(sampler([], toolsPresent: false).sample('v.mp4'),
        throwsA(isA<FfmpegUnavailable>()));
    expect(sampler([], duration: null).sample('v.mp4'),
        throwsA(isA<StateError>()));
    expect(
        sampler([], probeExit: 1).sample('v.mp4'), throwsA(isA<StateError>()));
    expect(sampler([], frameOk: false).sample('v.mp4'),
        throwsA(isA<StateError>()));
    expect(await sampler([], toolsPresent: false).available(), isFalse);
    expect(await sampler([]).available(), isTrue);
  });

  test('real FFmpeg, when installed: samples the committed MP4s', () async {
    final real = FfmpegSampler();
    if (!await real.available()) {
      // ignore: avoid_print
      print('SKIP: FFmpeg is not installed on this machine');
      return;
    }
    final frames = await real.sample('../../fixtures/media/chart_growth.mp4');
    expect(frames, hasLength(4)); // 4 s clip -> one frame per second
    for (final f in frames) {
      expect(f.take(2), [0xFF, 0xD8], reason: 'JPEG');
    }
    expect(frames.first, isNot(equals(frames.last)),
        reason: 'frames must differ (bars grow)');
  });
}
