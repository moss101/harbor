import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/surfaces/settings_surface.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// The GPU qualification store (decision 0022) through the real core: a
/// pinned record written the way the core writes it shows in Settings with
/// its reason, and "Re-test the GPU" clears it.
final repoRoot = Directory.current.parent.parent.path;
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('a crashed GPU key is listed with its reason and can be reset',
      (tester) async {
    if (!File(dylibPath).existsSync()) return;
    late HarborService service;
    late Directory dir;
    await tester.runAsync(() async {
      dir = Directory.systemTemp.createTempSync('harbor-gpu-qual-test-');
      final runtime = Directory('${dir.path}/models/.runtime')
        ..createSync(recursive: true);
      File('${runtime.path}/qualification.json').writeAsStringSync(jsonEncode({
        'version': 1,
        'records': {
          '0123456789abcdef': {
            'key': {
              'backend': 'Vulkan',
              'device': 'Adreno (TM) 829',
              'platform': 'android-aarch64/test',
              'model_sha256': 'ab' * 32,
              'runtime': 'llama.cpp/test',
            },
            'state': 'crashed',
            'interrupted': 0,
            'note': 'process died while loading on the GPU',
            'updated': '2026-11-02T00:00:00Z',
          }
        }
      }));
      service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-gpu-qual',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c',
      );
      final q = await service.runtimeQualification();
      final records = (q['records'] as List).cast<Map>();
      expect(records, hasLength(1));
      expect(records.single['state'], 'crashed');
      expect(records.single['pins_cpu'], true);
    });
    addTearDown(() {
      service.close();
      dir.deleteSync(recursive: true);
    });

    await tester.pumpWidget(HarborTheme(
      colors: HarborColors.light,
      text: const HarborType(arabic: false),
      child: MaterialApp(
        localizationsDelegates: const [
          AppLocalizations.delegate,
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        supportedLocales: const [Locale('en')],
        home: Scaffold(body: GpuQualificationCard(service: service)),
      ),
    ));
    await tester
        .runAsync(() => Future.delayed(const Duration(milliseconds: 300)));
    await tester.pump();
    expect(find.textContaining('the GPU crashed'), findsOneWidget);
    expect(find.textContaining('Adreno (TM) 829'), findsOneWidget);

    await tester.tap(find.text('Re-test the GPU'));
    await tester
        .runAsync(() => Future.delayed(const Duration(milliseconds: 300)));
    await tester.pump();
    await tester.runAsync(() async {
      final q = await service.runtimeQualification();
      expect(q['records'], isEmpty);
    });
    expect(find.text('Re-test the GPU'), findsNothing);
  });

  testWidgets(
      'LIVE models run in the inference worker when its launcher ships '
      'beside the core (auto mode)', (tester) async {
    final launcher = File('$repoRoot/core/target/debug/harbor_worker');
    final model = '$repoRoot/fixtures/models/bge-small-en-v1.5-q8_0.gguf';
    if (!File(dylibPath).existsSync() ||
        !launcher.existsSync() ||
        !File(model).existsSync()) {
      return;
    }
    await tester.runAsync(() async {
      final dir = Directory.systemTemp.createTempSync('harbor-worker-mode-');
      final service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-worker-mode',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c',
      );
      addTearDown(() {
        service.close();
        dir.deleteSync(recursive: true);
      });
      expect(
          await service.installModelFromPath(
              packageId: 'bge-small-en-v1.5-q8_0', path: model),
          isTrue);
      expect(await service.openKnowledge(packageId: 'bge-small-en-v1.5-q8_0'),
          isTrue);
      final q = await service.runtimeQualification();
      expect(q['inference_mode'], 'worker',
          reason: 'fell back: ${q['inference_mode_detail']}');
    });
  });
}
