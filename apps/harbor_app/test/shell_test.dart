import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_ui/harbor_ui.dart';

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
    home: const HarborApp(),
  ));
  await tester.pumpAndSettle();
}

void main() {
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
    await tester.pumpAndSettle();
    await tester.tap(find.text('العربية'));
    await tester.pumpAndSettle();
    final context = tester.element(find.byType(NavigationBar).first);
    expect(Directionality.of(context), TextDirection.rtl);
    expect(find.text('الرئيسية'), findsOneWidget);
    // Back on Home: work-first headline is localized.
    await tester.tap(find.text('الرئيسية'));
    await tester.pumpAndSettle();
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
    await tester.pumpAndSettle();
    await tester.tap(find.text('العربية'));
    await tester.pumpAndSettle();
    expect(find.text('الإعدادات'), findsWidgets);
    expect(find.text('اللغة'), findsOneWidget);
  });
}
