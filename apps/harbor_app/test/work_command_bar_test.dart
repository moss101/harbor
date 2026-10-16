import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/surfaces/home_surface.dart';
import 'package:harbor_app/surfaces/skill_run.dart';
import 'package:harbor_app/surfaces/work_surface.dart';

import 'fake_service.dart';

/// The Work command bar and the Home composer route requests (fake core):
/// ask → grounded answer from the file's passages or Knowledge; change /
/// task → the skill's run sheet, file pre-attached, nothing run.
void main() {
  FakeHarborService withFile({
    List<Map<String, dynamic>> models = const [
      {'id': 'qwen-chat'}
    ],
  }) {
    final s =
        FakeHarborService(models: models, skillList: [tidySkill, emailSkill])
          ..fakePreview = docxPreview()
          ..fakePreviewName = 'report.docx'
          ..fakeBytes = [1, 2, 3]
          ..fakePath = '/docs/report.docx';
    return s;
  }

  Future<void> send(WidgetTester tester, String text) async {
    await tester.enterText(find.byKey(const ValueKey('composer-field')), text);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('composer-send')));
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
  }

  testWidgets('the bar appears only while a file is open', (tester) async {
    final s = FakeHarborService(models: const [
      {'id': 'qwen-chat'}
    ]);
    await pumpSurface(tester, s, (_) => const WorkSurface());
    expect(find.byKey(const ValueKey('work-command-bar')), findsNothing);

    s
      ..fakePreview = docxPreview()
      ..fakePreviewName = 'report.docx'
      ..fakeBytes = [1];
    s.notify();
    await tester.pump();
    expect(find.byKey(const ValueKey('work-command-bar')), findsOneWidget);
    expect(find.text('This file · report.docx'), findsOneWidget);
    // Work still edits nothing by itself.
    expect(find.text('Read-only preview'), findsOneWidget);

    s.clearPreview();
    await tester.pump();
    expect(find.byKey(const ValueKey('work-command-bar')), findsNothing);
  });

  testWidgets(
      'Ask sends the open file passages and a cited paragraph is shown '
      'when tapped', (tester) async {
    final s = withFile()
      ..answer = {
        'answer': 'Revenue grew 12%.',
        'used_citations': true,
        'execution': 'ON_DEVICE',
        'executed_on': 'cpu',
        'citations': [
          {
            'passage_id': 'para-2',
            'locator': '¶2',
            'text': 'Revenue grew 12% over the quarter.'
          },
        ],
      };
    await pumpSurface(tester, s, (_) => const WorkSurface());
    await tester.tap(find.text('Ask'));
    await tester.pump();
    await send(tester, 'How did revenue do');

    expect(s.calls.contains('suggestIntent'), isFalse,
        reason: 'a picked mode is not second-guessed by the router');
    expect(s.askAboutFileArgs, hasLength(1));
    final args = s.askAboutFileArgs.single;
    expect(args['question'], 'How did revenue do');
    expect(args['chat_package'], 'qwen-chat');
    final doc = args['document'] as Map;
    expect(doc['name'], 'report.docx');
    expect(doc['passages'], [
      {'id': 'para-0', 'text': 'Quarterly report', 'locator': '¶0'},
      {
        'id': 'para-2',
        'text': 'Revenue grew 12% over the quarter.',
        'locator': '¶2'
      },
    ]);
    expect(find.text('Revenue grew 12%.'), findsOneWidget);
    expect(find.text('Paragraph 2'), findsOneWidget);

    expect(find.byKey(const ValueKey('work-marked-paragraph')), findsNothing);
    await tester.tap(find.text('Paragraph 2'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 600));
    expect(find.byKey(const ValueKey('work-marked-paragraph')), findsOneWidget);
  });

  testWidgets('a question is routed to Ask without a toggle', (tester) async {
    final s = withFile()..indexOpens = false;
    await pumpSurface(tester, s, (_) => const WorkSurface());
    await send(tester, 'What is the total?');
    expect(s.calls, contains('askAboutFile'));
  });

  testWidgets('without a chat model the bar says how to get one',
      (tester) async {
    final s = withFile(models: const [])..indexOpens = false;
    await pumpSurface(tester, s, (_) => const WorkSurface());
    await send(tester, 'What is the total?');
    expect(find.byKey(const ValueKey('work-needs-model')), findsOneWidget);
    expect(s.calls.contains('askAboutFile'), isFalse);
  });

  testWidgets(
      'Change opens the routed skill with the open file attached and the '
      'request prefilled, and runs nothing', (tester) async {
    final s = withFile()
      ..suggestion = {
        'abstained': false,
        'ranked': [
          {'skill_id': 'table-cleanup', 'title': 'Table Cleanup', 'score': 0.9},
        ],
      };
    await pumpSurface(tester, s, (_) => const WorkSurface());
    await tester.tap(find.text('Change'));
    await tester.pump();
    await send(tester, 'tidy the totals table');
    await tester.pumpAndSettle();

    expect(find.byType(SkillRunSheet), findsOneWidget);
    final sheet = tester.widget<SkillRunSheet>(find.byType(SkillRunSheet));
    expect(sheet.skill.id, 'table-cleanup');
    expect(sheet.initialArtifact?.name, 'report.docx');
    expect(sheet.initialArtifact?.bytes, [1, 2, 3]);
    expect(sheet.initialArtifact?.path, '/docs/report.docx');
    expect(find.text('Attached: report.docx'), findsOneWidget);
    final instruction = tester
        .widget<TextField>(find.byKey(const ValueKey('input-instruction')));
    expect(instruction.controller!.text, 'tidy the totals table');
    expect(s.calls, isNot(contains('startSkillRun')),
        reason: 'routing opens the sheet; only Run there starts a run');
  });

  testWidgets('an approved change reopens the canvas on the saved result',
      (tester) async {
    final dir = await tester
        .runAsync(() => Directory.systemTemp.createTemp('harbor-work-bar-'));
    final dest = '${dir!.path}${Platform.pathSeparator}report (Harbor).docx';
    await tester.runAsync(() => File(dest).writeAsBytes([9, 9]));
    addTearDown(() => dir.deleteSync(recursive: true));
    SkillRunSheet.debugResolveSaveDestination = (_) async => dest;
    addTearDown(() => SkillRunSheet.debugResolveSaveDestination = null);
    final s = withFile()
      ..suggestion = {
        'abstained': false,
        'ranked': [
          {'skill_id': 'table-cleanup', 'title': 'Table Cleanup', 'score': 0.9},
        ],
      }
      ..startReport = {
        'run_id': 'run-fake',
        'state': 'WAITING_APPROVAL',
        'status': {
          'status': 'waiting_approval',
          'approval': {
            'effect_class': 'artifact.commit',
            'proposed_output_hash': 'ab' * 32,
            'artifact_id': 'a1',
          },
        },
        'trail': [],
      };
    await pumpSurface(tester, s, (_) => const WorkSurface());
    await tester.tap(find.text('Change'));
    await tester.pump();
    await send(tester, 'tidy the totals table');
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('skill-run-button')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save new copy'));
    for (var i = 0; i < 20 && !s.calls.contains('loadPreviewFromBytes'); i++) {
      await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 20)));
      await tester.pump(const Duration(milliseconds: 20));
    }
    expect(s.calls, containsAllInOrder(['startSkillRun', 'commitProposal']));
    expect(s.fakeBytes, [9, 9], reason: 'the canvas shows the saved result');
    expect(s.fakePreviewName, 'report (Harbor).docx');
    expect(s.fakePath, dest);
  });

  testWidgets('an abstaining router offers choices and the Skills library',
      (tester) async {
    final s = withFile()
      ..suggestion = {
        'abstained': true,
        'reason': 'ambiguous',
        'ranked': [
          {'skill_id': 'table-cleanup', 'title': 'Table Cleanup', 'score': 0.5},
        ],
      };
    final state = await pumpSurface(tester, s, (_) => const WorkSurface());
    await tester.tap(find.text('Change'));
    await tester.pump();
    await send(tester, 'sort it out');
    expect(find.byKey(const ValueKey('route-panel')), findsOneWidget);
    expect(find.byKey(const ValueKey('route-skill-table-cleanup')),
        findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('route-browse-skills')));
    await tester.pump();
    expect(state.surface, HarborSurface.skills);
  });

  testWidgets('the bar fits a phone and mirrors in Arabic', (tester) async {
    final s = withFile();
    await pumpSurface(tester, s, (_) => const WorkSurface(),
        size: const Size(390, 844), locale: const Locale('ar'));
    expect(find.byKey(const ValueKey('work-command-bar')), findsOneWidget);
    expect(find.text('هذا الملف · report.docx'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  group('Home composer', () {
    testWidgets('a question is answered inline, grounded in Knowledge',
        (tester) async {
      final s = FakeHarborService(models: const [
        {'id': 'qwen-chat'}
      ], skillList: [
        emailSkill
      ], indexOpens: false);
      await pumpSurface(tester, s, (state) => HomeSurface(state: state));
      await send(tester, 'What did the board decide?');
      expect(s.generateArgs.single['question'], 'What did the board decide?');
      expect(find.byKey(const ValueKey('home-answer')), findsOneWidget);
      expect(find.text('Revenue grew 12% [p1].'), findsOneWidget);
    });

    testWidgets('a task opens the routed skill sheet, prefilled, unrun',
        (tester) async {
      final s = FakeHarborService(models: const [
        {'id': 'qwen-chat'}
      ], skillList: [
        emailSkill,
        tidySkill
      ], intent: {
        'abstained': false,
        'ranked': [
          {'id': 'run_skill', 'score': 0.9}
        ]
      }, suggestion: {
        'abstained': false,
        'ranked': [
          {'skill_id': 'email-drafting', 'title': 'Email', 'score': 0.9},
        ],
      });
      await pumpSurface(tester, s, (state) => HomeSurface(state: state));
      // Quick actions prefill; sending routes like anything typed.
      await send(tester, 'Draft an email to Sam about Friday');
      await tester.pumpAndSettle();
      final sheet = tester.widget<SkillRunSheet>(find.byType(SkillRunSheet));
      expect(sheet.skill.id, 'email-drafting');
      expect(sheet.initialArtifact, isNull);
      final notes =
          tester.widget<TextField>(find.byKey(const ValueKey('input-notes')));
      expect(notes.controller!.text, 'Draft an email to Sam about Friday');
      expect(s.calls.contains('generateAnswer'), isFalse);
    });

    testWidgets('a quick action routes on send, never just records',
        (tester) async {
      final s = FakeHarborService(models: const [
        {'id': 'qwen-chat'}
      ], skillList: [
        emailSkill
      ], indexOpens: false);
      await pumpSurface(tester, s, (state) => HomeSurface(state: state));
      await tester.tap(find.text('Summarize document'));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('composer-send')));
      for (var i = 0; i < 5; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(s.calls, contains('suggestSkills'));
      expect(find.byKey(const ValueKey('route-panel')), findsOneWidget);
      expect(find.text('Request recorded as a durable run'), findsNothing);
    });
  });
}
