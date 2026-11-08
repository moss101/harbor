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
