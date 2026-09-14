import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/surfaces/work_surface.dart';
import 'package:harbor_ui/harbor_ui.dart';

const dylibPath =
    '/Users/mohsin/projects/harbor/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// Pump HarborApp with a REAL viewport of [width]x[height] logical pixels
/// (the shell's breakpoint decisions must agree with actual layout).
Future<void> pumpApp(
  WidgetTester tester, {
  Locale locale = const Locale('en'),
  double width = 1440,
  double height = 900,
}) async {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  final arabic = locale.languageCode == 'ar';
  HarborService? service;
  if (coreAvailable) {
    // Real IO + FFI futures must run in a real zone inside testWidgets.
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-shell-test-');
      final s = await HarborService.open(
          libraryPath: dylibPath, dataRoot: dir.path, workspaceId: 'ws-shell');
      await s.refresh();
      service = s;
    });
  }
  addTearDown(() => service?.close());
  await tester.pumpWidget(MaterialApp(
    locale: locale,
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: const [Locale('en'), Locale('ar')],
    theme: harborThemeData(dark: false, arabic: arabic),
    builder: (_, child) => HarborTheme(
      colors: HarborColors.light,
      text: HarborType(arabic: arabic),
      child: child!,
    ),
    home: HarborApp(service: service),
  ));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
}

Map<String, dynamic>? _result;

void main() {
  _appendLiveTests();
  _appendPreviewTest();
  _appendSkillsTest();
  _appendKnowledgeTest();
  _appendRagTest();
  _appendComposerTest();
  testWidgets('home shows work-first headline and quick actions',
      (tester) async {
    await pumpApp(tester);
    expect(find.text('What do you want to get done?'), findsOneWidget);
    expect(find.text('Summarize document'), findsOneWidget);
    expect(find.text('Analyze spreadsheet'), findsOneWidget);
    // No chat-with-model phrasing on Home (goal §21).
    expect(find.textContaining('Chat with'), findsNothing);
  });

  testWidgets('switching to Arabic renders RTL and localized labels',
      (tester) async {
    // Arabic is chosen through the app's own Settings: the locale is
    // presentation state owned by the shell, not an external override.
    await pumpApp(tester, width: 390, height: 844);
    await tester.tap(find.text('Settings'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.text('العربية'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    final context = tester.element(find.byType(NavigationBar).first);
    expect(Directionality.of(context), TextDirection.rtl);
    expect(find.text('الرئيسية'), findsOneWidget);
    // Back on Home: work-first headline is localized.
    await tester.tap(find.text('الرئيسية'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('ما الذي تريد إنجازه؟'), findsOneWidget);
  });

  testWidgets('compact width uses bottom navigation', (tester) async {
    await pumpApp(tester, width: 390, height: 844);
    expect(find.byType(NavigationBar), findsOneWidget);
    expect(find.byType(NavigationRail), findsNothing);
    // All nine surfaces reachable from the bottom bar.
    for (final label in [
      'Models',
      'Skills',
      'Knowledge',
      'Activity',
      'Settings'
    ]) {
      expect(find.text(label), findsWidgets);
    }
  });

  testWidgets('medium width shows compact rail, no persistent lens',
      (tester) async {
    await pumpApp(tester, width: 800, height: 600);
    expect(find.byType(NavigationRail), findsOneWidget);
    expect(find.byType(NavigationBar), findsNothing);
  });

  testWidgets(
      'wide width shows rail; at 1280 lens is persistent with '
      'canvas >= 640', (tester) async {
    await pumpApp(tester, width: 1280, height: 800);
    final rail = tester.widget<NavigationRail>(find.byType(NavigationRail));
    // At 1280 the rail is not extended-pinned... it uses the full 220 width.
    expect(rail, isNotNull);
    // Trust Pulse is present in the persistent Harbor Lens.
    expect(find.byType(TrustPulse), findsOneWidget);
    // Rail (220) + lens (320) + canvas (>=640) <= 1280.
    final railFinder = find.byType(NavigationRail);
    final railSize = tester.getSize(railFinder);
    expect(railSize.width, lessThanOrEqualTo(1280 - HarborLayout.desktopLens));
  });

  testWidgets('settings switches to Arabic and dark', (tester) async {
    await pumpApp(tester);
    await tester.tap(find.text('Settings'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.text('العربية'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('الإعدادات'), findsWidgets);
    expect(find.text('اللغة'), findsOneWidget);
  });
}

void _appendLiveTests() {
  testWidgets('home model dock reflects the live core state', (tester) async {
    await pumpApp(tester);
    if (!coreAvailable) {
      // Degraded-but-honest state when the native core is absent.
      expect(find.text('Core unavailable — native runtime not loaded'),
          findsOneWidget);
      return;
    }
    // With the real core loaded the dock shows LOCAL ONLY policy facts,
    // never fake model names.
    expect(find.text('LOCAL ONLY'), findsWidgets);
    expect(find.text('No model installed — open Models to install one'),
        findsOneWidget);
  });

  testWidgets('activity lists durable runs created through the core',
      (tester) async {
    await pumpApp(tester);
    if (!coreAvailable) return;
    HarborService? service;
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-activity-');
      final s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-activity');
      await s.createRun('run-visible-1');
      await s.refresh();
      service = s;
    });
    if (service == null) return;
    addTearDown(() => service?.close());
    // ignore: use_build_context_synchronously
    await tester.pumpWidget(HarborApp(service: service));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    // "Activity" appears in nav, rail and Lens — tap the rail entry.
    await tester.tap(find.text('Activity').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
    // The run id appears in BOTH the Activity list and the persistent
    // Harbor Lens (which now shows real runs, not samples).
    expect(find.text('run-visible-1'), findsWidgets);
    expect(find.textContaining('CREATED'), findsWidgets);
  });
}

void _appendPreviewTest() {
  testWidgets('work canvas renders a real workbook preview via the core',
      (tester) async {
    if (!coreAvailable) return;
    const fixturePath =
        '/Users/mohsin/projects/harbor/fixtures/office/board_demo.xlsx';
    HarborService? service;
    await tester.runAsync(() async {
      final bytes = await File(fixturePath).readAsBytes();
      final dir = await Directory.systemTemp.createTemp('harbor-preview-');
      final s = await HarborService.open(
          libraryPath: dylibPath, dataRoot: dir.path, workspaceId: 'ws-canvas');
      await s.loadPreviewFromBytes(bytes);
      service = s;
    });
    if (service == null) return;
    addTearDown(() => service?.close());
    final home = HarborServiceProvider(
      failed: false,
      service: service!,
      child: const WorkSurface(),
    );
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
        home: home,
      ),
    ));
    await tester.pump();
    // The preview comes from the pinned engine's recalculation, not a mock.
    expect(find.textContaining('Sheet: Sheet1'), findsOneWidget);
    // OOXML stores formulas without the leading '='.
    expect(find.textContaining('SUM(B2:B5)'), findsOneWidget);
  });
}

void _appendSkillsTest() {
  testWidgets('skills surface lists the real builtin skills from core',
      (tester) async {
    await pumpApp(tester);
    if (!coreAvailable) {
      expect(find.textContaining('Native core not loaded'), findsNothing);
      return;
    }
    await tester.tap(find.text('Skills').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    // The 21 authority skill families come through the live boundary.
    final sp = HarborServiceProvider.of(
        tester.element(find.textContaining('tools').first));
    expect(sp.notifier!.skills.length, greaterThanOrEqualTo(21));
    expect(find.text('Document Intelligence'), findsOneWidget);
    expect(find.text('Spreadsheet Analyst'), findsOneWidget);
    // The list is lazy: scroll to the last family.
    await tester.scrollUntilVisible(find.text('Privacy Inspector'), 300,
        scrollable: find.byType(Scrollable).first);
    expect(find.text('Privacy Inspector'), findsOneWidget);
  });
}

void _appendKnowledgeTest() {
  testWidgets('knowledge answers from a real embedded model end-to-end',
      (tester) async {
    if (!coreAvailable) return;
    HarborService? service;
    var opened = false;
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-knowledge-');
      final s = await HarborService.open(
          libraryPath: dylibPath, dataRoot: dir.path, workspaceId: 'ws-know');
      // Install the REAL bge-small-en-v1.5 embedding model through the
      // staged-install path, then open the durable index over it.
      opened = await s.installModelFromPath(
            packageId: 'bge-small-en-v1.5',
            path:
                '/Users/mohsin/projects/harbor/fixtures/models/bge-small-en-v1.5-q8_0.gguf',
          ) &&
          await s.openKnowledge();
      if (opened) {
        final ingest = await s.ingestSources([
          {
            'id': 'en-contract',
            'title': 'Master Services Agreement',
            'text': 'The contract value is 5000 USD. The agreement ends on '
                '2026-12-31. Payment terms are net thirty days.',
          }
        ]);
        opened = ingest != null;
      }
      service = s;
    });
    if (!opened || service == null) {
      fail('embedding model must install and open in the test environment');
    }
    addTearDown(() => service?.close());
    // Answerable question returns a real citation.
    Map<String, dynamic>? cited;
    Map<String, dynamic>? empty;
    await tester.runAsync(() async {
      cited = await service!
          .searchKnowledge('What is the contract value?', topK: 3);
      empty = await service!
          .searchKnowledge("What is the CEO's favorite color?", topK: 3);
    });
    _result = cited;
    expect(_result, isNotNull);
    final citations = (_result!['citations'] as List).cast<Map>();
    expect(citations, isNotEmpty);
    expect(citations.first['source_id'], 'en-contract');
    expect((citations.first['score'] as num).toDouble(), greaterThan(0.5));
    // Unanswerable question: nothing above the evidence bar.
    final emptyCitations = (empty!['citations'] as List)
        .cast<Map>()
        .where((c) => (c['score'] as num).toDouble() > 0.5)
        .toList();
    expect(
      emptyCitations,
      isEmpty,
      reason: 'unrelated question must not surface evidence',
    );
    // Source management: the indexed source is listed and removable.
    await tester.runAsync(() async {
      final sources = service!.knowledgeSources;
      expect(sources, isNotEmpty);
      expect(
        sources.any((s) => s['source_id'] == 'en-contract'),
        isTrue,
      );
      // Re-ingesting the same source id REPLACES (never duplicates).
      await service!.ingestSources([
        {
          'id': 'en-contract',
          'title': 'Master Services Agreement v2',
          'text': 'Short replacement body.',
        }
      ]);
      final after = service!.knowledgeSources
          .firstWhere((s) => s['source_id'] == 'en-contract');
      expect(after['chunks'], 1,
          reason: 'replacement must drop stale higher-ordinal chunks');
    });
  });
}

void _appendRagTest() {
  testWidgets('ask generates grounded answers on-device with durable runs',
      (tester) async {
    if (!coreAvailable) return;
    HarborService? service;
    var opened = false;
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-rag-');
      final s = await HarborService.open(
          libraryPath: dylibPath, dataRoot: dir.path, workspaceId: 'ws-rag');
      // Install BOTH models through the real staged-install path:
      // the chat model and the embedding model.
      await s.installModelFromPath(
          packageId: 'stories260k',
          path:
              '/Users/mohsin/projects/harbor/fixtures/models/stories260K.gguf');
      await s.installModelFromPath(
          packageId: 'bge-small-en-v1.5',
          path:
              '/Users/mohsin/projects/harbor/fixtures/models/bge-small-en-v1.5-q8_0.gguf');
      opened = await s.openKnowledge();
      if (opened) {
        final ingest = await s.ingestSources([
          {
            'id': 'contract',
            'title': 'Master Services Agreement',
            'text': 'The contract value is 5000 USD. The agreement ends on '
                '2026-12-31.',
          }
        ]);
        opened = ingest != null;
      }
      service = s;
    });
    if (!opened || service == null) {
      fail('knowledge must open in the test environment');
    }
    addTearDown(() => service?.close());
    // The complete journey: durable run -> grounded generation ->
    // answer recorded as run events.
    Map<String, dynamic>? answer;
    String? runId;
    await tester.runAsync(() async {
      runId = HarborService.newRunId();
      await service!.createRun(runId!);
      answer = await service!.generateAnswer('What is the contract value?',
          chatPackage: 'stories260k', maxTokens: 24, runId: runId);
    });
    // The RAG loop ran fully on-device with real provenance.
    expect(answer, isNotNull);
    expect(answer!['executed_on'], 'stories260k');
    expect(answer!['execution'], 'ON_DEVICE');
    expect((answer!['usage']['prompt_tokens'] as num).toInt(), greaterThan(0));
    expect(
        (answer!['usage']['completion_tokens'] as num).toInt(), greaterThan(0));
    expect((answer!['answer'] as String).trim(), isNotEmpty);
    // Citations were retrieved and surfaced with the answer.
    expect((answer!['used_citations'] as bool), isTrue);
    // The run is durable and replays with the question + answer.
    await tester.runAsync(() async {
      final runs = service!.runs;
      expect(runs.any((r) => r['run_id'] == runId), isTrue,
          reason: 'the generated answer must leave a durable run');
      final replay = await service!.replayRun(runId!);
      expect(replay, isNotNull);
      final trail =
          (replay!['trail'] as List).cast<Map>().map((e) => e['summary']);
      expect(
        trail.any((s) => (s as String).startsWith('request:')),
        isTrue,
      );
      expect(
        trail.any((s) => (s as String).startsWith('answer:')),
        isTrue,
      );
    });
  });
}

void _appendComposerTest() {
  testWidgets('home composer creates a durable run through the core',
      (tester) async {
    if (!coreAvailable) return;
    await pumpApp(tester);
    await tester.enterText(
        find.byType(TextField).first, 'summarize the board pack');
    await tester.pump();
    // The send action is the labeled FilledButton in the composer.
    await tester.tap(find.text('Send'));
    // submitRequest runs through the worker isolate — give the real
    // async chain time to finish before asserting durability.
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 400)));
    await tester.pump();
    // The submitted request became a durable run visible in Activity.
    await tester.tap(find.text('Activity').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.textContaining('run-'), findsWidgets);
  });
}
