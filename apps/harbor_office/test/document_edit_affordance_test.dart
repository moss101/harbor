import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/l10n/app_localizations.dart';
import 'package:harbor_office/surfaces/work/document_view.dart';
import 'package:harbor_ui/harbor_ui.dart';

// A long-press on a touch screen starts text selection, so on phones every
// paragraph needs a visible edit button; wide layouts keep long-press only.
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
      home: Scaffold(body: DocumentView(preview: preview)),
    );

final _preview = {
  'paragraphs': [
    {'index': 1, 'text': 'Quarterly report', 'style': 'Title'},
    {'index': 2, 'text': 'Revenue grew in every region.', 'style': 'Normal'},
  ],
};

void main() {
  testWidgets('phones get an edit button per paragraph', (tester) async {
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_host(_preview));
    await tester.pump();
    expect(find.byTooltip('Edit paragraph 1'), findsOneWidget);
    expect(find.byTooltip('Edit paragraph 2'), findsOneWidget);
    final size = tester.getSize(find.byTooltip('Edit paragraph 2'));
    expect(size.width, greaterThanOrEqualTo(44));
    expect(size.height, greaterThanOrEqualTo(44));
  });

  testWidgets('wide layouts keep long-press and show no extra buttons',
      (tester) async {
    tester.view.physicalSize = const Size(1400, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_host(_preview));
    await tester.pump();
    expect(find.byIcon(Icons.edit_outlined), findsNothing);
  });
}
