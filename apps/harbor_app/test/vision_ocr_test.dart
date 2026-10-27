// Vision OCR (T4, decision 0019): models.installed exposes roles, and
// ocr.open / ocr.release round-trip through the real core where it is
// available. The UI flow tests cover the Models-surface action; they
// never submit anything.
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/surfaces/models_surface.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

const _deviceRootHex =
    'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c';
final _dylibPath =
    '${Directory.current.parent.parent.path}/core/target/debug/libharbor_ffi.dylib';
final _coreAvailable = File(_dylibPath).existsSync();

Widget _host(HarborService service, {Widget? child}) => MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: harborThemeData(dark: false, arabic: false),
      builder: (_, child) => HarborTheme(
        colors: HarborColors.light,
        text: const HarborType(arabic: false),
        child: child!,
      ),
      home: HarborServiceProvider(
        service: service,
        failed: false,
        child: Scaffold(body: child ?? const SizedBox.shrink()),
      ),
    );

/// Pumps real time + fake timers until [done] is true.
Future<bool> _settle(WidgetTester tester, bool Function() done,
    {Duration budget = const Duration(seconds: 20)}) async {
  final deadline = DateTime.now().add(budget);
  while (DateTime.now().isBefore(deadline)) {
    await tester
        .runAsync(() => Future<void>.delayed(const Duration(milliseconds: 50)));
    await tester.pump(const Duration(milliseconds: 100));
    if (done()) return true;
  }
  return false;
}

/// Opens the Installed tab (index 3) and lets the tab animation finish.
Future<void> _openInstalled(WidgetTester tester) async {
  await tester.tap(find.text('Installed'));
  await tester.pump(const Duration(milliseconds: 400));
  await tester.pump(const Duration(milliseconds: 100));
}

/// Pins the test surface so the card and its buttons are on screen.
Future<void> _pinSurface(WidgetTester tester) async {
  tester.view.physicalSize = const Size(1400, 1000);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
}

void main() {
  group('vision ocr (real core, gated)', () {
    if (!_coreAvailable) {
      test('core dylib present', () {});
      return;
    }

    late ffi.HarborCoreClient core;
    late Directory dataRoot;
    setUpAll(() {
      dataRoot = Directory.systemTemp.createTempSync('harbor-vision-');
      core = ffi.HarborCoreClient.openEx(_dylibPath, dataRoot.path, 'ws-vision',
          ffi.HarborPrivacyMode.localOnly, _deviceRootHex);
    });
    tearDownAll(() {
      try {
        core.call('ocr.release');
      } on ffi.HarborCoreException {
        // not open
      }
      core.close();
      dataRoot.deleteSync(recursive: true);
    });

    void installVisionPackage(String id, Directory dir) {
      final weights = File('${dir.path}/$id-weights.gguf');
      weights.writeAsBytesSync([0x47, 0x47, 0x55, 0x46, 1, 2, 3]);
      final mmproj = File('${dir.path}/$id-mmproj.gguf');
      mmproj.writeAsBytesSync([9, 8, 7, 6, 5, 4]);
      core.call('models.install_from_path', {
        'package_id': id,
        'path': weights.path,
        'role': 'weights',
        'extra_files': [
          {'path': mmproj.path, 'role': 'mmproj'},
        ],
      });
    }

    test('a vision install surfaces roles in models.installed', () async {
      final dir = Directory.systemTemp.createTempSync('harbor-vision-pkg-');
      addTearDown(() => dir.deleteSync(recursive: true));
      installVisionPackage('vision-test-pkg', dir);
      final installed = core.call('models.installed');
      final pkg = (installed['models'] as List)
          .cast<Map>()
          .firstWhere((m) => m['id'] == 'vision-test-pkg');
      expect(pkg['has_projector'], isTrue);
    });

    test('ocr.open refuses a missing package with a typed error', () async {
      expect(() => core.call('ocr.open', {'package_id': 'no-such-package'}),
          throwsA(isA<ffi.HarborCoreException>()));
    });

    test('ocr.release is safe when nothing is loaded', () async {
      final r = core.call('ocr.release');
      expect(r['released'], isTrue);
    });
  });

  group('models surface scan action (fake service)', () {
    Map<String, dynamic> pkg(String id, List<String> roles) => {
          'id': id,
          'runtime': 'gguf/llama.cpp',
          'min_revision': 1,
          'files': roles.length,
          'total_bytes': 10,
          'roles': roles,
          'has_projector': roles.contains('mmproj'),
        };

    testWidgets('vision package shows Use for scanning and reports ready',
        (tester) async {
      final svc = _FakeService(models: [
        pkg('vision-pkg', ['weights', 'mmproj'])
      ], ocrResult: {
        'engine': 'vision.llava',
        'backend': 'gpu'
      });
      await _pinSurface(tester);
      await tester.pumpWidget(_host(svc, child: const ModelsSurface()));
      await tester.pump(const Duration(milliseconds: 50));
      await _openInstalled(tester);
      // Let surface entrance fades finish so the button accepts taps.
      await tester.pump(const Duration(milliseconds: 800));
      final button = find.byKey(const ValueKey('scan-use-vision-pkg'));
      expect(button, findsOneWidget);

      await tester.tap(button);
      final settled = await _settle(
          tester,
          () => find
              .text('Reading scans (GPU)')
              .evaluate()
              .isNotEmpty);
      expect(settled, isTrue);
      expect(find.text('Reading scans (GPU)'), findsOneWidget);
      expect(svc.submitCalls, 0,
          reason: 'opening the scanner must never submit a request');
    });

    testWidgets('plain package hides Use for scanning', (tester) async {
      final svc = _FakeService(models: [
        pkg('text-pkg', ['weights'])
      ]);
      await _pinSurface(tester);
      await tester.pumpWidget(_host(svc, child: const ModelsSurface()));
      await tester.pump(const Duration(milliseconds: 50));
      await _openInstalled(tester);
      await tester.pump(const Duration(milliseconds: 800));
      expect(find.byKey(const ValueKey('scan-use-text-pkg')), findsNothing);
    });

    testWidgets('typed failure surfaces verbatim', (tester) async {
      final svc = _FakeService(models: [
        pkg('vision-pkg', ['mmproj'])
      ], ocrError: 'load-verified failed: no vision projector');
      await _pinSurface(tester);
      await tester.pumpWidget(_host(svc, child: const ModelsSurface()));
      await tester.pump(const Duration(milliseconds: 50));
      await _openInstalled(tester);
      await tester.pump(const Duration(milliseconds: 800));
      await tester.tap(find.byKey(const ValueKey('scan-use-vision-pkg')));
      final settled = await _settle(
          tester,
          () =>
              find.textContaining('no vision projector').evaluate().isNotEmpty);
      expect(settled, isTrue);
      expect(find.textContaining('no vision projector'), findsOneWidget);
    });
  });
}

/// Service double for the UI flow tests: the OCR outcomes are scripted,
/// every other member is a canary — the flow under test must never
/// touch them (in particular, never submit a request).
class _FakeService with ChangeNotifier implements HarborService {
  _FakeService({this.models = const [], this.ocrResult, this.ocrError});

  final List<Map<String, dynamic>> models;
  final Map<String, dynamic>? ocrResult;
  final String? ocrError;
  int submitCalls = 0;

  @override
  List<Map<String, dynamic>> get installedModels => models;
  @override
  List<Map<String, dynamic>> get catalog => const [];
  @override
  bool get catalogImported => false;
  @override
  int get catalogEpoch => 0;
  @override
  Map<String, Map<String, dynamic>> get kindProgress => const {};

  @override
  Future<Map<String, dynamic>?> fitScore(String packageId) async => null;

  @override
  bool get knowledgeOpen => false;
  @override
  bool get needsFirstModel => false;
  @override
  List<Map<String, dynamic>> get runs => const [];
  @override
  List<Map<String, dynamic>> get activeOps => const [];
  @override
  List<SkillSummary> get skills => const [];
  @override
  String? get policyCode => 'LOCAL_ONLY';
  @override
  String? get executionHint => null;
  @override
  String? get policyVersion => null;
  @override
  String? get workspaceId => 'ws-vision';

  @override
  String? scanModel;
  @override
  String? scanBackend;

  @override
  Future<void> useForScanning(String packageId) async {
    final err = ocrError;
    if (err != null) throw ffi.HarborCoreException(err);
    scanModel = packageId;
    scanBackend = ocrResult!['backend'] as String?;
    notifyListeners();
  }

  @override
  Future<void> releaseScanModel() async {
    scanModel = null;
    scanBackend = null;
    notifyListeners();
  }

  @override
  Future<String?> submitRequest(String text) async {
    submitCalls++;
    return null;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('unexpected service call: ${invocation.memberName}');
}
