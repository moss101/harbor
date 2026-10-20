import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_app/services/request_router.dart';
import 'package:harbor_app/surfaces/skill_run.dart';
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
  Future<File> buildDocx(Directory dir,
      {List<String> texts = paragraphs,
      String name = 'Acme contract.docx',
      bool rtl = false,
      List<String?>? styles,
      Set<int> spaced = const {}}) async {
    final src = Directory('${dir.path}/docx-src-${name.hashCode}')
      ..createSync(recursive: true);
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
    final rpr = rtl ? '<w:rPr><w:rtl/></w:rPr>' : '';
    String pPr(int i) {
      final style = styles?[i];
      final inner = '${style == null ? '' : '<w:pStyle w:val="$style"/>'}'
          '${rtl ? '<w:bidi/>' : ''}'
          '${spaced.contains(i) ? '<w:spacing w:before="0" w:after="0"/>' : ''}';
      return inner.isEmpty ? '' : '<w:pPr>$inner</w:pPr>';
    }

    final body = [
      for (var i = 0; i < texts.length; i++)
        '<w:p>${pPr(i)}<w:r>$rpr<w:t xml:space="preserve">${texts[i]}</w:t></w:r></w:p>'
    ].join();
    File('${src.path}/word/document.xml').writeAsStringSync(
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        '<w:body>$body</w:body></w:document>');
    final out = File('${dir.path}/$name');
    final r = await Process.run(
        'zip', ['-X', '-r', out.path, '[Content_Types].xml', '_rels', 'word'],
        workingDirectory: src.path);
    expect(r.exitCode, 0, reason: 'zip failed: ${r.stderr}');
    return out;
  }

  /// A small Arabic service agreement: one payment clause (¶5, Arabic-Indic
  /// digits) among enough other clauses that retrieval has to choose (the
  /// core sends at most 8 passages to the model).
  List<String> arabicContract() {
    const clauses = [
      'اتفاقية توريد بين شركة الأفق للتجارة وشركة هاربر المحدودة.',
      'يلتزم المورد بتسليم البضائع إلى مستودع المشتري في مدينة جدة.',
      'تبدأ مدة الاتفاقية في الأول من مارس وتنتهي بعد سنة واحدة.',
      'تتجدد الاتفاقية تلقائياً في شهر يناير من كل عام ما لم يخطر أحد الطرفين الآخر.',
      'شروط الدفع: يتم سداد كل فاتورة خلال ٤٥ يوماً من تاريخ استلامها.',
      'تقع مسؤولية الشحن والتأمين على عاتق المورد حتى ميناء الوصول.',
      'يحق للمشتري رفض أي شحنة لا تطابق المواصفات المتفق عليها خلال خمسة أيام عمل.',
      'تخضع هذه الاتفاقية لأنظمة المملكة العربية السعودية وتختص محاكمها بنظر النزاعات.',
      'يحافظ الطرفان على سرية المعلومات التجارية المتبادلة بينهما.',
      'لا يجوز لأي طرف التنازل عن حقوقه أو التزاماته دون موافقة كتابية مسبقة.',
      'تُرسل الإشعارات الرسمية بالبريد المسجل إلى العناوين المذكورة في هذه الاتفاقية.',
      'يتم تعديل هذه الاتفاقية بموافقة كتابية موقعة من الطرفين فقط.',
      'يضمن المورد جودة المنتجات لمدة اثني عشر شهراً من تاريخ التسليم.',
      'يتحمل المشتري رسوم التخليص الجمركي في بلد الاستيراد.',
      'تعتبر القوة القاهرة عذراً مقبولاً لتأخير تنفيذ الالتزامات التعاقدية.',
      'يحق لكل طرف إنهاء الاتفاقية عند إخلال الطرف الآخر بالتزاماته الجوهرية.',
      'تحدد قائمة الأسعار الملحقة بهذه الاتفاقية سعر كل صنف من الأصناف.',
      'تُحل الخلافات وديًا بين الطرفين قبل اللجوء إلى القضاء.',
      'يلتزم المورد بتزويد المشتري بشهادات المنشأ والمطابقة لكل شحنة.',
      'تسري هذه الاتفاقية على جميع الطلبات الموقعة خلال مدة سريانها.',
    ];
    return clauses;
  }

  Future<HarborService> openLive(WidgetTester tester,
      {List<String> texts = paragraphs,
      String name = 'Acme contract.docx',
      bool rtl = false,
      List<String?>? styles,
      Set<int> spaced = const {},
      String? embedder}) async {
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
              packageId: embedder ?? 'embeddinggemma-2',
              path: embedder == null
                  ? embedGguf
                  : '$repoRoot/fixtures/models/$embedder.gguf'),
          isTrue,
          reason: 'embedding model must install');
      final docx = await buildDocx(dir,
          texts: texts, name: name, rtl: rtl, styles: styles, spaced: spaced);
      await s.loadPreviewFromBytes(await docx.readAsBytes(),
          name: name, path: docx.path);
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

  testWidgets(
      'LIVE Change this file: routed run sheet, real run, approve, Save new '
      'copy, canvas reloads onto the copy', (tester) async {
    if (!coreAvailable || !haveModels) {
      markTestSkipped('dylib or model fixtures absent');
      return;
    }
    const template = [
      'Service agreement for {{client_name}}.',
      'This agreement takes effect on {{effective_date}}.',
      'Payment terms are net 45 days from the invoice date.',
    ];
    final s = await openLive(tester, texts: template);
    final original = File(s.previewPath!);
    final originalBytes = await tester.runAsync(original.readAsBytes);
    final outDir = Directory('${original.parent.path}/out')..createSync();
    final dest = '${outDir.path}/Acme contract (Harbor).docx';
    SkillRunSheet.debugResolveSaveDestination = (_) async => dest;
    addTearDown(() => SkillRunSheet.debugResolveSaveDestination = null);

    // The skill router needs an open embedding index. The app only opens
    // its default (`bge-small-en-v1.5`); this run installed EmbeddingGemma 2
    // instead, so open that one the way Library would.
    final open = Stopwatch()..start();
    expect(
        await tester
            .runAsync(() => s.openKnowledge(packageId: 'embeddinggemma-2')),
        isTrue);
    // ignore: avoid_print
    print('live change: knowledge open in ${open.elapsed}');
    await pumpSurface(tester, s, (_) => const WorkSurface());
    await tester.tap(find.text('Change'));
    await tester.pump();
    final sw = Stopwatch()..start();
    await send(tester, 'Fill in the placeholders in this contract template');
    // Routing opens the index (cold) and ranks skills before the sheet.
    await settleUntil(tester, find.byType(SkillRunSheet),
        what: 'the routed run sheet', budget: const Duration(minutes: 20));
    // ignore: avoid_print
    print('live change: sheet open after ${sw.elapsed}');
    final sheet = tester.widget<SkillRunSheet>(find.byType(SkillRunSheet));
    // ignore: avoid_print
    print('live change: routed to ${sheet.skill.id}');
    expect(sheet.skill.id, 'placeholder-fill');
    expect(sheet.initialArtifact?.name, 'Acme contract.docx');
    expect(find.text('Attached: Acme contract.docx'), findsOneWidget);

    // `values` is an object field: the person fills it as key = value lines.
    await tester.enterText(find.byKey(const ValueKey('input-values')),
        'client_name = Harbor Ltd\neffective_date = 1 March 2027');
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('skill-run-button')));
    await tester.pump();
    await settleUntil(tester, find.byKey(const ValueKey('skill-approval')),
        what: 'the approval step', budget: const Duration(minutes: 5));
    expect(find.text('Save new copy'), findsOneWidget);

    await tester.tap(find.text('Save new copy'));
    await tester.pump();
    await settleUntil(tester, find.byKey(const ValueKey('skill-committed')),
        what: 'the commit receipt', budget: const Duration(minutes: 2));
    // The canvas reloads from the saved file once the commit returns.
    for (var i = 0;
        i < 100 && s.previewName != 'Acme contract (Harbor).docx';
        i++) {
      await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 50)));
      await tester.pump(const Duration(milliseconds: 50));
    }

    expect(await tester.runAsync(original.readAsBytes), originalBytes,
        reason: 'Save new copy must leave the original untouched');
    final copy = File(dest);
    expect(copy.existsSync(), isTrue);
    expect(s.previewName, 'Acme contract (Harbor).docx');
    expect(s.previewPath, dest);
    // Re-open the saved file from disk as a valid docx, independently of
    // the reload the app did.
    await tester.runAsync(() async =>
        s.loadPreviewFromBytes(await copy.readAsBytes(), name: 'copy.docx'));
    final doc = FileDocument.fromPreview('copy.docx', s.preview)!;
    final text = doc.passages.map((p) => p.text).join('\n');
    // ignore: avoid_print
    print('live change: saved copy text:\n$text');
    expect(text, contains('Harbor Ltd'));
    expect(text, contains('1 March 2027'));
    expect(text, isNot(contains('{{')));
    expect(text, contains('net 45 days'), reason: 'nothing else changes');
  }, timeout: const Timeout(Duration(minutes: 30)));

  testWidgets(
      'LIVE Change this file with a model skill: the real Qwen run reaches a '
      'terminal report from the routed sheet', (tester) async {
    if (!coreAvailable || !haveModels) {
      markTestSkipped('dylib or model fixtures absent');
      return;
    }
    // Two paragraphs carry direct spacing overrides (style contamination):
    // the inspector must report them, so the model node (prioritise)
    // really runs instead of ending on `clean`.
    final s = await openLive(tester, texts: const [
      'Supply agreement',
      'Acme Supplies agrees to deliver goods to Harbor Ltd.',
      'Payment',
      'Payment terms are net 45 days from the invoice date.',
    ], spaced: const {
      1,
      3
    });
    expect(
        await tester
            .runAsync(() => s.openKnowledge(packageId: 'embeddinggemma-2')),
        isTrue);
    await pumpSurface(tester, s, (_) => const WorkSurface());
    await tester.tap(find.text('Change'));
    await tester.pump();
    await send(tester, 'Review the style and formatting of this document');
    await settleUntil(tester, find.byType(SkillRunSheet),
        what: 'the routed run sheet', budget: const Duration(minutes: 20));
    final sheet = tester.widget<SkillRunSheet>(find.byType(SkillRunSheet));
    // ignore: avoid_print
    print('live change (model skill): routed to ${sheet.skill.id}');
    expect(sheet.skill.id, 'document-style-review');
    expect(sheet.skill.graph!.usesModel, isTrue);
    expect(find.text('Attached: Acme contract.docx'), findsOneWidget);
    final sw = Stopwatch()..start();
    await tester.tap(find.byKey(const ValueKey('skill-run-button')));
    await tester.pump();
    await settleUntil(tester, find.text('COMPLETED'),
        what: 'a terminal report from the Qwen run',
        budget: const Duration(minutes: 8));
    // ignore: avoid_print
    print('live change (model skill): COMPLETED in ${sw.elapsed}');
    final texts = tester
        .widgetList<Text>(find.byType(Text))
        .map((t) => t.data ?? '')
        .where((t) => t.trim().isNotEmpty)
        .join(' | ');
    // ignore: avoid_print
    print('live change (model skill): the sheet shows: $texts');
    expect(find.text('Completed'), findsWidgets,
        reason: 'the outcome label of a finished run');
    expect(find.byKey(const ValueKey('skill-approval')), findsNothing,
        reason: 'a review proposes nothing to commit');
  }, timeout: const Timeout(Duration(minutes: 30)));

  // Task: an honest probe of Arabic questions over an Arabic file. It
  // asserts only what is deterministic (the passage the core hands the
  // model) and RECORDS everything else: the model's answer is printed, not
  // judged beyond what held when it was written (see the notes inline).
  test(
      'LIVE Arabic question over an Arabic .docx: grounding per embedder, '
      'answer text recorded', () async {
    final models = '$repoRoot/fixtures/models';
    final embedders = {
      'embeddinggemma-2': embedGguf,
      'bge-m3': '$models/bge-m3-q8_0.gguf',
      'multilingual-e5-small': '$models/multilingual-e5-small-q8_0.gguf',
      'bge-small-en-v1.5': '$models/bge-small-en-v1.5-q8_0.gguf',
    };
    final llama = '$models/Llama-3.2-1B-Instruct-Q4_K_M.gguf';
    if (!coreAvailable ||
        !File(chatGguf).existsSync() ||
        embedders.values.any((p) => !File(p).existsSync())) {
      markTestSkipped('dylib or model fixtures absent');
      return;
    }
    const target = 'para-5';
    final texts = arabicContract();
    expect(texts[4], contains('٤٥'));
    final dir = await Directory.systemTemp.createTemp('harbor-work-ar-');
    final s = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-work-ar',
        deviceRootHex:
            '3a2b3c4d5e6f708192a3b4c5d6e7f8093a2b3c4d5e6f708192a3b4c5d6e7f809');
    addTearDown(s.close);
    expect(
        await s.installModelFromPath(
            packageId: 'qwen2.5-1.5b-chat', path: chatGguf),
        isTrue);
    final haveLlama = File(llama).existsSync() &&
        await s.installModelFromPath(
            packageId: 'llama-3.2-1b-chat', path: llama);
    for (final e in embedders.entries) {
      expect(
          await s.installModelFromPath(packageId: e.key, path: e.value), isTrue,
          reason: '${e.key} must install');
    }
    final docx = await buildDocx(dir,
        texts: texts, name: 'اتفاقية توريد.docx', rtl: true);
    await s.loadPreviewFromBytes(await docx.readAsBytes(),
        name: 'اتفاقية توريد.docx', path: docx.path);
    expect(s.preview, isNotNull, reason: s.previewError);
    final doc = FileDocument.fromPreview(s.previewName, s.preview)!;
    expect(doc.passages.length, texts.length);
    expect(doc.passages[4].text, texts[4], reason: 'Arabic text round-trips');

    const supported = 'ما هي شروط الدفع؟';
    const paraphrase = 'خلال كم يوماً يجب سداد الفاتورة؟';
    const unsupported = 'من هو المدير التنفيذي لشركة هاربر؟';

    Future<Map<String, dynamic>> ask(String q, String chat) async {
      final runId = HarborService.newRunId();
      await s.createRun(runId);
      final a = await s.askAboutFile(q,
          chatPackage: chat, runId: runId, maxTokens: 96, document: doc);
      // A multi-byte character split across tokens used to come back as
      // U+FFFD ("��" for an Arabic-Indic number); fixed in the decoder.
      expect(a['answer'], isNot(contains('\u{FFFD}')),
          reason: 'garbled answer for "$q" on $chat: ${a['answer']}');
      return a;
    }

    String describe(Map<String, dynamic> a) {
      final cites = (a['citations'] as List).cast<Map>();
      final ranked = [...cites]
        ..sort((x, y) => (y['score'] as num).compareTo(x['score'] as num));
      final top = ranked.first;
      final at =
          ranked.indexWhere((c) => c['passage_id'] == target) + 1; // 0 = absent
      return 'ranking=${(a['scope'] as Map)['ranking']} '
          'selected=${cites.length} top=${top['passage_id']} '
          'target_rank=${at == 0 ? 'ABSENT' : at}';
    }

    // Lexical baseline: no index open yet.
    final base = await ask(supported, 'qwen2.5-1.5b-chat');
    // ignore: avoid_print
    print(
        'AR lexical (no index): ${describe(base)}\n  answer: ${base['answer']}');
    expect((base['scope'] as Map)['ranking'], 'lexical');

    final results = <String, String>{};
    for (final id in embedders.keys) {
      expect(await s.openKnowledge(packageId: id), isTrue, reason: id);
      for (final q in [supported, paraphrase]) {
        final a = await ask(q, 'qwen2.5-1.5b-chat');
        final line = '${describe(a)}\n  q: $q\n  answer: ${a['answer']}';
        results['$id | $q'] = line;
        // ignore: avoid_print
        print('AR $id: $line');
        final cites = (a['citations'] as List).cast<Map>();
        if (id == 'embeddinggemma-2' || id == 'bge-m3') {
          // The retrieval step is deterministic; a multilingual embedder
          // must hand the model the payment paragraph.
          expect(cites.map((c) => c['passage_id']), contains(target),
              reason: '$id lost the payment paragraph for "$q"');
        }
      }
    }

    // Abstention and the other chat model, on the best embedder.
    expect(await s.openKnowledge(packageId: 'embeddinggemma-2'), isTrue);
    final none = await ask(unsupported, 'qwen2.5-1.5b-chat');
    // ignore: avoid_print
    print('AR unsupported (qwen): ${describe(none)}\n'
        '  answer: ${none['answer']}');
    if (haveLlama) {
      for (final q in [supported, unsupported]) {
        final a = await ask(q, 'llama-3.2-1b-chat');
        // ignore: avoid_print
        print(
            'AR llama-3.2-1b: ${describe(a)}\n  q: $q\n  answer: ${a['answer']}');
      }
    }
  }, timeout: const Timeout(Duration(minutes: 40)));

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
