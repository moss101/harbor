import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/surfaces/home_surface.dart';
import 'package:harbor_app/surfaces/skill_run.dart';

import 'fake_service.dart';

/// Home is one conversation with Ask merged in (scripted core): questions
/// become inline answer turns (durable run, cancel, model picker, the
/// no-model Library fallback, Library-only search); tasks open the routed
/// skill's run sheet and stay as a card linking to the run; the dashboard
/// is the empty state and comes back when the conversation is cleared.
void main() {
  const List<Map<String, dynamic>> chat = [
    {'id': 'qwen-chat'},
    {'id': 'llama-chat'},
  ];

  Future<void> send(WidgetTester tester, String text) async {
    await tester.enterText(find.byKey(const ValueKey('composer-field')), text);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('composer-send')));
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
  }

  testWidgets(
      'the dashboard is the empty conversation; a question becomes an '
      'inline turn on a durable run; clearing returns to the dashboard',
      (tester) async {
    final s = FakeHarborService(models: chat, indexOpens: false);
    await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    expect(find.text('What do you want to get done?'), findsOneWidget);
    expect(find.text('Summarize document'), findsOneWidget);
    expect(find.byKey(const ValueKey('home-conversation')), findsNothing);

    await send(tester, 'What did the board decide?');
    expect(find.byKey(const ValueKey('home-conversation')), findsOneWidget);
    // The dashboard gives way to the turns; the question and its grounded
    // answer read as one turn.
    expect(find.text('What do you want to get done?'), findsNothing);
    expect(find.text('What did the board decide?'), findsOneWidget);
    expect(find.text('Revenue grew 12% [p1].'), findsOneWidget);
    expect(find.text('GROUNDED'), findsOneWidget);
    // Durable: a run is created first and generation is bound to it.
    expect(s.calls, containsAllInOrder(['createRun', 'generateAnswer']));
    expect(s.generateRunIds.single, startsWith('run-'));

    // A second question appends a turn; the composer stays docked.
    await send(tester, 'Who attended?');
    expect(find.byKey(const ValueKey('home-answer')), findsNWidgets(2));

    await tester.tap(find.byKey(const ValueKey('home-clear-conversation')));
    await tester.pump();
    expect(find.text('What do you want to get done?'), findsOneWidget);
    expect(find.byKey(const ValueKey('home-answer')), findsNothing);
  });

  testWidgets('the model picker chooses the chat model answers run with',
      (tester) async {
    final s = FakeHarborService(models: chat, indexOpens: false);
    final state =
        await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    expect(find.byKey(const ValueKey('home-model-picker')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('home-model-picker')));
    await tester.pump();
    await tester.tap(find.text('llama-chat').last);
    await tester.pump();
    expect(state.chatModel, 'llama-chat');
    await send(tester, 'What did the board decide?');
    expect(s.generateArgs.single['chat_package'], 'llama-chat');
  });

  testWidgets('a generating answer cancels from the composer', (tester) async {
    final s = FakeHarborService(models: chat, indexOpens: false)
      ..generateGate = Completer<void>();
    await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    await send(tester, 'What did the board decide?');
    // While it generates: live progress in the turn, stop in the composer.
    expect(find.text('Generating on-device…'), findsOneWidget);
    expect(find.text('3 tokens generated'), findsOneWidget);
    expect(find.byKey(const ValueKey('composer-cancel')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('composer-cancel')));
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(s.calls, contains('cancelOp:op-generate'));
    expect(find.text('Generation cancelled'), findsOneWidget);
    expect(find.byKey(const ValueKey('composer-cancel')), findsNothing);
  });

  testWidgets(
      'without a chat model: Library evidence when it is open, install '
      'guidance when it is not', (tester) async {
    final s = FakeHarborService(indexOpens: true)
      ..evidence = {
        'citations': [
          {'title': 'Board minutes', 'score': 0.8, 'state': 'indexed'},
        ],
      };
    await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    await send(tester, 'What did the board decide?');
    // Retrieval-only, honestly labelled, no generation.
    expect(find.text('Evidence'), findsOneWidget);
    expect(find.text('Board minutes'), findsOneWidget);
    expect(
        find.text('Retrieval only — no chat model was used.'), findsOneWidget);
    expect(s.calls.contains('generateAnswer'), isFalse);

    final closed = FakeHarborService(indexOpens: false);
    final state =
        await pumpSurface(tester, closed, (state) => HomeSurface(state: state));
    await send(tester, 'What did the board decide?');
    expect(find.byKey(const ValueKey('home-needs-model')), findsOneWidget);
    await tester.tap(find.text('Models'));
    await tester.pump();
    expect(state.surface, HarborSurface.models);
    expect(state.pendingModelsTab, ModelsTab.recommended);
  });

  testWidgets('Library-only search answers with citations, never generates',
      (tester) async {
    final s = FakeHarborService(models: chat, indexOpens: true)
      ..evidence = const {'citations': []};
    await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    final search = find.byKey(const ValueKey('home-search-only'));
    expect(tester.widget<IconButton>(search).onPressed, isNull,
        reason: 'nothing to search yet');
    await tester.enterText(
        find.byKey(const ValueKey('composer-field')), 'board minutes');
    await tester.pump();
    await tester.tap(search);
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(s.calls, contains('searchKnowledge'));
    expect(s.calls.contains('generateAnswer'), isFalse);
    // An empty result abstains instead of guessing.
    expect(find.text('No supporting evidence'), findsOneWidget);
  });

  testWidgets(
      'a task opens the run sheet and stays as a card linking to its run',
      (tester) async {
    final s = FakeHarborService(models: chat, skillList: [
      emailSkill
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
    final state =
        await pumpSurface(tester, s, (state) => HomeSurface(state: state));
    await send(tester, 'Draft an email to Sam about Friday');
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(SkillRunSheet), findsOneWidget);
    expect(s.calls.contains('startSkillRun'), isFalse,
        reason: 'routing recommends; only Run starts a run');
    await tester.tap(find.byKey(const ValueKey('skill-run-button')));
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(s.calls, contains('startSkillRun'));
    Navigator.of(tester.element(find.byType(SkillRunSheet))).pop();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pump();
    expect(find.byType(SkillRunSheet), findsNothing);

    // The task is a turn: the request, which skill, and the run.
    expect(find.text('Draft an email to Sam about Friday'), findsOneWidget);
    expect(find.byKey(const ValueKey('home-task')), findsOneWidget);
    expect(find.textContaining('Opened Email Drafting'), findsOneWidget);
    expect(find.text('run-fake'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('home-task-view-run')));
    await tester.pump();
    expect(state.surface, HarborSurface.activity);

    // The sheet can be reopened from the card.
    await tester.tap(find.byKey(const ValueKey('home-task-reopen')));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(SkillRunSheet), findsOneWidget);
  });

  testWidgets('the conversation fits a phone and mirrors in Arabic',
      (tester) async {
    final s = FakeHarborService(models: chat, indexOpens: false);
    await pumpSurface(tester, s, (state) => HomeSurface(state: state),
        size: const Size(390, 844), locale: const Locale('ar'));
    await send(tester, 'ما الذي قرره المجلس؟');
    expect(find.byKey(const ValueKey('home-conversation')), findsOneWidget);
    expect(find.text('مسح المحادثة'), findsOneWidget);
    expect(
        Directionality.of(
            tester.element(find.byKey(const ValueKey('home-conversation')))),
        TextDirection.rtl);
    expect(tester.takeException(), isNull);
  });
}
