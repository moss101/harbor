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
import 'package:harbor_ui/deck_designer.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';

/// "Save a copy" on the Work surface, against the real core: the action
/// exists only while the working copy differs from the opened file, saves
/// the restyled bytes to a new file and never writes the source.
String? skipReason() {
  final lib = File(dylibPath);
  if (!lib.existsSync()) return 'libharbor_ffi.dylib is not built';
  if (!latin1.decode(lib.readAsBytesSync()).contains('deck.restyle')) {
    return 'libharbor_ffi.dylib predates the deck.* FFI arms';
  }
  return null;
}

const channel = MethodChannel('plugins.flutter.io/file_selector');

Future<void> settleReal(WidgetTester tester, bool Function() done) async {
  for (var i = 0; i < 200 && !done(); i++) {
    await tester
        .runAsync(() => Future<void>.delayed(const Duration(milliseconds: 25)));
    await tester.pump(const Duration(milliseconds: 25));
  }
  expect(done(), isTrue, reason: 'timed out');
}

void main() {
  test('copyFileName never equals the opened name', () {
    expect(copyFileName('Deck.pptx'), 'Deck copy.pptx');
    expect(copyFileName('a.b.c.pptx'), 'a.b.c copy.pptx');
    expect(copyFileName('noext'), 'noext copy');
    expect(copyFileName('.hidden'), '.hidden copy');
  });

  testWidgets(
      'the action appears after a restyle, saves the restyled bytes as a '
      'new file and leaves the source alone', (tester) async {
    final skip = skipReason();
    if (skip != null) {
      markTestSkipped(skip);
      return;
    }
    tester.view.physicalSize = const Size(1200, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    late HarborService service;
    late Directory dir;
    late DeckStarterResult starter;
    await tester.runAsync(() async {
      dir = await Directory.systemTemp.createTemp('harbor-work-savecopy-');
      service = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: '${dir.path}/data',
          workspaceId: 'ws-work-savecopy',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
      starter = await service.deckStarter(
          purpose: 'pitch_deck',
          style: const DeckStyleChoice(design: 'ocean', motion: 'subtle'));
      await service.loadPreviewFromBytes(starter.bytes, name: starter.fileName);
    });
    addTearDown(() => service.close());
    addTearDown(() => dir.deleteSync(recursive: true));

    // The save picker: records the suggested name, answers `answer`.
    String? suggested;
    String? answer;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      if (call.method == 'getSavePath') {
        suggested = (call.arguments as Map)['suggestedName'] as String?;
        return answer;
      }
      return null;
    });
    addTearDown(() => TestDefaultBinaryMessengerBinding
        .instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null));

    await tester.pumpWidget(AppStateScope(
      state: AppState(store: MemoryPreferencesStore()),
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
          home: Scaffold(
            body: HarborServiceProvider(
                failed: false, service: service, child: const WorkSurface()),
          ),
        ),
      ),
    ));
    await tester.pump();

    final save = find.byKey(const ValueKey('work-save-copy'));
    // Before any change: the deck is exactly what was opened.
    expect(service.hasUnsavedChanges, isFalse);
    expect(find.text(starter.fileName), findsWidgets);
    expect(save, findsNothing);

    // Restyle: the working copy now differs from the opened file.
    late DeckRestyleResult restyled;
    await tester.runAsync(() async {
      restyled = await service.restyleOpenDeck(
          const DeckStyleChoice(design: 'slate', motion: 'subtle', rtl: false));
    });
    await tester.pump();
    expect(service.hasUnsavedChanges, isTrue);
    expect(save, findsOneWidget);
    expect(find.byTooltip('Save a copy'), findsOneWidget);
    expect(tester.getSize(save).height, greaterThanOrEqualTo(44));
    expect(tester.getSize(save).width, greaterThanOrEqualTo(44));

    // Cancelled picker: nothing is written and the action stays.
    answer = null;
    await tester.tap(save);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(suggested, '${_stem(starter.fileName)} copy.pptx');
    expect(save, findsOneWidget);
    expect(service.hasUnsavedChanges, isTrue);

    // A chosen destination: the restyled bytes land there.
    final dest = '${dir.path}/kept.pptx';
    answer = dest;
    await tester.tap(save);
    await settleReal(tester, () => !service.hasUnsavedChanges);
    final written = File(dest).readAsBytesSync();
    expect(written, restyled.bytes);
    expect(written, isNot(starter.bytes));
    expect(service.currentWorkingBytes(), restyled.bytes);
    expect(find.text('Saved kept.pptx'), findsOneWidget);
    // Saved: nothing pending, so the action is gone; the opened bytes held
    // by the caller were never touched.
    await tester.pump(const Duration(milliseconds: 100));
    expect(save, findsNothing);
    expect(starter.bytes, isNotEmpty);

    // A further restyle brings it back.
    await tester.runAsync(() => service.restyleOpenDeck(
        const DeckStyleChoice(design: 'ivory', motion: 'subtle')));
    await tester.pump();
    expect(save, findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a failed write is reported and the action stays',
      (tester) async {
    final skip = skipReason();
    if (skip != null) {
      markTestSkipped(skip);
      return;
    }
    tester.view.physicalSize = const Size(1200, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    late HarborService service;
    late Directory dir;
    await tester.runAsync(() async {
      dir = await Directory.systemTemp.createTemp('harbor-work-savefail-');
      service = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: '${dir.path}/data',
          workspaceId: 'ws-work-savefail',
          deviceRootHex:
              'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
      final r = await service.deckStarter(
          purpose: 'pitch_deck',
          style: const DeckStyleChoice(design: 'ocean', motion: 'subtle'));
      await service.loadPreviewFromBytes(r.bytes, name: r.fileName);
      await service.restyleOpenDeck(
          const DeckStyleChoice(design: 'slate', motion: 'subtle'));
    });
    addTearDown(() => service.close());
    addTearDown(() => dir.deleteSync(recursive: true));
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      // A folder that does not exist: the write fails.
      return call.method == 'getSavePath' ? '${dir.path}/nope/x.pptx' : null;
    });
    addTearDown(() => TestDefaultBinaryMessengerBinding
        .instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null));
    await tester.pumpWidget(AppStateScope(
      state: AppState(store: MemoryPreferencesStore()),
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
          home: Scaffold(
            body: HarborServiceProvider(
                failed: false, service: service, child: const WorkSurface()),
          ),
        ),
      ),
    ));
    await tester.pump();
    final save = find.byKey(const ValueKey('work-save-copy'));
    expect(save, findsOneWidget);
    await tester.tap(save);
    await settleReal(
        tester,
        () => find
            .textContaining("Couldn't save the copy")
            .evaluate()
            .isNotEmpty);
    expect(service.hasUnsavedChanges, isTrue);
    expect(save, findsOneWidget);
  });
}

String _stem(String name) => name.substring(0, name.lastIndexOf('.'));
