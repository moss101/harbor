import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/diagnostics.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/preferences.dart';
import 'package:harbor_app/shell/adaptive_shell.dart';
import 'package:harbor_app/shell/keyboard.dart';
import 'package:harbor_app/surfaces/settings_surface.dart';
import 'package:harbor_app/surfaces/skill_run.dart';
import 'package:harbor_app/surfaces/work_surface.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// Pump real time until [until] matches. The live-core tests drive a
/// worker isolate and real file IO, so they cannot use `pumpAndSettle`.
///
/// It fails with what the UI is actually showing rather than returning
/// quietly and leaving a later `findsOneWidget` to report "Found 0
/// widgets": a run that failed surfaces the core's reason from its Run
/// failed banner, and a run that never finished says so with its budget.
/// A shared CI runner executes the debug core several times slower than
/// a development machine, which is why the budgets are generous — and
/// why an opaque failure there is expensive to diagnose.
Future<void> settleUntil(
  WidgetTester tester,
  Finder until, {
  Duration budget = const Duration(seconds: 120),
  String? what,
  Future<String> Function()? onTimeout,
}) async {
  final target = what ?? until.toString();
  final deadline = DateTime.now().add(budget);
  while (DateTime.now().isBefore(deadline)) {
    await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)));
    // Advance the FAKE clock too — load-bearing, not cosmetic. The
    // service's poll loop starts from a tap handler, so the
    // `Future.delayed(250ms)` it waits between `op.status` calls is a
    // FAKE timer; `pump()` calls `elapse` only when given a duration, so
    // with no argument that timer can never fire. The loop got away with
    // it because the run normally finishes before the first status reply
    // is processed and it returns on the first pass — but when the op is
    // slower than that round trip (CPU contention, a loaded CI runner)
    // it reaches the dead timer and waits for ever. That is the stall.
    await tester.pump(const Duration(milliseconds: 100));
    if (until.evaluate().isNotEmpty) return;
    final failed = tester
        .widgetList<HarborBanner>(find.byType(HarborBanner))
        .where((b) => b.title == 'Run failed')
        .map((b) => b.body ?? '(no detail in the banner)');
    if (failed.isNotEmpty) {
      fail('the run failed while waiting for $target: ${failed.first}');
    }
  }
  // A stall, not a slow runner: this wait normally resolves in seconds.
  // Print what the sheet is showing so the next occurrence says where the
  // run stopped instead of only that it did.
  final visible = tester
      .widgetList<Text>(find.byType(Text))
      .map((t) => t.data)
      .whereType<String>()
      .where((t) => t.trim().isNotEmpty)
      .take(40)
      .join(' | ');
  // The tree only shows what the UI last HEARD. Ask the core directly
  // so a stall that is really a starved poll — core finished, nothing
  // delivered it — cannot be mistaken for a core that is stuck.
  var probe = '';
  if (onTimeout != null) {
    try {
      probe = await tester.runAsync(onTimeout) ?? '';
    } catch (e) {
      probe = '\nthe timeout probe itself failed: $e';
    }
  }
  fail('timed out after ${budget.inSeconds}s waiting for $target; '
      'the tree is showing: $visible$probe');
}

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
  // Without the dylib this file produces ten failures spread across
  // tests that look like layout and localization ones, because pumping
  // HarborApp opens the service; a further ten guard on `coreAvailable`
  // and return quietly. Naming the cause once, first, is worth one test.
  test('the live native core is built', () {
    expect(coreAvailable, isTrue,
        reason: 'no $dylibPath — build it with '
            '`cargo build --manifest-path core/Cargo.toml -p harbor_ffi`');
  });
  _appendLiveTests();
  _appendPreviewTest();
  _appendSkillsTest();
  _appendSkillRunTests();
  _appendSkillCommitTests();
  _appendDiagnosticsTests();
  _appendFirstRunTests();
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

void _appendSkillRunTests() {
  testWidgets(
      'skills surface marks graph skills runnable and opens the run form',
      (tester) async {
    await pumpApp(tester);
    if (!coreAvailable) return;
    await goTo(tester, 'Skills');
    // Cards say honestly which skills can run: the nine decomposed graph
    // skills are runnable, the prose ones are declarations.
    expect(find.text('Runnable graph'), findsWidgets);
    expect(find.text('Declaration only'), findsWidgets);
    final sp = HarborServiceProvider.of(
        tester.element(find.textContaining('tools').first));
    final runnable = sp.notifier!.skills.where((s) => s.runnable).toList();
    expect(
        runnable.map((s) => s.id),
        containsAll([
          'placeholder-fill',
          'formula-audit',
          'second-look',
          'meeting-notes',
          'deck-review',
          'financial-model-review',
          'document-style-review',
          'team-update',
          'doc-coauthoring',
        ]));
    expect(runnable, hasLength(9));
    expect(
        runnable.firstWhere((s) => s.id == 'placeholder-fill').graph!.usesModel,
        isFalse);
    // Open the detail sheet of a runnable skill: graph facts + Run.
    await tester.enterText(find.byType(TextField).first, 'placeholder');
    await tester.pump();
    await tester.tap(find.text('Placeholder & Form Fill'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('No model calls — fully deterministic'), findsOneWidget);
    expect(find.byKey(const ValueKey('skill-run-placeholder-fill')),
        findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('skill-run-placeholder-fill')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    // The run form is generated from the graph's input schema: an
    // artifact attach button, a values editor, and Run disabled until the
    // required inputs are present.
    expect(find.byKey(const ValueKey('attach-artifact_id')), findsOneWidget);
    expect(find.byKey(const ValueKey('input-values')), findsOneWidget);
    final run = tester
        .widget<FilledButton>(find.byKey(const ValueKey('skill-run-button')));
    expect(run.onPressed, isNull);
  });

  testWidgets(
      'placeholder fill runs through the service to approval and completion',
      (tester) async {
    if (!coreAvailable) return;
    HarborService? service;
    Map<String, dynamic>? report;
    Map<String, dynamic>? decided;
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-skill-');
      final s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-skill',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
      await s.refresh();
      service = s;
      final bytes = await File('$repoRoot/fixtures/office/letter_template.docx')
          .readAsBytes();
      report = await s.startSkillRun(
        skillId: 'placeholder-fill',
        inputs: {
          'artifact_id': 'a1',
          'values': {
            'name': 'Amina',
            'ref': 'HB-42',
            'AMOUNT': '1,250.00',
            'sender': 'Harbor Team'
          },
        },
        artifacts: [
          SkillArtifact(id: 'a1', name: 'letter_template.docx', bytes: bytes)
        ],
      );
      decided = await s.decideRun(report!['run_id'] as String, approved: true);
    });
    addTearDown(() => service?.close());
    expect(report!['state'], 'WAITING_APPROVAL');
    expect(report!['status']['approval']['batch']['operations'], hasLength(3));
    expect(decided!['state'], 'COMPLETED');
    expect(
        decided!['status']['outputs']['approvals.approve']['approved'], isTrue);
    // The run is in the durable run list the Activity surface reads.
    expect(service!.runs.map((r) => r['run_id']), contains(report!['run_id']));
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

/// Production plan B1/B2 exit journey on the live core: open a fixture,
/// run Formula-free Placeholder Fill, review the before/after diff, Save
/// New Copy, reopen the copy and verify it is the approved output while
/// the original is untouched.
void _appendSkillCommitTests() {
  testWidgets(
      'run sheet reviews the diff and saves a new copy on the live core',
      (tester) async {
    if (!coreAvailable) return;
    tester.view.physicalSize = const Size(1200, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    HarborService? service;
    late Directory dir;
    await tester.runAsync(() async {
      dir = await Directory.systemTemp.createTemp('harbor-commit-');
      final s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-commit',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
      await s.refresh();
      service = s;
    });
    addTearDown(() => service?.close());
    // Real file IO must run inside runAsync (fake-async zone otherwise).
    final original = File('$repoRoot/fixtures/office/letter_template.docx');
    final originalBytes = await tester.runAsync(() => original.readAsBytes());
    final copy = File('${dir.path}/letter_template (Harbor).docx');
    final skill = service!.skills.firstWhere((s) => s.id == 'placeholder-fill');

    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: const [Locale('en'), Locale('ar')],
      theme: harborThemeData(dark: false, arabic: false),
      builder: (_, child) => HarborTheme(
        colors: HarborColors.light,
        text: const HarborType(arabic: false),
        child: child!,
      ),
      home: Scaffold(
        body: SkillRunSheet(
          skill: skill,
          service: service!,
          pickArtifact: () async => XFile(original.path),
          resolveSaveDestination: (suggested) async {
            expect(suggested, 'letter_template (Harbor).docx');
            return copy.path;
          },
        ),
      ),
    ));
    await tester.pump();

    // Real async (worker isolate, file IO) only progresses inside
    // runAsync; widget interaction must stay outside it (guarded calls).
    Future<void> settle(Finder until,
            {String? what, Future<String> Function()? onTimeout}) =>
        settleUntil(tester, until, what: what, onTimeout: onTimeout);

    // Attach the fixture through the injected picker and fill the values.
    await tester.tap(find.byKey(const ValueKey('attach-artifact_id')));
    await settle(find.textContaining('letter_template.docx'),
        what: 'the attached fixture');
    expect(find.textContaining('letter_template.docx'), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('input-values')),
        'name=Amina\nref=HB-42\nAMOUNT=1,250.00\nsender=Harbor Team');
    await tester.pump();
    final run = tester
        .widget<FilledButton>(find.byKey(const ValueKey('skill-run-button')));
    expect(run.onPressed, isNotNull);

    // Run → the executor parks the run for approval with a diff.
    await tester.tap(find.byKey(const ValueKey('skill-run-button')));
    await settle(find.byKey(const ValueKey('skill-approval')),
        what: 'the run to park for approval', onTimeout: () async {
      final ops = await service!.listOps();
      final summary = ops.isEmpty
          ? '(no ops)'
          : ops
              .map((o) =>
                  '${o['kind']} ${o['state']} phase=${o['phase']} detail=${o['detail']}')
              .join('; ');
      return '\nthe core itself reports: $summary';
    });
    expect(find.byKey(const ValueKey('skill-approval')), findsOneWidget);
    expect(find.byKey(const ValueKey('skill-proposal-diff')), findsOneWidget);
    // Before/after lines from the core's diff, rendered by ArtifactDiffView.
    expect(find.textContaining('- Dear {{name}},'), findsOneWidget);
    expect(find.textContaining('+ Dear Amina,'), findsOneWidget);
    expect(find.text('Save new copy'), findsOneWidget);
    expect(find.text('Reject'), findsOneWidget);
    // Overwrite is offered because the picked file has a real path.
    expect(find.byKey(const ValueKey('skill-overwrite')), findsOneWidget);
    expect(copy.existsSync(), isFalse);

    // Save new copy (the default primary action).
    await tester.tap(find.text('Save new copy'));
    await settle(find.byKey(const ValueKey('skill-committed')),
        what: 'the commit to land');
    expect(find.byKey(const ValueKey('skill-committed')), findsOneWidget);
    expect(find.text('Saved as a new copy'), findsOneWidget);
    expect(find.textContaining(copy.path), findsWidgets);
    expect(find.text('COMPLETED'), findsOneWidget);

    // Reopen: the copy is the approved output; the original is untouched.
    expect(copy.existsSync(), isTrue);
    late List<int> copyBytes;
    late List<int> originalNow;
    Map<String, dynamic>? preview;
    await tester.runAsync(() async {
      copyBytes = await copy.readAsBytes();
      originalNow = await original.readAsBytes();
      preview = await service!.extractPreview(copyBytes);
    });
    expect(copyBytes, isNot(equals(originalBytes)));
    expect(originalNow, equals(originalBytes));
    final text = preview.toString();
    expect(text, contains('Dear Amina,'));
    expect(text, contains('HB-42'));
    expect(text, isNot(contains('{{name}}')));
    // The run's durable record shows the commit resolved.
    Map<String, dynamic>? snap;
    await tester.runAsync(() async {
      final runId = service!.runs.first['run_id'] as String;
      snap = await service!.runSnapshot(runId);
    });
    expect(snap!['state'], 'COMPLETED');
    expect(snap!['pending_approval'], isNull);
  });
}

/// Production plan C1: uncaught app errors reach the core's encrypted
/// diagnostics log, and Settings → Export diagnostics writes a zip with
/// redacted records and build facts to a user-chosen path.
void _appendDiagnosticsTests() {
  testWidgets('diagnostics export writes a bundle from the live core',
      (tester) async {
    if (!coreAvailable) return;
    tester.view.physicalSize = const Size(1200, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    HarborService? service;
    late Directory dir;
    await tester.runAsync(() async {
      dir = await Directory.systemTemp.createTemp('harbor-diag-');
      final s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-diag',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
      await s.refresh();
      // An app-side error recorded through the sink lands in the log.
      DiagnosticsSink.instance.attach(s);
      DiagnosticsSink.instance.record(
          level: 'error',
          message: 'RenderFlex overflowed in /Users/me/Documents/plan.docx',
          context: 'package:harbor_app/surfaces/work_surface.dart');
      await Future<void>.delayed(const Duration(milliseconds: 300));
      service = s;
    });
    addTearDown(() {
      DiagnosticsSink.instance.attach(null);
      return service?.close();
    });
    final dest = File('${dir.path}/export/harbor-diagnostics.zip');
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: const [Locale('en'), Locale('ar')],
      theme: harborThemeData(dark: false, arabic: false),
      builder: (_, child) => HarborTheme(
        colors: HarborColors.light,
        text: const HarborType(arabic: false),
        child: child!,
      ),
      home: Scaffold(
        body: DiagnosticsCard(
          service: service,
          resolveDestination: (suggested) async {
            expect(suggested, startsWith('harbor-diagnostics-'));
            expect(suggested, endsWith('.zip'));
            return dest.path;
          },
        ),
      ),
    ));
    await tester.pump();
    Future<void> settle(Finder until, {String? what}) =>
        settleUntil(tester, until,
            budget: const Duration(seconds: 60), what: what);

    await settle(find.textContaining('record'), what: 'a diagnostics record');
    expect(find.textContaining('record'), findsWidgets);
    await tester.tap(find.byKey(const ValueKey('diagnostics-export')));
    await settle(find.byKey(const ValueKey('diagnostics-exported')),
        what: 'the diagnostics export');
    expect(find.byKey(const ValueKey('diagnostics-exported')), findsOneWidget);
    expect(find.textContaining(dest.path), findsOneWidget);
    expect(dest.existsSync(), isTrue);
    // The bundle carries the redacted app record and the app version.
    late String manifest;
    late String records;
    await tester.runAsync(() async {
      final bytes = await dest.readAsBytes();
      final text = String.fromCharCodes(bytes);
      // Deflated entries are not byte-visible; read them back through the core.
      expect(text, contains('diagnostics.json'));
      final list = await service!.listDiagnostics(limit: 10);
      manifest = list.toString();
      records = (list['records'] as List).toString();
    });
    expect(records, contains('<path:.docx>'));
    expect(records, isNot(contains('/Users/me')));
    expect(records, contains('work_surface.dart'));
    expect(manifest, contains('redaction_policy'));
  });
}

/// Production plan C2: first run has no model; Home says so with the Local
/// Only fact and takes the user to Models → Recommended, which lists the
/// bundled signed catalog offline once the core has accepted it.
void _appendFirstRunTests() {
  testWidgets(
      'first run: Home drives to Models and Recommended lists the catalog',
      (tester) async {
    await pumpApp(tester);
    if (!coreAvailable) return;
    // No model installed on a fresh data root → the first-run card.
    expect(find.byKey(const ValueKey('home-first-run')), findsOneWidget);
    expect(find.text('Install a model to get started'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('home-first-run-install')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    // Models opens on Recommended. The bundled catalog is imported by the
    // app bootstrap; in this harness we import it the same way.
    final sp = HarborServiceProvider.of(
        tester.element(find.text('Recommended').first));
    final service = sp.notifier!;
    expect(service.needsFirstModel, isTrue);
    var imported = false;
    await tester.runAsync(() async {
      imported = await importBundledCatalog(service);
    });
    expect(imported, isTrue);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(service.catalogImported, isTrue);
    expect(
        service.catalog.map((p) => p['id']), contains('qwen2.5-1.5b-instruct'));
    expect(find.byKey(const ValueKey('models-first-run')), findsOneWidget);
    expect(find.text('Local only'), findsWidgets);
    expect(find.byKey(const ValueKey('catalog-qwen2.5-1.5b-instruct')),
        findsOneWidget);
    // The Test-tier fixture package is not recommended.
    expect(find.byKey(const ValueKey('catalog-stories260k')), findsNothing);
    expect(find.text('Check size & fit'), findsWidgets);
    expect(find.text('Install'), findsWidgets);
    // Epochs are monotonic: re-importing the already accepted epoch is
    // refused by the core (rollback protection), and the accepted catalog
    // stays in place. The app only imports when nothing is accepted yet.
    var again = true;
    await tester.runAsync(() async {
      again = await importBundledCatalog(service);
    });
    expect(again, isFalse);
    expect(service.catalogImported, isTrue);
  });
}
