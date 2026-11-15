import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/widgets/answer.dart';
import 'package:harbor_app/widgets/save_pdf.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// A CV or letter drafted in chat can be saved as a PDF from its answer.
void main() {
  test('a document title becomes a safe file name', () {
    expect(pdfFileStem('Mohsin Ali — Systems Analyst'),
        'Mohsin Ali — Systems Analyst');
    expect(pdfFileStem('Q3: results/plan?'), 'Q3 results plan');
    expect(pdfFileStem('   '), 'Harbor answer');
    expect(pdfFileStem(null), 'Harbor answer');
    expect(pdfFileStem('x' * 90).length, 60);
  });

  group('composeAnswerDocument', () {
    test('the question is the title; broken lines rejoin into paragraphs', () {
      // Exactly how a 1B model answered: a hard break mid-sentence.
      const answer =
          'A bill becomes a law through a process. The bill is then\n'
          'referred to a committee for review.\n\n'
          'If the President signs the bill,\n'
          'it becomes a law.';
      expect(
          composeAnswerDocument('How does a bill become a law?', answer),
          '# How does a bill become a law?\n\n'
          'A bill becomes a law through a process. The bill is then referred '
          'to a committee for review.\n\n'
          'If the President signs the bill, it becomes a law.');
    });

    test('lists stay line by line, with a gap before and after', () {
      expect(
          composeAnswerDocument('Why?',
              'Reasons:\n- rivers carry salt\n- rain erodes rock\nDone.'),
          '# Why?\n\nReasons:\n\n- rivers carry salt\n- rain erodes rock\n\nDone.');
    });

    test('numbered items stay separate lines', () {
      expect(composeAnswerDocument(null, '1. one\n2. two'), '1. one\n2. two');
    });

    test('an answer with its own heading is not given a second title', () {
      final out = composeAnswerDocument('Q?', '# Own title\n\nBody text.');
      expect(out, '# Own title\n\nBody text.');
    });

    test('no question: no title is invented; code blocks are untouched', () {
      expect(composeAnswerDocument('  ', 'Text\n```\na\n  b\n```'),
          'Text\n\n```\na\n  b\n```');
    });

    test('a very long question is shortened in the title', () {
      final out = composeAnswerDocument('word ' * 60, 'Body.');
      expect(out.split('\n').first.length, lessThanOrEqualTo(122));
    });
  });

  Future<void> show(WidgetTester tester, String text) =>
      tester.pumpWidget(HarborTheme(
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
            body: SingleChildScrollView(
              child: GroundedAnswerView(answer: {
                'answer': text,
                'used_citations': false,
                'executed_on': 'lfm3',
                'citations': const [],
                'usage': const {'completion_tokens': 40},
              }),
            ),
          ),
        ),
      ));

  testWidgets('an answer offers Save as PDF', (tester) async {
    await show(tester, '# Mohsin Ali\nSystems Analyst');
    expect(find.byKey(const ValueKey('answer-save-pdf')), findsOneWidget);
    expect(find.byTooltip('Save as PDF'), findsOneWidget);
  });

  testWidgets('an abstention has nothing to save', (tester) async {
    await show(tester, 'INSUFFICIENT_EVIDENCE');
    expect(find.byKey(const ValueKey('answer-save-pdf')), findsNothing);
  });
}
