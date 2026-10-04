import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/l10n/app_localizations.dart';
import 'package:harbor_office/surfaces/work/workbook_view.dart';
import 'package:harbor_ui/harbor_ui.dart';

// A new workbook has no cell records at all. Its blank cells must still be
// selectable AND editable, or a new spreadsheet cannot be typed into.
Widget _host(Map preview) => MaterialApp(
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: const [Locale('en')],
      theme: harborThemeData(dark: false, arabic: false),
      builder: (_, child) => HarborTheme(
        colors: HarborColors.light,
        text: const HarborType(arabic: false),
        child: child!,
      ),
      home: Scaffold(body: WorkbookView(preview: preview)),
    );

void main() {
  testWidgets(
      'the formula bar accepts input for a blank cell of an empty sheet',
      (tester) async {
    tester.view.physicalSize = const Size(900, 1400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_host({
      'sheets': ['Sheet1'],
      'sheet': 'Sheet1',
      'cells': <Map>[],
    }));
    await tester.pump();

    TextField field() => tester.widget<TextField>(find.byType(TextField).first);
    expect(field().enabled, isTrue, reason: 'A1 is selected and blank');

    await tester.enterText(find.byType(TextField).first, '=SUM(1,2)');
    expect(find.text('=SUM(1,2)'), findsOneWidget);
    final commit = tester
        .widget<IconButton>(find.widgetWithIcon(IconButton, Icons.check).first);
    expect(commit.onPressed, isNotNull, reason: 'commit is available');
  });

  testWidgets('an edit that reloads the sheet keeps the selected cell',
      (tester) async {
    tester.view.physicalSize = const Size(900, 1400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    Map preview(List<Map> cells, String sheet) => {
          'sheets': ['Sheet1', 'Sheet2'],
          'sheet': sheet,
          'cells': cells,
        };
    final first = preview([
      {'row': 1, 'col': 1, 'formula': 'SUM(1,2)', 'value': 3},
    ], 'Sheet1');
    await tester.pumpWidget(_host(first));
    await tester.pump();
    expect(find.text('A1'), findsWidgets, reason: 'starts on the formula');

    // Select B2 (blank), then the core reloads the same sheet after an edit.
    await tester.tap(find.byKey(const ValueKey('cell-2-2')));
    await tester.pump();
    expect(find.text('B2'), findsWidgets);
    await tester.pumpWidget(_host(preview([
      {'row': 1, 'col': 1, 'formula': 'SUM(1,2)', 'value': 3},
      {'row': 2, 'col': 2, 'value': 7},
    ], 'Sheet1')));
    await tester.pump();
    expect(find.text('B2'), findsWidgets, reason: 'stays on the edited cell');

    // A different sheet starts fresh.
    await tester.pumpWidget(_host(preview(const <Map>[], 'Sheet2')));
    await tester.pump();
    expect(find.text('A1'), findsWidgets);
  });
}
