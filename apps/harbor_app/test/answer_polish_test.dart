import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/widgets/answer.dart';
import 'package:harbor_app/widgets/skill_output.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// The reading polish found on the iOS simulator: light Markdown in small
/// model answers, English answers inside the Arabic UI, skill outputs
/// shown as text instead of JSON, and an answer-shaped loading state.
void main() {
  Future<void> show(WidgetTester tester, Widget child,
          {Locale locale = const Locale('en')}) =>
      tester.pumpWidget(HarborTheme(
        colors: HarborColors.light,
        text: HarborType(arabic: locale.languageCode == 'ar'),
        child: MaterialApp(
          locale: locale,
          localizationsDelegates: const [
            AppLocalizations.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          supportedLocales: const [Locale('en'), Locale('ar')],
          home: Scaffold(body: SingleChildScrollView(child: child)),
        ),
      ));

  String plain(WidgetTester tester) {
    final w = tester
        .widget<SelectableText>(find.byKey(const ValueKey('answer-text')));
    return w.data ?? w.textSpan!.toPlainText();
  }

  testWidgets('Markdown marks become bullets and bold, not stray symbols',
      (tester) async {
    await show(
        tester,
        const AnswerText('## Why\n\nIt is **salty** because:\n'
            '- rivers carry `minerals`\n* rain erodes rock\n1. one'));
    final text = plain(tester);
    expect(text, contains('•  rivers carry minerals'));
    expect(text, contains('•  rain erodes rock'));
    expect(text, contains('1. one'));
    expect(text, isNot(contains('**')));
    expect(text, isNot(contains('`')));
    expect(text, isNot(contains('##')));
  });

  testWidgets('plain prose stays a plain selectable text', (tester) async {
    await show(tester, const AnswerText('The sky is blue.'));
    final w = tester
        .widget<SelectableText>(find.byKey(const ValueKey('answer-text')));
    expect(w.data, 'The sky is blue.');
  });

  testWidgets('an English answer reads LTR inside the Arabic UI',
      (tester) async {
    await show(tester, const AnswerText('The sky is blue.'),
        locale: const Locale('ar'));
    final w = tester
        .widget<SelectableText>(find.byKey(const ValueKey('answer-text')));
    expect(w.textDirection, TextDirection.ltr);
    expect(contentDirection('تنص شروط الدفع', TextDirection.ltr),
        TextDirection.rtl);
    expect(contentDirection('123 ؟', TextDirection.rtl), TextDirection.rtl,
        reason: 'digits and punctuation are not strong: the UI decides');
  });

  testWidgets('generating: a status line with cancel over skeleton lines',
      (tester) async {
    var cancelled = false;
    await show(
        tester,
        GeneratingCard(
            progress: const {'items_done': 7},
            onCancel: () => cancelled = true));
    expect(find.byKey(const ValueKey('answer-generating')), findsOneWidget);
    expect(find.byType(HarborSkeletonBlock), findsNWidgets(4));
    expect(find.textContaining('7 tokens generated', findRichText: true),
        findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsNothing);
    await tester.tap(find.byKey(const ValueKey('answer-cancel')));
    expect(cancelled, isTrue);
  });

  testWidgets('skill outputs read as sections, not JSON', (tester) async {
    await show(
        tester,
        const SkillOutputView(outputs: {
          'update': {
            'header': 'Team – This week',
            'plans': ['The launch is moving to Friday'],
            'problems': <String>[],
          },
        }));
    expect(find.text('Team – This week'), findsOneWidget);
    expect(find.text('Plans'), findsOneWidget);
    expect(find.text('The launch is moving to Friday'), findsOneWidget);
    expect(find.textContaining('"plans"'), findsNothing);
    expect(SkillOutputView.humanize('rendered.subject'), 'Subject');
    expect(SkillOutputView.humanize('next_steps'), 'Next steps');
  });

  testWidgets('a text output keeps its Copy action', (tester) async {
    await show(tester,
        const SkillOutputView(outputs: {'rendered.text': 'Hello Sam,'}));
    expect(find.byKey(const ValueKey('copy-rendered.text')), findsOneWidget);
    expect(find.text('Text'), findsOneWidget);
  });

  testWidgets('nested outputs keep the exact JSON', (tester) async {
    await show(
        tester,
        const SkillOutputView(outputs: {
          'rows': [
            {'a': 1}
          ]
        }));
    expect(find.textContaining('"a": 1'), findsOneWidget);
  });
}
