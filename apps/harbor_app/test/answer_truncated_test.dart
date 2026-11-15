import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/widgets/answer.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// An answer that hit the token limit says so instead of ending
/// mid-sentence as if it were complete.
void main() {
  Future<void> show(WidgetTester tester, Map<String, dynamic> answer) =>
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
              child: GroundedAnswerView(answer: answer),
            ),
          ),
        ),
      ));

  Map<String, dynamic> answer({required bool truncated}) => {
        'answer': 'Elias climbed the stairs and',
        'truncated': truncated,
        'used_citations': false,
        'executed_on': 'lfm3',
        'execution': 'ON_DEVICE',
        'citations': const [],
        'usage': {'completion_tokens': 1024},
      };

  testWidgets('a cut-off answer is marked as cut off', (tester) async {
    await show(tester, answer(truncated: true));
    expect(find.byKey(const ValueKey('answer-truncated')), findsOneWidget);
    expect(find.textContaining('cut off'), findsOneWidget);
  });

  testWidgets('a complete answer carries no notice', (tester) async {
    await show(tester, answer(truncated: false));
    expect(find.byKey(const ValueKey('answer-truncated')), findsNothing);
  });
}
