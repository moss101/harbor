import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/l10n/app_localizations.dart';
import 'package:harbor_office/main.dart';
import 'package:harbor_office/services/harbor_service.dart';
import 'package:harbor_office/services/preferences.dart';
import 'package:harbor_ui/harbor_ui.dart';

final _repoRoot = Directory.current.parent.parent.path;
final _dylib = '$_repoRoot/core/target/debug/libharbor_ffi.dylib';

// The phone path for editing a paragraph: tap the edit button, change the
// text, confirm. This crashed on Android with a framework assertion
// (`_dependents.isEmpty`) while the Work surface re-parented the document
// under the "draft restored" banner.
void main() {
  testWidgets('editing a paragraph on a phone saves without a framework error',
      (tester) async {
    if (!File(_dylib).existsSync()) return;
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    final dir = await tester.runAsync(
        () => Directory.systemTemp.createTemp('harbor-office-docedit-'));
    final service = await tester.runAsync(() => HarborService.open(
        libraryPath: _dylib,
        dataRoot: dir!.path,
        workspaceId: 'ws-docedit',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c'));
    addTearDown(() async => service?.close());
    await tester.runAsync(() async {
      await service!.refresh();
      final bytes = await service.createDocument(title: 'Quarterly report');
      await service.loadPreviewFromBytes(bytes, name: 'Quarterly report');
    });

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
        text: const HarborType(arabic: false),
        child: child!,
      ),
      home: HarborOfficeApp(
          service: service, preferences: MemoryPreferencesStore()),
    ));
    await tester.pump(const Duration(milliseconds: 300));

    await tester.tap(find.byTooltip('Edit paragraph 1'));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.enterText(find.byType(TextField).last, 'Quarterly report Q3');
    // The soft keyboard is up while the dialog is open ...
    tester.view.viewInsets = const FakeViewPadding(bottom: 300);
    addTearDown(tester.view.resetViewInsets);
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.text('OK'));
    // ... and closes while it fades out, which rebuilds the dialog's field.
    tester.view.viewInsets = FakeViewPadding.zero;
    // The dialog fades out over several frames while its text field is
    // still mounted: the controller must outlive them.
    for (var n = 0; n < 12; n++) {
      await tester.pump(const Duration(milliseconds: 40));
    }
    expect(tester.takeException(), isNull);
    // The edit runs through the real core on the real clock.
    for (var n = 0; n < 40; n++) {
      await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 50)));
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(tester.takeException(), isNull);
    expect(find.text('Quarterly report Q3'), findsWidgets);
  });
}
