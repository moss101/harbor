import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/file_passages.dart';
import 'package:harbor_app/services/request_router.dart';

import 'fake_service.dart';

/// The request router (recommend-only): ask vs task, which skill, and the
/// honest fallbacks when the core cannot rank — against a fake core.
void main() {
  Map<String, dynamic> ranked(List<String> ids, {bool abstained = false}) => {
        'abstained': abstained,
        if (abstained) 'reason': 'ambiguous',
        'ranked': [
          for (final id in ids) {'skill_id': id, 'title': id, 'score': 0.8},
        ],
      };

  test('a mode the person picked wins, without consulting the core', () async {
    final s = FakeHarborService(skillList: [tidySkill]);
    final r =
        await RequestRouter(s).route('make it tidy', mode: RequestMode.ask);
    expect(r, isA<RouteAsk>());
    expect(s.calls, isEmpty);
  });

  test("the core's decision layer sends questions to Ask", () async {
    final s = FakeHarborService(skillList: [
      tidySkill
    ], intent: {
      'abstained': false,
      'ranked': [
        {'id': 'search_documents', 'score': 0.9}
      ]
    });
    final r = await RequestRouter(s).route('revenue last quarter');
    expect(r, isA<RouteAsk>());
    expect((r as RouteAsk).fromCore, isTrue);
    expect(s.calls, ['openKnowledge', 'suggestIntent']);
  });

  test('run_skill goes to the skill router even when phrased as a question',
      () async {
    final s = FakeHarborService(skillList: [
      tidySkill,
      emailSkill
    ], intent: {
      'abstained': false,
      'ranked': [
        {'id': 'run_skill', 'score': 0.9}
      ]
    }, suggestion: ranked(['email-drafting', 'table-cleanup']));
    final r = await RequestRouter(s)
        .route('can you draft an email to Sam?', chatPackage: 'qwen');
    expect(r, isA<RouteSkill>());
    expect((r as RouteSkill).skill.id, 'email-drafting');
    // Without a file, a skill that REQUIRES one is not offered.
    expect(r.alternatives, isEmpty);
  });

  test('with a file open only file-taking skills qualify', () async {
    final s = FakeHarborService(
        skillList: [tidySkill, emailSkill],
        suggestion: ranked(['email-drafting', 'table-cleanup']));
    final r = await RequestRouter(s).route('tidy the totals table',
        file: const RouteFile(name: 'budget.xlsx'), mode: RequestMode.change);
    expect(r, isA<RouteSkill>());
    expect((r as RouteSkill).skill.id, 'table-cleanup');
    // Change mode skips the intent step entirely.
    expect(s.calls.contains('suggestIntent'), isFalse);
  });

  test('an abstaining skill router is shown, never resolved by guessing',
      () async {
    final s = FakeHarborService(
        skillList: [tidySkill, emailSkill],
        suggestion: ranked(['email-drafting'], abstained: true));
    final r = await RequestRouter(s).route('do the thing');
    expect(r, isA<RouteNone>());
    expect((r as RouteNone).reason, RouteNoneReason.ambiguous);
    expect(r.suggestions.map((s) => s.id), ['email-drafting']);
  });

  test('a ranked skill that cannot run here is not recommended', () async {
    final s = FakeHarborService(
        skillList: [tidySkill], suggestion: ranked(['table-cleanup']));
    final r = await RequestRouter(s).route('clean up this table');
    expect(r, isA<RouteNone>());
    expect((r as RouteNone).reason, RouteNoneReason.noRunnableSkill);
  });

  group('without an index', () {
    test('a question mark or interrogative opener means Ask', () async {
      for (final q in [
        'What did the board decide?',
        'how much did we spend',
        'is the contract signed',
        'ما هي الإيرادات؟',
        'هل وقع العقد',
        'totals for march?',
      ]) {
        final s = FakeHarborService(indexOpens: false, skillList: [emailSkill]);
        final r = await RequestRouter(s).route(q);
        expect(r, isA<RouteAsk>(), reason: q);
        expect((r as RouteAsk).fromCore, isFalse);
      }
    });

    test('anything else is a task, offered as choices, never picked', () async {
      final s = FakeHarborService(
          indexOpens: false, skillList: [emailSkill, tidySkill]);
      final r = await RequestRouter(s).route('draft an email to the team');
      expect(r, isA<RouteNone>());
      expect((r as RouteNone).reason, RouteNoneReason.noIndex);
      expect(r.suggestions.map((s) => s.id), ['email-drafting']);
    });

    test('openers that usually ask for an action are not questions', () {
      expect(RequestRouter.looksLikeQuestion('Can you shorten the intro'),
          isFalse);
      expect(RequestRouter.looksLikeQuestion('Summarize document'), isFalse);
      expect(RequestRouter.looksLikeQuestion('Whatever works'), isFalse);
      expect(RequestRouter.looksLikeQuestion('  '), isFalse);
    });
  });

  test('the chat model is the preferred one when installed', () {
    final s = FakeHarborService(models: [
      {'id': 'a'},
      {'id': 'b'},
    ]);
    expect(chatPackageFor(s, 'b'), 'b');
    expect(chatPackageFor(s, 'gone'), 'a');
    expect(chatPackageFor(FakeHarborService(), 'b'), isNull);
  });

  group('file passages', () {
    test('docx: one passage per non-empty paragraph, ¶ locators', () {
      final doc = FileDocument.fromPreview('r.docx', docxPreview())!;
      expect(doc.name, 'r.docx');
      expect([for (final p in doc.passages) p.id], ['para-0', 'para-2']);
      expect(doc.passages.last.locator, '¶2');
    });

    test('pdf pages split when long; deck slides; workbook cells in A1', () {
      final long = 'x' * (FileDocument.maxPassageChars + 10);
      final pdf = FileDocument.fromPreview('a.pdf', {
        'kind': 'pdf',
        'preview': {
          'pages': [
            {'index': 1, 'text': 'Intro'},
            {'index': 2, 'text': long},
          ]
        }
      })!;
      expect([for (final p in pdf.passages) p.id],
          ['page-1', 'page-2', 'page-2~2']);
      expect(PassageTarget.parse('page-2~2')!.index, 2);

      final deck = FileDocument.fromPreview('d.pptx', {
        'kind': 'deck',
        'preview': {
          'slides': [
            {
              'index': 1,
              'title': 'Plan',
              'bullets': ['Ship', 'Learn']
            },
          ]
        }
      })!;
      expect(deck.passages.single.text, 'Plan\nShip\nLearn');
      expect(deck.passages.single.locator, 'slide 1');

      final book = FileDocument.fromPreview('b.xlsx', {
        'kind': 'workbook',
        'preview': {
          'sheet': 'Budget',
          'cells': [
            {'row': 2, 'col': 28, 'value': '4250', 'formula': 'SUM(A1:A3)'},
            {'row': 1, 'col': 1, 'value': 'Total'},
            {'row': 1, 'col': 2, 'value': ''},
          ]
        }
      })!;
      expect([for (final p in book.passages) p.id], ['cell-A1', 'cell-AB2']);
      expect(book.passages.last.locator, 'Budget!AB2');
      expect(book.passages.last.text, '4250 (=SUM(A1:A3))');
      expect(FileDocument.parseA1('Budget!AB2'), (2, 28));
      expect(PassageTarget.parse('cell-AB2')!.cell, 'AB2');
      expect(PassageTarget.parse('chunk-9'), isNull);
    });
  });
}
