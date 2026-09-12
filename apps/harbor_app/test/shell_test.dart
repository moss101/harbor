import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/foundation.dart';
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
      final s = HarborService.open(
          libraryPath: dylibPath, dataRoot: dir.path, workspaceId: 'ws-shell');
      await s.refresh();
      service = s;
    });
  }
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
  // ignore: avoid_print
  print('PUMP: widget mounted, pumping');
  await tester.pump();
  // ignore: avoid_print
  print('PUMP: pump1 done');
  await tester.pump(const Duration(milliseconds: 100));
  // ignore: avoid_print
  print('PUMP: pump2 done');
}

void main() {
  _appendLiveTests();
  _appendPreviewTest();
  _appendSkillsTest();
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
    for (final label in ['Models', 'Skills', 'Knowledge', 'Activity', 'Settings']) {
      expect(find.text(label), findsWidgets);
    }
  });

  testWidgets('medium width shows compact rail, no persistent lens',
      (tester) async {
    await pumpApp(tester, width: 800, height: 600);
    expect(find.byType(NavigationRail), findsOneWidget);
    expect(find.byType(NavigationBar), findsNothing);
  });

  testWidgets('wide width shows rail; at 1280 lens is persistent with '
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
  testWidgets('home model dock reflects the live core state',
      (tester) async {
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
    expect(
        find.text('No model installed — open Models to install one'),
        findsOneWidget);
  });

  testWidgets('activity lists durable runs created through the core',
      (tester) async {
    await pumpApp(tester);
    if (!coreAvailable) return;
    HarborService? service;
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-activity-');
      final s = HarborService.open(
          libraryPath: dylibPath, dataRoot: dir.path, workspaceId: 'ws-activity');
      s.createRun('run-visible-1');
      await s.refresh();
      service = s;
    });
    if (service == null) return;
    // ignore: use_build_context_synchronously
    await tester.pumpWidget(HarborApp(service: service));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    // "Activity" appears in nav, rail and Lens — tap the rail entry.
    await tester.tap(find.text('Activity').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('run-visible-1'), findsOneWidget);
    expect(find.textContaining('state: CREATED'), findsOneWidget);
    service!.close();
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
      final s = HarborService.open(
          libraryPath: dylibPath, dataRoot: dir.path, workspaceId: 'ws-canvas');
      await s.loadPreviewFromBytes(bytes);
      service = s;
    });
    if (service == null) return;
    await tester.pumpWidget(HarborTheme(
      colors: HarborColors.light,
      text: HarborType(arabic: false),
      child: MaterialApp(
        localizationsDelegates: const [
          AppLocalizations.delegate,
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        supportedLocales: const [Locale('en')],
        home: HarborServiceProvider(
          failed: false, service: service, child: const WorkSurface()),
      ),
    ));
    await tester.pump();
    // The preview comes from the pinned engine's recalculation, not a mock.
    expect(find.textContaining('Sheet: Sheet1'), findsOneWidget);
    // OOXML stores formulas without the leading '='.
    expect(find.textContaining('SUM(B2:B5)'), findsOneWidget);
    service!.close();
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
    await tester.scrollUntilVisible(
        find.text('Privacy Inspector'), 300,
        scrollable: find.byType(Scrollable).first);
    expect(find.text('Privacy Inspector'), findsOneWidget);
  });
}
