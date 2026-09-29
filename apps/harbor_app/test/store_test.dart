import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/preferences.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// The Store surface: Harbor Office is an INCLUDED product card (never a
/// download), the first-party catalog quotes before transfer, and the
/// installed section manages uninstalls. This test exercises the render
/// path against the real dylib when it is available.
Future<void> pumpApp(WidgetTester tester,
    {double width = 390, double height = 844}) async {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  HarborService? service;
  if (coreAvailable) {
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-store-test-');
      final s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-store',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
      await s.refresh();
      service = s;
    });
  }
  addTearDown(() => service?.close());
  await tester.pumpWidget(MaterialApp(
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: const [Locale('en')],
    theme: harborThemeData(dark: false, arabic: false),
    builder: (_, child) => HarborTheme(
      colors: HarborColors.light,
      text: HarborType(arabic: false),
      child: child!,
    ),
    home: HarborApp(
        service: service, preferences: MemoryPreferencesStore()),
  ));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
}

void main() {
  testWidgets('store opens from More with the included office card',
      (tester) async {
    if (!coreAvailable) return;
    await pumpApp(tester);
    await tester.tap(find.text('More').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.tap(find.text('Store').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    // The office card is a status card: included, with an Open Work
    // action — and there is no "download office" control anywhere.
    expect(find.text('Harbor Office'), findsOneWidget);
    expect(find.text('Included with this app'), findsOneWidget);
    expect(find.text('Open Work'), findsOneWidget);
    expect(find.text('Download office'), findsNothing);
    // Open Work navigates to the Work surface.
    await tester.tap(find.text('Open Work'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('Work'), findsWidgets);
  });
}
