import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/services/preferences.dart';
import 'package:harbor_app/shell/adaptive_shell.dart';
import 'package:harbor_app/shell/keyboard.dart';
import 'package:harbor_ui/harbor_ui.dart';

import 'fake_service.dart';

/// The navigation contract (scripted core, no dylib): three primary
/// destinations in the answer-first sidebar — Home (with Ask merged in) ·
/// Work · Library — and four out of the main path under More — Models
/// (with the Store as a tab) · Skills · Activity (with scheduled goals) ·
/// Settings. Ask, Store and Agents are not destinations.
Future<AppState> pumpShell(
  WidgetTester tester,
  FakeHarborService service, {
  Size size = const Size(1440, 900),
  Locale locale = const Locale('en'),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  final arabic = locale.languageCode == 'ar';
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
    home: HarborApp(service: service, preferences: MemoryPreferencesStore()),
  ));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
  return AppStateScope.of(tester.element(find.byType(AdaptiveShell)));
}

Future<void> settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
  await tester.pump();
}

void main() {
  test('the surface enum is the published seven-destination contract', () {
    expect(HarborSurface.values, [
      HarborSurface.home,
      HarborSurface.work,
      HarborSurface.knowledge,
      HarborSurface.models,
      HarborSurface.skills,
      HarborSurface.activity,
      HarborSurface.settings,
    ]);
    expect(HarborSurface.primary, [
      HarborSurface.home,
      HarborSurface.work,
      HarborSurface.knowledge,
    ]);
    expect(HarborSurface.secondary, [
      HarborSurface.models,
      HarborSurface.skills,
      HarborSurface.activity,
      HarborSurface.settings
    ]);
    // Every destination is primary or secondary, exactly once.
    expect({...HarborSurface.primary, ...HarborSurface.secondary},
        HarborSurface.values.toSet());
    // ⌘1–7: every destination, in rail order.
    expect(HarborSurface.shortcutOrder, HarborSurface.values);
  });

  test('the shortcut map binds exactly ⌘/Ctrl+1–7 to surfaces', () {
    final digits = harborShortcuts()
        .values
        .whereType<GoToSurfaceIntent>()
        .map((i) => i.index)
        .toSet();
    expect(digits, {0, 1, 2, 3, 4, 5, 6});
  });

  testWidgets(
      'the sidebar lists Home · Work · Library; More holds the other four',
      (tester) async {
    await pumpShell(tester, FakeHarborService());
    final bar = tester.widget<HarborSidebar>(find.byType(HarborSidebar));
    expect(bar.destinations.map((d) => d.label), ['Home', 'Work', 'Library']);
    expect(bar.collapsed, isFalse, reason: 'open by default at 1440');
    if (harborHasKeyboardShortcuts) {
      expect([for (var i = 0; i < 3; i++) bar.shortcutHint!(i)],
          [for (var i = 1; i <= 3; i++) harborShortcutHint('$i')]);
    }
    expect(find.byKey(const ValueKey('shell-new-thread')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('shell-more')));
    await settle(tester);
    expect(
        tester
            .widgetList<MenuItemButton>(find.byType(MenuItemButton))
            .map((b) => ((b.child as Text).data)),
        ['Models', 'Skills', 'Activity', 'Settings']);
  });

  testWidgets('the sidebar mirrors in Arabic with the localized Library',
      (tester) async {
    final state = await pumpShell(tester, FakeHarborService());
    // The language is the app's own preference (Settings), not the host's.
    state.setLocale(const Locale('ar'));
    await settle(tester);
    final bar = tester.widget<HarborSidebar>(find.byType(HarborSidebar));
    expect(bar.destinations.map((d) => d.label), contains('المكتبة'));
    expect(bar.destinations.length, 3);
    final sidebar = find.byType(HarborSidebar);
    expect(Directionality.of(tester.element(sidebar)), TextDirection.rtl);
    expect(tester.getRect(sidebar).right, 1440,
        reason: 'the sidebar sits at the start (right) edge in RTL');
  });

  testWidgets('compact: no bottom bar; the drawer holds the sidebar',
      (tester) async {
    final state = await pumpShell(tester, FakeHarborService(),
        size: const Size(390, 844));
    expect(find.byType(NavigationBar), findsNothing);
    expect(find.byType(HarborSidebar), findsNothing);
    await tester.tap(find.byTooltip('Open navigation menu'));
    await settle(tester);
    final bar = tester.widget<HarborSidebar>(find.byType(HarborSidebar));
    expect(bar.destinations.map((d) => d.label), ['Home', 'Work', 'Library']);
    await tester.tap(find.byKey(const ValueKey('shell-more')));
    await settle(tester);
    await tester.tap(find.text('Settings'));
    await settle(tester);
    expect(state.surface, HarborSurface.settings);
    final scaffold = tester.state<ScaffoldState>(find.byType(Scaffold).first);
    expect(scaffold.isDrawerOpen, isFalse, reason: 'navigating closes it');
    await settle(tester);
    expect(find.byType(Drawer), findsNothing, reason: 'navigating closes it');
  });

  testWidgets('the sidebar collapses to icons and back', (tester) async {
    await pumpShell(tester, FakeHarborService(), size: const Size(1440, 900));
    final sidebar = find.byType(HarborSidebar);
    expect(tester.getSize(sidebar).width, HarborLayout.sidebar);
    await tester.tap(find.byKey(const ValueKey('sidebar-toggle')));
    await settle(tester);
    expect(tester.getSize(sidebar).width, HarborLayout.sidebarCollapsed);
    expect(find.text('Library'), findsNothing);
    expect(find.byTooltip(RegExp(r'^Library')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('sidebar-toggle')));
    await settle(tester);
    expect(tester.getSize(sidebar).width, HarborLayout.sidebar);
  });

  testWidgets('⌘1–7 switch to each surface in order', (tester) async {
    if (!harborHasKeyboardShortcuts) return;
    final state = await pumpShell(tester, FakeHarborService());
    const digits = [
      LogicalKeyboardKey.digit1,
      LogicalKeyboardKey.digit2,
      LogicalKeyboardKey.digit3,
      LogicalKeyboardKey.digit4,
      LogicalKeyboardKey.digit5,
      LogicalKeyboardKey.digit6,
      LogicalKeyboardKey.digit7,
    ];
    // Back to front, so every press is a real change.
    for (final (i, key) in digits.indexed.toList().reversed) {
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(key);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await settle(tester);
      expect(state.surface, HarborSurface.shortcutOrder[i],
          reason: '⌘${i + 1}');
    }
    // ⌘8 is no longer a surface: it changes nothing.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit8);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
    await settle(tester);
    expect(state.surface, HarborSurface.home);
  });

  testWidgets('the palette lists the seven surfaces plus Store and goals',
      (tester) async {
    if (!harborHasKeyboardShortcuts) return;
    final state = await pumpShell(tester, FakeHarborService());
    await tester.tap(find.byIcon(Icons.keyboard_command_key));
    await settle(tester);
    for (final label in [
      'Home',
      'Work',
      'Library',
      'Models',
      'Skills',
      'Activity',
      'Settings'
    ]) {
      expect(find.text('Go to $label'), findsOneWidget);
    }
    for (final gone in ['Ask', 'Agents', 'Store', 'Knowledge']) {
      expect(find.text('Go to $gone'), findsNothing);
    }
    // The Store action lands on the Store tab of Models.
    await tester.enterText(find.byType(TextField).last, 'store');
    await tester.pump();
    await tester.tap(find.text('Store').last);
    await settle(tester);
    expect(state.surface, HarborSurface.models);
    expect(find.text('Harbor Office Suite'), findsOneWidget);
    // Scheduled goals land on the Goals tab of Activity.
    await tester.tap(find.byIcon(Icons.keyboard_command_key));
    await settle(tester);
    await tester.enterText(find.byType(TextField).last, 'scheduled');
    await tester.pump();
    await tester.tap(find.text('Scheduled goals').last);
    await settle(tester);
    expect(state.surface, HarborSurface.activity);
    expect(find.text('Your goals'), findsOneWidget);
    expect(find.text('New goal'), findsOneWidget);
  });

  group('Models with the Store tab', () {
    testWidgets(
        'tabs: Recommended · Installed · Hugging Face · Store · '
        'Benchmark; the Store keeps the office card', (tester) async {
      final state = await pumpShell(tester, FakeHarborService());
      state.goTo(HarborSurface.models);
      await settle(tester);
      final tabs = tester
          .widgetList<Tab>(find.descendant(
              of: find.byType(TabBar), matching: find.byType(Tab)))
          .map((t) => t.text);
      expect(tabs,
          ['Recommended', 'Installed', 'Hugging Face', 'Store', 'Benchmark']);
      await tester.tap(find.text('Store'));
      await settle(tester);
      expect(find.text('Harbor Office Suite'), findsOneWidget);
      expect(
          find.text('Part of the app you already have — nothing to download.'),
          findsOneWidget);
      await tester.tap(find.text("What's inside"));
      await settle(tester);
      expect(find.text('Live'), findsNWidgets(3));
      await tester.tap(find.text('Open in Work'));
      await settle(tester);
      expect(state.surface, HarborSurface.work);
    });

    testWidgets('Installed cards carry the SEC-024 uninstall preview',
        (tester) async {
      final s = FakeHarborService(models: [
        {
          'id': 'qwen-chat',
          'total_bytes': 1 << 30,
          'files': 1,
          'runtime': 'gguf'
        },
      ]);
      final state = await pumpShell(tester, s);
      state.openModels(ModelsTab.installed);
      await settle(tester);
      final uninstall =
          find.byKey(const ValueKey('models-uninstall-qwen-chat'));
      expect(uninstall, findsOneWidget);
      await tester.tap(uninstall);
      await settle(tester);
      expect(s.calls, contains('uninstallPreview:qwen-chat'));
      // The exact scope is previewed before anything moves.
      expect(find.text('Uninstall qwen-chat?'), findsOneWidget);
      expect(find.textContaining('Undo stays available for 72 hours'),
          findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await settle(tester);
      expect(find.text('Uninstall qwen-chat?'), findsNothing);
    });

    testWidgets('an embedder is never offered as the chat model',
        (tester) async {
      Map<String, dynamic> m(String id, String role) => {
            'id': id,
            'role': role,
            'total_bytes': 1 << 20,
            'files': 1,
            'runtime': 'gguf'
          };
      final s = FakeHarborService(models: [
        m('bge-small', 'embedder'),
        m('llama-chat', 'chat'),
        m('qwen-chat', 'chat'),
      ]);
      final state = await pumpShell(tester, s);
      state.openModels(ModelsTab.installed);
      await settle(tester);
      // The first chat model is in use by default (not the embedder that
      // sorts first); only the other chat model offers "Use for answers".
      expect(find.text('Use for answers'), findsOneWidget);
      expect(find.text('In use'), findsOneWidget);
    });
  });
}
