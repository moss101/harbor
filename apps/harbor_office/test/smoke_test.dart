import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/l10n/app_localizations.dart';
import 'package:harbor_office/main.dart';
import 'package:harbor_office/services/harbor_service.dart';
import 'package:harbor_office/services/preferences.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_office
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// The suite renders its two surfaces. With the real dylib the Work
/// canvas is fully live; without it the honest degraded state renders.
void main() {
  late HarborService? service;
  setUpAll(() async {
    service = null;
    if (coreAvailable) {
      final dir = await Directory.systemTemp.createTemp('harbor-office-test-');
      service = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-office',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
      await service!.refresh();
    }
  });

  tearDownAll(() async {
    if (service != null) await service!.close();
  });

  testWidgets('Work and Settings render; navigation switches surfaces',
      (tester) async {
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
      home: HarborOfficeApp(
          service: service, preferences: MemoryPreferencesStore()),
    ));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    // Work is the first surface. With the core: the empty state with the
    // convert action; without: the honest degraded state.
    if (coreAvailable) {
      expect(find.text('Open file'), findsOneWidget);
    } else {
      expect(find.byType(HarborErrorState), findsOneWidget);
    }
    // Settings is reachable on the bottom bar.
    await tester.tap(find.text('Settings').last);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Settings'), findsWidgets);
  });
}
