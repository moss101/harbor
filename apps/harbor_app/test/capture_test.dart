import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/capture.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/surfaces/capture_flow.dart';
import 'package:harbor_app/surfaces/home_surface.dart';

import 'ops_test.dart' show pumpWithL10n;

final repoRoot = Directory.current.parent.parent.path;
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('HarborCapture channel contract', () {
    const channel = MethodChannel('dev.harbor/capture');
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    tearDown(() => messenger.setMockMethodCallHandler(channel, null));

    test('a platform without the channel supports nothing', () async {
      final s = await HarborCapture().support();
      expect((s.record, s.photo), (false, false));
    });

    test('support, record and photo round-trip; denial is typed', () async {
      messenger.setMockMethodCallHandler(channel, (call) async {
        switch (call.method) {
          case 'support':
            return {'record': true, 'photo': false};
          case 'recordStart':
            throw PlatformException(code: 'permission');
          case 'recordStop':
            return '/tmp/a.m4a';
          case 'takePhoto':
            return null;
        }
        return null;
      });
      final cap = HarborCapture();
      final s = await cap.support();
      expect((s.record, s.photo), (true, false));
      await expectLater(
          cap.recordStart(), throwsA(isA<CapturePermissionDenied>()));
      expect(await cap.recordStop(), '/tmp/a.m4a');
      expect(await cap.takePhoto(), isNull); // cancelled camera
    });
  });

  testWidgets('every action kind has a label in both languages',
      (tester) async {
    for (final locale in const [Locale('en'), Locale('ar')]) {
      await tester.pumpWidget(MaterialApp(
        locale: locale,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: const SizedBox(),
      ));
      final l10n = await AppLocalizations.delegate.load(locale);
      for (final id in const [
        'run_skill',
        'search_documents',
        'remember_fact',
        'recall_memory',
        'schedule_goal',
        'read_scan',
        'transcribe_audio',
        'general_chat',
      ]) {
        expect(intentLabel(id, l10n), isNot(id), reason: '$id in $locale');
      }
    }
  });

  testWidgets(
      'the review dialog shows engine, pages to check and the '
      'suggestion, and returns the EDITED text', (tester) async {
    String? result = 'unset';
    final l10n = await pumpWithL10n(tester, (context) {
      return TextButton(
        onPressed: () async => result = await showReviewDialog(
          context,
          title: 'Review',
          text: 'Invoice 1001 4,200',
          engine: 'apple-vision',
          checkPages: const ['2', '3'],
          suggestion: 'This looks like: Read a scan',
        ),
        child: const Text('open'),
      );
    });
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(find.text(l10n.captureEngine('apple-vision')), findsOneWidget);
    expect(find.byKey(const ValueKey('capture-check-pages')), findsOneWidget);
    expect(find.byKey(const ValueKey('capture-suggestion')), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('capture-review-text')),
        'Invoice 1001 4,250');
    await tester.tap(find.byKey(const ValueKey('capture-use')));
    await tester.pumpAndSettle();
    expect(result, 'Invoice 1001 4,250');

    // Discard returns nothing.
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('capture-discard')));
    await tester.pumpAndSettle();
    expect(result, isNull);
  });

  testWidgets(
      'the composer offers scan and record only when wired, and '
      'neither sends anything', (tester) async {
    var scans = 0, records = 0;
    final sent = <String>[];
    await pumpWithL10n(
        tester,
        (_) => Composer(
              hint: 'ask',
              onSubmit: sent.add,
              onScan: () => scans++,
              onRecord: () => records++,
            ));
    await tester.tap(find.byKey(const ValueKey('composer-scan')));
    await tester.tap(find.byKey(const ValueKey('composer-record')));
    expect(scans, 1);
    expect(records, 1);
    expect(sent, isEmpty);
    await pumpWithL10n(tester, (_) => const Composer(hint: 'ask'));
    expect(find.byKey(const ValueKey('composer-scan')), findsNothing);
    expect(find.byKey(const ValueKey('composer-record')), findsNothing);
  });

  testWidgets(
      'a package that cannot read scans is refused with a typed '
      'error, not silently accepted', (tester) async {
    if (!File(dylibPath).existsSync()) return;
    await tester.runAsync(() async {
      final dir = Directory.systemTemp.createTempSync('harbor-scan-model-');
      final service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-scan-model',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c',
      );
      addTearDown(() {
        service.close();
        dir.deleteSync(recursive: true);
      });
      Object? refused;
      try {
        await service.useForScanning('no-such-package');
      } catch (e) {
        refused = e;
      }
      expect(refused, isNotNull);
      expect(service.scanModel, isNull);
      // Releasing with nothing chosen is a no-op, not an error.
      await service.releaseScanModel();
    });
  });
}
