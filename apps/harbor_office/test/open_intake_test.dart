import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/l10n/app_localizations.dart';
import 'package:harbor_office/main.dart';
import 'package:harbor_office/services/harbor_service.dart';
import 'package:harbor_office/services/open_intake.dart';
import 'package:harbor_office/services/preferences.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path;
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// A fake native side: answers `getInitialPath` and can push `openPath`.
class FakeNative {
  FakeNative(this.initial) {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      if (call.method == 'getInitialPath') {
        final v = initial;
        initial = null; // consumed once, like the real embedders
        return v;
      }
      return null;
    });
  }
  final channel = const MethodChannel(OpenIntake.channelName);
  Object? initial;

  Future<void> push(Map<String, String> payload) async {
    final codec = channel.codec;
    await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .handlePlatformMessage(channel.name,
            codec.encodeMethodCall(MethodCall('openPath', payload)), (_) {});
  }

  void dispose() => TestDefaultBinaryMessengerBinding
      .instance.defaultBinaryMessenger
      .setMockMethodCallHandler(channel, null);
}

Future<String> _temp(Directory dir, String name, List<int> bytes) async {
  final d = await Directory('${dir.path}/open-intake/${name.hashCode}')
      .create(recursive: true);
  final f = File('${d.path}/$name');
  await f.writeAsBytes(bytes);
  return f.path;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory root;
  late Directory opened;

  setUp(() async {
    root = await Directory.systemTemp.createTemp('open-intake-test-');
    opened = await Directory('${root.path}/Opened').create();
  });
  tearDown(() async => root.delete(recursive: true));

  OpenIntake intake(FakeNative native) =>
      OpenIntake(openedDir: () async => opened);

  group('name handling', () {
    test('sanitizeIncomingName strips paths, controls and leading dots', () {
      expect(sanitizeIncomingName('../../etc/passwd.docx'), 'passwd.docx');
      expect(sanitizeIncomingName(r'..\..\evil.xlsx'), 'evil.xlsx');
      expect(sanitizeIncomingName('.hidden.pdf'), 'hidden.pdf');
      expect(sanitizeIncomingName('a<b>:c|d?.docx'), 'a_b__c_d_.docx');
      expect(sanitizeIncomingName('..'), '');
    });

    test('long names keep their extension', () {
      final n = sanitizeIncomingName('${'x' * 300}.xlsx');
      expect(n.length, 120);
      expect(n.endsWith('.xlsx'), isTrue);
    });
  });

  group('launch document', () {
    test('is imported into Opened/, temp copy removed, emitted once', () async {
      final tmp = await _temp(root, 'Budget.xlsx', [1, 2, 3]);
      final native = FakeNative({'path': tmp, 'name': 'Budget.xlsx'});
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      await Future<void>.delayed(Duration.zero);
      expect(got, hasLength(1));
      expect(got.single.ok, isTrue);
      expect(got.single.path, '${opened.path}/Budget.xlsx');
      expect(await File(got.single.path!).readAsBytes(), [1, 2, 3]);
      expect(File(tmp).existsSync(), isFalse);
      expect(File(tmp).parent.existsSync(), isFalse);
      // A second start is a no-op: the launch document is not re-delivered.
      await i.start();
      await Future<void>.delayed(Duration.zero);
      expect(got, hasLength(1));
      await i.dispose();
    });

    test('no launch document emits nothing', () async {
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      await Future<void>.delayed(Duration.zero);
      expect(got, isEmpty);
      await i.dispose();
    });
  });

  group('warm delivery', () {
    test('pushed openPath is emitted', () async {
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      await native.push({
        'path': await _temp(root, 'Memo.docx', [9]),
        'name': 'Memo.docx'
      });
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(got.single.name, 'Memo.docx');
      expect(got.single.ok, isTrue);
      await i.dispose();
    });

    test('unsupported type is refused and its temp copy discarded', () async {
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      final tmp = await _temp(root, 'notes.exe', [1]);
      await native.push({'path': tmp, 'name': 'notes.exe'});
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(got.single.failure, OpenFailure.unsupportedType);
      expect(got.single.name, 'notes.exe');
      expect(File(tmp).existsSync(), isFalse);
      expect(opened.listSync(), isEmpty);
      await i.dispose();
    });

    test('native error payload becomes an unreadable failure', () async {
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      await native.push({'error': 'too_large', 'name': 'Huge.pdf'});
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(got.single.failure, OpenFailure.unreadable);
      expect(got.single.name, 'Huge.pdf');
      await i.dispose();
    });

    test('a missing temp file is unreadable, not a crash', () async {
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      await native.push({'path': '${root.path}/gone.docx', 'name': 'gone.docx'});
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(got.single.failure, OpenFailure.unreadable);
      await i.dispose();
    });

    test('a hostile name cannot escape Opened/', () async {
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      await native.push({
        'path': await _temp(root, 'x.docx', [1]),
        'name': '../../../escape.docx'
      });
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(got.single.path, '${opened.path}/escape.docx');
      expect(File('${root.path}/escape.docx').existsSync(), isFalse);
      await i.dispose();
    });
  });

  group('store collisions', () {
    test('identical bytes reuse the file; different bytes get a new name',
        () async {
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final i = intake(native);
      final got = <OpenRequest>[];
      i.requests.listen(got.add);
      await i.start();
      Future<void> send(List<int> bytes, String tag) async {
        await native.push({
          'path': await _temp(root, 'Plan.docx', Uint8List.fromList(bytes)),
          'name': 'Plan.docx'
        });
        await Future<void>.delayed(const Duration(milliseconds: 50));
      }

      await send([1, 2, 3], 'a');
      await send([1, 2, 3], 'b'); // same document again
      await send([4, 5, 6], 'c'); // different document, same name
      expect(got.map((r) => r.path!.split('/').last),
          ['Plan.docx', 'Plan.docx', 'Plan (2).docx']);
      expect(opened.listSync(), hasLength(2));
      await i.dispose();
    });
  });

  group('Work surface integration (real core)', () {
    late HarborService? service;
    setUpAll(() async {
      service = null;
      if (coreAvailable) {
        final dir =
            await Directory.systemTemp.createTemp('harbor-office-open-');
        service = await HarborService.open(
            libraryPath: dylibPath,
            dataRoot: dir.path,
            workspaceId: 'ws-office-open',
            deviceRootHex:
                'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c');
        await service!.refresh();
      }
    });
    tearDownAll(() async => service?.close());

    Widget app(OpenIntake i, MemoryPreferencesStore prefs) => MaterialApp(
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
              service: service, preferences: prefs, openIntake: i),
        );

    testWidgets('a delivered workbook opens in Work and lands in recents',
        (tester) async {
      if (!coreAvailable) return;
      await tester.runAsync(() async {});
      final src = File('$repoRoot/fixtures/office/board_demo.xlsx');
      final tmp = await tester.runAsync(
          () => _temp(root, 'board_demo.xlsx', src.readAsBytesSync()));
      final native = FakeNative({'path': tmp, 'name': 'board_demo.xlsx'});
      addTearDown(native.dispose);
      final prefs = MemoryPreferencesStore();
      final i = intake(native);
      await tester.pumpWidget(app(i, prefs));
      // The intake does real file I/O: drive it on the real clock.
      for (var n = 0; n < 40; n++) {
        await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)));
        await tester.pump(const Duration(milliseconds: 100));
        if (prefs.current.recentFiles.isNotEmpty) break;
      }
      expect(prefs.current.recentFiles, isNotEmpty);
      final recent = prefs.current.recentFiles.first;
      expect(recent.name, 'board_demo.xlsx');
      expect(recent.kind, 'workbook');
      expect(recent.path, startsWith(opened.path));
      expect(find.text('board_demo.xlsx'), findsWidgets);
    });

    testWidgets('an unsupported delivery is stated, not swallowed',
        (tester) async {
      if (!coreAvailable) return;
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final prefs = MemoryPreferencesStore();
      final i = intake(native);
      await tester.pumpWidget(app(i, prefs));
      await tester.pump();
      final tmp = await tester.runAsync(() => _temp(root, 'virus.exe', [1]));
      await tester.runAsync(() => native.push({'path': tmp!, 'name': 'virus.exe'}));
      for (var n = 0; n < 20; n++) {
        await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)));
        await tester.pump(const Duration(milliseconds: 100));
        if (find.textContaining("Can't open virus.exe").evaluate().isNotEmpty) {
          break;
        }
      }
      expect(find.textContaining("Can't open virus.exe"), findsOneWidget);
      expect(prefs.current.recentFiles, isEmpty);
    });

    testWidgets('a non-document with a document extension hits the core error',
        (tester) async {
      if (!coreAvailable) return;
      final native = FakeNative(null);
      addTearDown(native.dispose);
      final prefs = MemoryPreferencesStore();
      final i = intake(native);
      await tester.pumpWidget(app(i, prefs));
      await tester.pump();
      final tmp = await tester
          .runAsync(() => _temp(root, 'fake.docx', 'not a zip'.codeUnits));
      await tester.runAsync(() => native.push({'path': tmp!, 'name': 'fake.docx'}));
      for (var n = 0; n < 30; n++) {
        await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)));
        await tester.pump(const Duration(milliseconds: 100));
        if (find.byType(HarborErrorState).evaluate().isNotEmpty) break;
      }
      expect(find.byType(HarborErrorState), findsOneWidget);
      // A refused file must not become a "recent".
      expect(prefs.current.recentFiles, isEmpty);
    });
  });
}
