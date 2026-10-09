// EmbeddingGemma 2 on a PHYSICAL device (decision 0015 follow-up).
//
// A tiny app entry point that runs inside the real Harbor app (so the core
// is the exact static archive the app ships) and records what this
// device's GPU actually does: the numerics canary outcome, retrieval across
// EN/AR/FR, and timings. Results are shown on screen AND written to
// Documents/qualify/result.txt so they can be read back with devicectl.
//
// Build/run (see tools/device_qualification/README.md): copy this file to
// apps/harbor_app/tool_device/qualify_main.dart and
//   flutter build ios --release -t tool_device/qualify_main.dart
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:path_provider/path_provider.dart';

final _lines = ValueNotifier<List<String>>([]);
late File _out;

void say(String s) {
  // ignore: avoid_print
  print('QUALIFY $s');
  _lines.value = [..._lines.value, s];
  try {
    _out.writeAsStringSync('${_lines.value.join('\n')}\n');
  } catch (_) {}
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(MaterialApp(
    home: Scaffold(
      appBar: AppBar(title: const Text('Harbor device qualification')),
      body: ValueListenableBuilder<List<String>>(
        valueListenable: _lines,
        builder: (_, lines, __) => ListView(
          padding: const EdgeInsets.all(12),
          children: [for (final l in lines) Text(l)],
        ),
      ),
    ),
  ));
  final docs = await getApplicationDocumentsDirectory();
  final base = Directory('${docs.path}/qualify')..createSync(recursive: true);
  _out = File('${base.path}/result.txt');
  try {
    await _run(base);
  } catch (e, st) {
    say('FAILED $e\n$st');
  }
}

Future<void> _run(Directory base) async {
  say('device os: ${Platform.operatingSystemVersion}');
  final model = File('${base.path}/embeddinggemma-2-Q8_0.gguf');
  if (!model.existsSync()) {
    say('SKIP model not pushed to ${model.path}');
    return;
  }
  final data = Directory('${base.path}/data');
  if (data.existsSync()) data.deleteSync(recursive: true);
  data.createSync(recursive: true);
  final service = await HarborService.open(
    libraryPath: 'libharbor_ffi.dylib', // device: resolved in-process
    dataRoot: data.path,
    workspaceId: 'ws-device-qualify',
    deviceRootHex:
        'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c',
  );
  final t0 = DateTime.now();
  final installed = await service.installModelFromPath(
      packageId: 'embeddinggemma2-device', path: model.path);
  if (!installed) {
    say('RESULT FAIL model install failed');
    return;
  }
  say('install ${DateTime.now().difference(t0).inMilliseconds} ms');

  final t1 = DateTime.now();
  final opened =
      await service.openKnowledge(packageId: 'embeddinggemma2-device');
  say('open=$opened backend=${service.knowledgeBackend} '
      'dim=${service.knowledgeDimension} '
      '${DateTime.now().difference(t1).inMilliseconds} ms');
  if (!opened) {
    say('RESULT FAIL EmbeddingGemma 2 did not open on this device');
    return;
  }
  await service.ingestSources([
    {
      'id': 'contract',
      'title': 'Contract',
      'text': 'The contract value is 5000 USD and ends 2026-12-31.'
    },
    {
      'id': 'travel',
      'title': 'Travel',
      'text': 'Employees may claim up to 180 USD per night for hotels.'
    },
    {
      'id': 'leave',
      'title': 'Leave',
      'text': 'Annual leave entitlement is 24 days per year.'
    },
  ]);
  var correct = 0;
  const cases = {
    'What is the contract value in USD?': 'contract',
    'How much can I claim for a hotel night?': 'travel',
    'How many vacation days do I get?': 'leave',
    'كم عدد أيام الإجازة السنوية؟': 'leave',
    'Quelle est la valeur du contrat ?': 'contract',
  };
  for (final e in cases.entries) {
    final t = DateTime.now();
    final r = await service.searchKnowledge(e.key, topK: 3);
    final c = (r?['citations'] as List?)?.cast<Map>() ?? const [];
    final top = c.isEmpty ? null : c.first['source_id'];
    final score = c.isEmpty ? null : c.first['score'];
    if (top == e.value) correct++;
    say('query "${e.key}" -> $top (want ${e.value}) score=$score '
        '${DateTime.now().difference(t).inMilliseconds} ms');
  }
  say('RESULT ${correct == cases.length ? 'PASS' : 'FAIL'} '
      'backend=${service.knowledgeBackend} correct=$correct/${cases.length}');
  service.close();

  final proj = File('${base.path}/mmproj-embeddinggemma-2-Q8_0.gguf');
  if (!proj.existsSync()) {
    say('MM SKIP projector not pushed');
    return;
  }
  await _runMultimodal(base, model, proj);
}

/// Images, audio and video (as sampled frames) indexed ON THE DEVICE next to
/// text, then retrieved by text and by image. PASS = every expected item is
/// the top result.
Future<void> _runMultimodal(Directory base, File model, File proj) async {
  final media = Directory('${base.path}/media');
  File m(String n) => File('${media.path}/$n');
  final data = Directory('${base.path}/data_mm');
  if (data.existsSync()) data.deleteSync(recursive: true);
  data.createSync(recursive: true);
  final service = await HarborService.open(
    libraryPath: 'libharbor_ffi.dylib',
    dataRoot: data.path,
    workspaceId: 'ws-device-mm',
    deviceRootHex:
        'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c',
  );
  final t0 = DateTime.now();
  final installed = await service.installModelFromPath(
    packageId: 'embeddinggemma2-mm-device',
    path: model.path,
    extraFiles: [
      {'path': proj.path, 'role': 'mmproj'},
    ],
  );
  say('MM install ok=$installed ${DateTime.now().difference(t0).inMilliseconds} ms');
  if (!installed) {
    say('MM RESULT FAIL package install failed');
    return;
  }
  final opened =
      await service.openKnowledge(packageId: 'embeddinggemma2-mm-device');
  say('MM open=$opened multimodal=${service.knowledgeMultimodal} '
      'backend=${service.knowledgeBackend}');
  if (!opened || !service.knowledgeMultimodal) {
    say('MM RESULT FAIL media towers not available');
    return;
  }
  Future<void> timed(String what, Future<void> Function() f) async {
    final t = DateTime.now();
    await f();
    say('MM index $what ${DateTime.now().difference(t).inMilliseconds} ms');
  }

  for (final n in ['invoice.jpg', 'landscape.jpg', 'chart.jpg']) {
    await timed(n, () async {
      await service.ingestMedia(
          bytes: await m(n).readAsBytes(), kind: 'image', title: n);
    });
  }
  for (final n in ['speech_revenue.wav', 'speech_weather.wav']) {
    await timed(n, () async {
      await service.ingestMedia(
          bytes: await m(n).readAsBytes(), kind: 'audio', title: n);
    });
  }
  for (final v in ['chart_growth', 'landscape_sun']) {
    await timed('$v (video, 4 frames)', () async {
      final frames = [
        for (var i = 0; i < 4; i++) await m('${v}_frame$i.jpg').readAsBytes()
      ];
      await service.ingestVideo(frames: frames, title: '$v.mp4');
    });
  }

  Future<String?> top(Map<String, dynamic>? r) async {
    final c = (r?['citations'] as List?)?.cast<Map>() ?? const [];
    return c.isEmpty ? null : c.first['title'] as String?;
  }

  var ok = 0, total = 0;
  Future<void> check(String label, String want, Map<String, dynamic>? r) async {
    total++;
    final got = await top(r);
    if (got == want) ok++;
    final c = (r?['citations'] as List?)?.cast<Map>() ?? const [];
    say('MM $label -> $got (want $want) '
        'score=${c.isEmpty ? null : c.first['score']}');
  }

  const textCases = {
    'an invoice with the total amount due': 'invoice.jpg',
    'a sunny landscape with blue sky and green grass': 'landscape.jpg',
    'a bar chart with four columns': 'chart.jpg',
    'a spoken report about quarterly revenue and sales growth':
        'speech_revenue.wav',
    'a spoken forecast of sunny weather for a walk in the park':
        'speech_weather.wav',
    'a video of bars growing taller in a chart': 'chart_growth.mp4',
    'a video of a bright sun crossing the sky over green grass':
        'landscape_sun.mp4',
  };
  for (final e in textCases.entries) {
    final t = DateTime.now();
    final r = await service.searchKnowledge(e.key, topK: 3);
    await check('text "${e.key}"', e.value, r);
    say('MM   query took ${DateTime.now().difference(t).inMilliseconds} ms');
  }
  // Search BY an image: the invoice picture must find itself first.
  final t = DateTime.now();
  final byImage = await service.searchByMedia(
      await m('invoice.jpg').readAsBytes(), 'image',
      topK: 3);
  await check('image query invoice.jpg', 'invoice.jpg', byImage);
  say('MM   image query took ${DateTime.now().difference(t).inMilliseconds} ms');
  say('MM RESULT ${ok == total ? 'PASS' : 'FAIL'} '
      'backend=${service.knowledgeBackend} correct=$ok/$total');
  service.close();
}
