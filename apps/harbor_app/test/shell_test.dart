import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/preferences.dart';
import 'package:harbor_app/shell/adaptive_shell.dart';
import 'package:harbor_app/shell/keyboard.dart';
import 'package:harbor_app/surfaces/work_surface.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// Pump HarborApp with a REAL viewport of [width]x[height] logical pixels
/// (the shell's breakpoint decisions must agree with actual layout).
Future<void> pumpApp(
  WidgetTester tester, {
  Locale locale = const Locale('en'),
  double width = 1440,
  double height = 900,
  PreferencesStore? preferences,
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
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-shell',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
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
    home: HarborApp(service: service, preferences: preferences),
  ));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
}

/// Navigate to a surface by its label. On compact widths the five-item
/// bottom bar keeps Agents/Skills/Knowledge/Activity/Settings under
/// "More", so the helper opens that sheet first when needed.
Future<void> goTo(WidgetTester tester, String label,
    {String more = 'More'}) async {
  var target = find.text(label);
  if (target.evaluate().isEmpty) {
    final rail = find.byType(HarborRail);
    if (rail.evaluate().isNotEmpty) {
      // Short windows: the rail scrolls to reach later destinations.
      await tester.scrollUntilVisible(find.text(label), 80,
          scrollable:
              find.descendant(of: rail, matching: find.byType(Scrollable)));
    } else {
      await tester.tap(find.text(more).first);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
    }
    target = find.text(label);
  }
  await tester.tap(target.first);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
  // One more frame: the previous surface retires (goes offstage) after
  // its fade completes.
  await tester.pump();
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
    await goTo(tester, 'Settings');
    await tester.tap(find.text('العربية'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    final context = tester.element(find.byType(NavigationBar).first);
    expect(Directionality.of(context), TextDirection.rtl);
    expect(find.text('الرئيسية'), findsOneWidget);
    // Back on Home: work-first headline is localized.
    await goTo(tester, 'الرئيسية', more: 'المزيد');
    expect(find.text('ما الذي تريد إنجازه؟'), findsOneWidget);
  });

  testWidgets('compact width uses a five-item bottom bar plus More',
      (tester) async {
    await pumpApp(tester, width: 390, height: 844);
    expect(find.byType(NavigationBar), findsOneWidget);
    expect(find.byType(HarborRail), findsNothing);
    // Material guidance: 3–5 destinations. Primary surfaces are direct.
    expect(find.byType(NavigationDestination), findsNWidgets(5));
    for (final label in ['Home', 'Ask', 'Work', 'Models', 'More']) {
      expect(find.text(label), findsOneWidget);
    }
    // The Trust chip is visible on every compact surface (§1 "Local is
    // visible") and opens the Lens sheet.
    expect(find.text('LOCAL'), findsOneWidget);
    // The remaining surfaces are one tap away under More.
    await tester.tap(find.text('More'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    for (final label in [
      'Agents',
      'Skills',
      'Knowledge',
      'Activity',
      'Settings'
    ]) {
      expect(find.text(label), findsOneWidget);
    }
    await tester.tap(find.text('Knowledge'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    // The app bar names the secondary surface (the surface header does
    // not repeat it on compact widths); More reads as selected.
    expect(find.byType(NavigationDestination), findsNWidgets(5));
    expect(find.text('Knowledge'), findsOneWidget);
    expect(find.textContaining('citation-backed'), findsOneWidget);
    expect(find.text('More'), findsOneWidget);
    final bar = tester.widget<NavigationBar>(find.byType(NavigationBar));
    expect(bar.selectedIndex, 4);
  });

  testWidgets('medium width shows compact rail, no persistent lens',
      (tester) async {
    await pumpApp(tester, width: 800, height: 600);
    expect(find.byType(HarborRail), findsOneWidget);
    expect(find.byType(NavigationBar), findsNothing);
    expect(find.byType(TrustPulse), findsNothing);
    // Medium rail: icon + caption for all nine surfaces.
    expect(find.text('Knowledge'), findsOneWidget);
  });

  testWidgets(
      'wide width shows rail; at 1280 lens is persistent with '
      'canvas >= 640', (tester) async {
    await pumpApp(tester, width: 1280, height: 800);
    final railFinder = find.byType(HarborRail);
    expect(railFinder, findsOneWidget);
    // Trust Pulse is present in the persistent (docked) Harbor Lens.
    expect(find.byType(TrustPulse), findsOneWidget);
    expect(find.text('Harbor Lens'), findsOneWidget);
    // Rail (220) + lens (320) + canvas (>=640) <= 1280.
    final railSize = tester.getSize(railFinder);
    expect(railSize.width, HarborLayout.desktopRail);
    expect(railSize.width, lessThanOrEqualTo(1280 - HarborLayout.desktopLens));
    // The Lens can be undocked (and the choice is a preference).
    await tester.tap(find.byIcon(Icons.view_sidebar));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(TrustPulse), findsNothing);
    await tester.tap(find.byIcon(Icons.view_sidebar_outlined));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(TrustPulse), findsOneWidget);
  });

  testWidgets('expanded width collapses the rail to 72px with a Lens drawer',
      (tester) async {
    await pumpApp(tester, width: 1100, height: 800);
    expect(tester.getSize(find.byType(HarborRail)).width,
        HarborLayout.desktopRailCollapsed);
    // Collapsed: labels become tooltips; the canvas keeps >= 640.
    expect(find.text('Knowledge'), findsNothing);
    // Tooltip carries the label (plus the ⌘7 hint on desktop).
    expect(find.byTooltip(RegExp(r'^Knowledge')), findsOneWidget);
    expect(find.byType(TrustPulse), findsNothing);
    await tester.tap(find.byIcon(Icons.view_sidebar_outlined));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(TrustPulse), findsOneWidget);
    expect(find.byType(Drawer), findsOneWidget);
  });

  testWidgets('language and theme choices persist through the store',
      (tester) async {
    final store = MemoryPreferencesStore();
    await pumpApp(tester, preferences: store);
    await goTo(tester, 'Settings');
    await tester.tap(find.text('Dark'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(store.current.themeMode, ThemeMode.dark);
    expect(Theme.of(tester.element(find.byType(HarborRail))).brightness,
        Brightness.dark);
    await tester.tap(find.text('العربية'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(store.current.locale, const Locale('ar'));
    // A fresh app over the same store starts in Arabic + dark.
    await tester.pumpWidget(const SizedBox());
    await pumpApp(tester, preferences: store);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('الإعدادات'), findsWidgets);
    expect(Theme.of(tester.element(find.byType(HarborRail))).brightness,
        Brightness.dark);
    expect(Directionality.of(tester.element(find.byType(HarborRail))),
        TextDirection.rtl);
  });

  testWidgets('desktop keyboard shortcuts switch surfaces', (tester) async {
    if (!harborHasKeyboardShortcuts) return;
    await pumpApp(tester);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit7);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    // Knowledge header (+ rail label, + the docked Lens link).
    expect(find.text('Knowledge'), findsAtLeastNWidgets(2));
    expect(find.textContaining('citation-backed'), findsOneWidget);
  });

  testWidgets('command palette navigates and prefills the composer',
      (tester) async {
    if (!harborHasKeyboardShortcuts) return;
    await pumpApp(tester);
    await tester.tap(find.byIcon(Icons.keyboard_command_key));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('Surfaces'), findsOneWidget);
    await tester.enterText(find.byType(TextField).last, 'activ');
    await tester.pump();
    await tester.tap(find.text('Go to Activity'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.textContaining('Durable runs and background'), findsOneWidget);
    // Quick actions from the palette land in the Home composer.
    await tester.tap(find.byIcon(Icons.keyboard_command_key));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.enterText(find.byType(TextField).last, 'Translate');
    await tester.pump();
    await tester.tap(find.text('Translate content').last);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    final composer = tester.widget<TextField>(find.byType(TextField).first);
    expect(composer.controller!.text, 'Translate content');
  });

  testWidgets('settings switches to Arabic and mirrors the rail',
      (tester) async {
    await pumpApp(tester);
    await goTo(tester, 'Settings');
    await tester.tap(find.text('العربية'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('الإعدادات'), findsWidgets);
    expect(find.text('اللغة'), findsOneWidget);
    // Structural mirroring: the rail sits at the end (right) edge.
    final rail = tester.getRect(find.byType(HarborRail));
    expect(rail.left, greaterThan(1440 - HarborLayout.desktopRail - 1));
    // Identifiers keep intrinsic LTR inside the RTL layout.
    final id = find.byType(HarborIdentifier).first;
    expect(
        Directionality.of(tester.element(
            find.descendant(of: id, matching: find.byType(Text)).first)),
        TextDirection.ltr);
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
          workspaceId: 'ws-activity',
          deviceRootHex:
              '02235e07dcc1083f22170c8d10ef3574f99a42f85ecbbc1402ab12afd8088493');
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
    // Default test viewport is medium: tap the rail's Activity entry.
    await goTo(tester, 'Activity');
    await tester.pump(const Duration(milliseconds: 100));
    // The run id and its state appear in the Activity list (real runs,
    // never samples), with the §11 state badge.
    expect(find.text('run-visible-1'), findsWidgets);
    expect(find.textContaining('CREATED'), findsWidgets);
    expect(find.byType(RunStateBadge), findsWidgets);
    // Home lists it under Recent runs AND the docked Harbor Lens (1440)
    // shows the same real run — never sample data.
    await goTo(tester, 'Home');
    expect(find.text('run-visible-1'), findsNWidgets(2));
    expect(find.byType(HarborLens), findsOneWidget);
  });
}

void _appendPreviewTest() {
  testWidgets('work canvas renders a real workbook preview via the core',
      (tester) async {
    if (!coreAvailable) return;
    final fixturePath = '$repoRoot/fixtures/office/board_demo.xlsx';
    HarborService? service;
    await tester.runAsync(() async {
      final bytes = await File(fixturePath).readAsBytes();
      final dir = await Directory.systemTemp.createTemp('harbor-preview-');
      final s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-canvas',
          deviceRootHex:
              '9f112e8df52dc050b9277024c181670c449b0aa0139c4846d5ca892fbf933056');
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
    // The grid opens on the first formula cell: the formula bar shows it
    // (OOXML stores formulas without the leading '='; the bar adds it).
    expect(find.textContaining('SUM(B2:B5)'), findsOneWidget);
    expect(find.text('=SUM(B2:B5)'), findsOneWidget);
    // Real spreadsheet chrome: column letters, row numbers, cached-value
    // disclaimer, read-only badge.
    expect(find.text('A'), findsOneWidget);
    expect(find.text('B'), findsOneWidget);
    expect(find.text('1'), findsOneWidget);
    expect(find.textContaining('not verified'), findsOneWidget);
    expect(find.text('Read-only preview'), findsOneWidget);
    // Selecting another cell rebinds the formula bar.
    await tester.tap(find.text('A').first);
    await tester.pump();
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
    await goTo(tester, 'Skills');
    // The 21 authority skill families come through the live boundary.
    final sp = HarborServiceProvider.of(
        tester.element(find.textContaining('tools').first));
    expect(sp.notifier!.skills.length, greaterThanOrEqualTo(21));
    expect(find.text('Document Intelligence'), findsOneWidget);
    expect(find.text('Spreadsheet Analyst'), findsOneWidget);
    // The list is lazy: scroll the skills list to the last family.
    await tester.scrollUntilVisible(find.text('Privacy Inspector'), 300,
        scrollable: find.descendant(
            of: find.byKey(const ValueKey('skills-list')),
            matching: find.byType(Scrollable)));
    expect(find.text('Privacy Inspector'), findsOneWidget);
    // Filtering narrows the grid; the detail sheet lists real tools.
    await tester.enterText(find.byType(TextField).first, 'privacy');
    await tester.pump();
    expect(find.text('Document Intelligence'), findsNothing);
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
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-know',
          deviceRootHex:
              '12721f8a3480ca77d995c914cb6dbc50f401e82e317b3b96e0b0e2b01747ca10');
      // Install the REAL bge-small-en-v1.5 embedding model through the
      // staged-install path, then open the durable index over it.
      opened = await s.installModelFromPath(
            packageId: 'bge-small-en-v1.5',
            path: '$repoRoot/fixtures/models/bge-small-en-v1.5-q8_0.gguf',
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
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-rag',
          deviceRootHex:
              '57ebfb10cdada18503fb8d0195b9055ffc3dab57cfb888620aca640dd5aa056c');
      // Install BOTH models through the real staged-install path:
      // the chat model and the embedding model.
      await s.installModelFromPath(
          packageId: 'stories260k',
          path: '$repoRoot/fixtures/models/stories260K.gguf');
      await s.installModelFromPath(
          packageId: 'bge-small-en-v1.5',
          path: '$repoRoot/fixtures/models/bge-small-en-v1.5-q8_0.gguf');
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
    // submitRequest runs through the worker isolate (create + log +
    // full refresh) — give the real async chain time to finish.
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 1500)));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    // The submitted request became a durable run visible in Activity.
    // The submit chain (create + log + refresh) is real async on the
    // worker isolate: poll the service state until the durable run lands.
    final sp = HarborServiceProvider.of(
        tester.element(find.text('What do you want to get done?')));
    var durable = false;
    for (var i = 0; i < 40 && !durable; i++) {
      await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 150)));
      durable = sp.notifier!.runs.isNotEmpty;
    }
    expect(durable, isTrue,
        reason: 'the composer submit must create a durable run');
    await goTo(tester, 'Activity');
    expect(find.textContaining('run-'), findsWidgets);
  });
}
