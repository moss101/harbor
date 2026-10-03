import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/preferences.dart';
import 'package:harbor_app/surfaces/work_surface.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';

/// The Work surface's "New presentation" entry, against the real core
/// (HarborService is not fakeable). Skipped when the built dylib is missing
/// or predates the `deck.*` arms.
String? skipReason() {
  final lib = File(dylibPath);
  if (!lib.existsSync()) return 'libharbor_ffi.dylib is not built';
  if (!latin1.decode(lib.readAsBytesSync()).contains('deck.restyle')) {
    return 'libharbor_ffi.dylib predates the deck.* FFI arms';
  }
  return null;
}

Future<void> until(WidgetTester tester, Finder f) async {
  for (var i = 0; i < 300; i++) {
    await tester
        .runAsync(() => Future<void>.delayed(const Duration(milliseconds: 50)));
    await tester.pump(const Duration(milliseconds: 50));
    if (f.evaluate().isNotEmpty) return;
  }
  fail('timed out waiting for $f');
}

void main() {
  testWidgets(
      'empty Work surface offers New presentation; creating previews the '
      'deck, remembers the choices and respects a cancelled save',
      (tester) async {
    final skip = skipReason();
    if (skip != null) {
      markTestSkipped(skip);
      return;
    }
    tester.view.physicalSize = const Size(1200, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    // A cancelled save dialog: the picker answers null.
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
            const MethodChannel('plugins.flutter.io/file_selector'),
            (call) async => null);
    addTearDown(() => TestDefaultBinaryMessengerBinding
        .instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
            const MethodChannel('plugins.flutter.io/file_selector'), null));

    late HarborService service;
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-work-newdeck-');
      addTearDown(() => dir.delete(recursive: true));
      service = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-work-newdeck',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
    });
    addTearDown(() => service.close());

    final store = MemoryPreferencesStore();
    final state = AppState(store: store);
    await tester.pumpWidget(AppStateScope(
      state: state,
      child: HarborTheme(
        colors: HarborColors.light,
        text: const HarborType(arabic: false),
        child: MaterialApp(
          localizationsDelegates: const [
            AppLocalizations.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          supportedLocales: const [Locale('en'), Locale('ar')],
          home: HarborServiceProvider(
              failed: false, service: service, child: const WorkSurface()),
        ),
      ),
    ));
    await tester.pump();

    final button = find.byKey(const ValueKey('work-new-presentation'));
    expect(button, findsOneWidget);
    expect(find.text('New presentation'), findsOneWidget);
    final size = tester.getSize(button);
    expect(size.height, greaterThanOrEqualTo(44));

    await tester.tap(button);
    await tester.pump();
    // The real catalogue arrives: purposes are listed.
    await until(tester, find.text('What is this presentation for?'));
    expect(find.byType(FilledButton), findsWidgets);

    // Purpose -> Design -> Motion -> Create, with the defaults.
    for (var i = 0; i < 2; i++) {
      await tester.tap(find.widgetWithText(FilledButton, 'Next'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 100));
    }
    await tester.tap(find.text('Create presentation'));
    await tester.pump();
    await until(tester, find.byIcon(Icons.slideshow_outlined));
    // Back on the Work surface with the new deck open (sheet popped).
    for (var i = 0;
        i < 100 && (service.preview == null || service.previewLoading);
        i++) {
      await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 50)));
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(service.preview, isNotNull);
    expect(service.previewName, endsWith('.pptx'));
    final data = service.preview!['preview'] as Map;
    expect((data['slides'] as List), isNotEmpty);
    expect((data['design'] as Map)['restylable'], isTrue);
    // The choices were remembered through the app's preference store.
    expect(state.deckDesign, isNotNull);
    expect(state.deckDesign!.purpose, isNotNull);
    expect(store.current.deckDesign, state.deckDesign);
    expect(tester.takeException(), isNull);
  });

  test('AppState persists the last New presentation choices', () async {
    final store = MemoryPreferencesStore();
    final state = AppState(store: store);
    expect(state.deckDesign, isNull);
    const d = DeckDesignPrefs(
        purpose: 'pitch_deck', design: 'ocean', palette: 'mint', rtl: true);
    state.setDeckDesign(d);
    expect(state.deckDesign, d);
    final reloaded = AppState(store: store);
    await reloaded.load();
    expect(reloaded.deckDesign, d);
  });
}
