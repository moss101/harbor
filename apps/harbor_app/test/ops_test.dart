import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/widgets/ops.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// Pump [child] with the app's localizations and theme, and hand the
/// caller the AppLocalizations the widgets themselves see.
Future<AppLocalizations> pumpWithL10n(
    WidgetTester tester, Widget Function(BuildContext) child) async {
  late AppLocalizations l10n;
  await tester.pumpWidget(MaterialApp(
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: const [Locale('en'), Locale('ar')],
    theme: harborThemeData(dark: false, arabic: false),
    builder: (_, inner) => HarborTheme(
      colors: HarborColors.light,
      text: const HarborType(arabic: false),
      child: inner!,
    ),
    home: Scaffold(body: Builder(builder: (context) {
      l10n = AppLocalizations.of(context)!;
      return child(context);
    })),
  ));
  await tester.pump();
  return l10n;
}

void main() {
  // The skill-run op had no case in either switch, so a running skill
  // showed up as the generic "operations" label with an hourglass —
  // in the Activity list and in the run sheet, which now renders this
  // card so the run can be cancelled.
  testWidgets('a skill run op is labelled as a run, not a generic operation',
      (tester) async {
    final l10n = await pumpWithL10n(tester, (_) => const SizedBox());
    expect(opKindTitle('skill_run', l10n), l10n.skillsRunning);
    expect(opKindTitle('skill_run', l10n), isNot(l10n.lensSectionOps));
    expect(opKindIcon('skill_run'), Icons.account_tree_outlined);
  });

  testWidgets('a running skill op card names the run and offers no dead action',
      (tester) async {
    const snapshot = <String, dynamic>{
      'op_id': 'op-1',
      'kind': 'skill_run',
      'state': 'running',
      'phase': 'running',
      'detail': 'placeholder-fill',
    };
    final l10n = await pumpWithL10n(
        tester, (_) => const OpProgressCard(progress: snapshot));
    expect(find.text(l10n.skillsRunning), findsOneWidget);
    // The graph id travels as the technical identifier (LTR).
    expect(find.text('placeholder-fill'), findsOneWidget);
    // No service to cancel through: the action must be absent rather
    // than present and dead.
    expect(find.text(l10n.cancelAction), findsNothing);
  });

  // Once cancellation is requested the button goes away, so it cannot
  // be pressed again while the core is winding the op down.
  testWidgets('a cancelling op withdraws the cancel button', (tester) async {
    const snapshot = <String, dynamic>{
      'op_id': 'op-1',
      'kind': 'skill_run',
      'state': 'cancelling',
      'phase': 'running',
      'detail': 'placeholder-fill',
    };
    final l10n = await pumpWithL10n(
        tester, (_) => const OpProgressCard(progress: snapshot));
    expect(find.widgetWithText(TextButton, l10n.cancelAction), findsNothing);
  });
}
