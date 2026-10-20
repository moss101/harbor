import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/request_router.dart';
import 'package:harbor_app/surfaces/work_surface.dart';

import 'fake_service.dart';
import 'shell_test.dart' show coreAvailable, dylibPath, repoRoot, settleUntil;

/// The Work command bar against the REAL stack: the Flutter service, the
/// real core (the debug dylib), and real weights. Everything else covering
/// this path (work_command_bar_test.dart) uses a fake service.
///
/// Skips quietly when the dylib or the model fixtures are absent (they are
/// not in CI), the way the Rust live tests do.
void main() {
  final chatGguf =
      '$repoRoot/fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf';
  final embedGguf = '$repoRoot/fixtures/models/embeddinggemma-2-Q8_0.gguf';
  final haveModels =
      File(chatGguf).existsSync() && File(embedGguf).existsSync();

  const paragraphs = [
    'Acme Supplies agreement between Acme Supplies and Harbor Ltd.',
    'Payment terms are net 45 days from the invoice date.',
    'The agreement renews automatically each January.',
  ];

  /// A real .docx, written with the system `zip` (no extra dependency).
  Future<File> buildDocx(Directory dir) async {
    final src = Directory('${dir.path}/docx-src')..createSync(recursive: true);
    Directory('${src.path}/_rels').createSync();
    Directory('${src.path}/word').createSync();
    File('${src.path}/[Content_Types].xml').writeAsStringSync(
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        '</Types>');
    File('${src.path}/_rels/.rels').writeAsStringSync(
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
        '</Relationships>');
    final body =
        paragraphs.map((p) => '<w:p><w:r><w:t>$p</w:t></w:r></w:p>').join();
    File('${src.path}/word/document.xml').writeAsStringSync(
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        '<w:body>$body</w:body></w:document>');
    final out = File('${dir.path}/Acme contract.docx');
    final r = await Process.run(
        'zip', ['-X', '-r', out.path, '[Content_Types].xml', '_rels', 'word'],
        workingDirectory: src.path);
    expect(r.exitCode, 0, reason: 'zip failed: ${r.stderr}');
    return out;
  }

  Future<HarborService> openLive(WidgetTester tester) async {
    late HarborService s;
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-work-live-');
      s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-work-live',
          deviceRootHex:
              '9a2b1c0d8e7f60514233445566778899aabbccddeeff00112233445566778899');
      expect(
          await s.installModelFromPath(
              packageId: 'qwen2.5-1.5b-chat', path: chatGguf),
          isTrue,
          reason: 'chat model must install');
      expect(
          await s.installModelFromPath(
              packageId: 'embeddinggemma-2', path: embedGguf),
          isTrue,
          reason: 'embedding model must install');
      final docx = await buildDocx(dir);
      await s.loadPreviewFromBytes(await docx.readAsBytes(),
          name: 'Acme contract.docx', path: docx.path);
    });
    addTearDown(() => s.close());
    expect(s.preview, isNotNull, reason: s.previewError);
    return s;
  }

  Future<void> send(WidgetTester tester, String text) async {
    await tester.enterText(find.byKey(const ValueKey('composer-field')), text);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('composer-send')));
    await tester.pump();
  }

  testWidgets(
      'LIVE Ask about this file: grounded answer with a locator citation, '
      'then an unsupported question abstains', (tester) async {
    if (!coreAvailable || !haveModels) {
      markTestSkipped('dylib or model fixtures absent');
      return;
    }
    final s = await openLive(tester);
    await pumpSurface(tester, s, (_) => const WorkSurface());
    expect(find.byKey(const ValueKey('work-command-bar')), findsOneWidget);
    await tester.tap(find.text('Ask'));
    await tester.pump();

    await send(tester, 'What are the payment terms?');
    final cited = find.text('Paragraph 2');
    await settleUntil(tester, cited,
        what: 'the grounded answer to cite paragraph 2');
    expect(
        tester
            .widgetList<Text>(find.byType(Text))
            .map((t) => t.data ?? '')
            .any((t) => t.contains('45')),
        isTrue,
        reason: 'the answer should state the 45-day term');

    await send(tester, "What is the name of Harbor Ltd's chief executive?");
    await settleUntil(tester, find.textContaining('could not find'),
        what: 'an abstention', budget: const Duration(seconds: 120));
  }, timeout: const Timeout(Duration(minutes: 8)));

  test('LIVE askAboutFile response contract (service + core + model)',
      () async {
    if (!coreAvailable || !haveModels) {
      markTestSkipped('dylib or model fixtures absent');
      return;
    }
    final dir = await Directory.systemTemp.createTemp('harbor-work-live2-');
    final s = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-work-live2',
        deviceRootHex:
            '1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809');
    addTearDown(s.close);
    expect(
        await s.installModelFromPath(
            packageId: 'qwen2.5-1.5b-chat', path: chatGguf),
        isTrue);
    final docx = await buildDocx(dir);
    await s.loadPreviewFromBytes(await docx.readAsBytes(),
        name: 'Acme contract.docx');
    expect(s.preview, isNotNull, reason: s.previewError);
    final runId = HarborService.newRunId();
    await s.createRun(runId);
    final a = await s.askAboutFile('What are the payment terms?',
        chatPackage: 'qwen2.5-1.5b-chat', runId: runId, maxTokens: 64);
    expect(a['used_citations'], true, reason: '$a');
    expect(a['answer'] as String, contains('45'));
    expect((a['scope'] as Map)['kind'], 'document');
    final cites = (a['citations'] as List).cast<Map>();
    final pay = cites.firstWhere(
        (c) => c['snippet'].toString().contains('net 45'),
        orElse: () => fail('no citation for the payment paragraph: $cites'));
    expect(pay['passage_id'], 'para-2');
    expect(pay['locator'], '¶2');
    expect(pay['score'], greaterThan(0));
    expect(cites.every((c) => c['title'] == 'Acme contract.docx'), isTrue);

    final none = await s.askAboutFile(
        "What is the name of Harbor Ltd's chief executive?",
        chatPackage: 'qwen2.5-1.5b-chat',
        maxTokens: 24);
    expect((none['answer'] as String).trim(), 'INSUFFICIENT_EVIDENCE');
  }, timeout: const Timeout(Duration(minutes: 8)));

  test('LIVE Change this file: the real router returns a runnable skill',
      () async {
    if (!coreAvailable || !haveModels) {
      markTestSkipped('dylib or model fixtures absent');
      return;
    }
    final dir = await Directory.systemTemp.createTemp('harbor-work-live3-');
    final s = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-work-live3',
        deviceRootHex:
            '2a2b3c4d5e6f708192a3b4c5d6e7f8092a2b3c4d5e6f708192a3b4c5d6e7f809');
    addTearDown(s.close);
    await s.installModelFromPath(
        packageId: 'qwen2.5-1.5b-chat', path: chatGguf);
    await s.installModelFromPath(
        packageId: 'embeddinggemma-2', path: embedGguf);
    final sw = Stopwatch()..start();
    expect(await s.openKnowledge(packageId: 'embeddinggemma-2'), isTrue);
    // ignore: avoid_print
    print('live: knowledge open in ${sw.elapsed}');
    await s.refresh();
    expect(s.skills, isNotEmpty);

    sw.reset();
    final decision = await RequestRouter(s).route(
      'Review the style and formatting of this document',
      file: const RouteFile(name: 'Acme contract.docx'),
      mode: RequestMode.change,
      chatPackage: 'qwen2.5-1.5b-chat',
    );
    // ignore: avoid_print
    print('live route (${sw.elapsed}): $decision'
        '${decision is RouteNone ? ' reason=${decision.reason}' : ''}');
    expect(decision, isA<RouteSkill>(), reason: '$decision');
    final skill = (decision as RouteSkill).skill;
    expect(RequestRouter.fits(skill, hasFile: true), isTrue);
    // ignore: avoid_print
    print('live route picked: ${skill.id}');
  }, timeout: const Timeout(Duration(minutes: 20)));
}
