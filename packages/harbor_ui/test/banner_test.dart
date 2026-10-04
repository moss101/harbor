import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

Widget host(Widget child,
    {TextDirection dir = TextDirection.ltr, double textScale = 1}) {
  final rtl = dir == TextDirection.rtl;
  return HarborTheme(
    colors: HarborColors.light,
    text: HarborType(arabic: rtl),
    child: MaterialApp(
      theme: harborThemeData(dark: false, arabic: rtl),
      builder: (context, app) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: Directionality(textDirection: dir, child: app!),
      ),
      home: Scaffold(body: Align(alignment: Alignment.topCenter, child: child)),
    ),
  );
}

void size(WidgetTester tester, double w, double h) {
  tester.view.physicalSize = Size(w, h);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

const longBody =
    'Some parts of this file are preserved but cannot be previewed here, '
    'including embedded fonts, custom XML and a macro project.';

HarborBanner oneLine({
  HarborBannerTone tone = HarborBannerTone.warning,
  String? body = longBody,
  String? actionLabel = 'Show',
  VoidCallback? onAction,
}) =>
    HarborBanner(
      tone: tone,
      oneLine: true,
      title: 'Compatibility',
      body: body,
      actionLabel: actionLabel,
      onAction: onAction,
    );

void main() {
  group('HarborBanner oneLine', () {
    testWidgets('renders a single ellipsised line, body before title',
        (tester) async {
      size(tester, 360, 640);
      await tester.pumpWidget(host(SizedBox(width: 340, child: oneLine())));
      final text = tester.widget<Text>(find.text(longBody));
      expect(text.maxLines, 1);
      expect(text.overflow, TextOverflow.ellipsis);
      // The title is not stacked above it.
      expect(find.text('Compatibility'), findsNothing);
      // One line tall: the 44 px floor, nowhere near a stacked banner.
      final h = tester.getSize(find.byType(HarborBanner)).height;
      expect(h, greaterThanOrEqualTo(44));
      expect(h, lessThan(60));
      expect(tester.takeException(), isNull);
    });

    testWidgets('falls back to the title without a body', (tester) async {
      await tester
          .pumpWidget(host(SizedBox(width: 340, child: oneLine(body: null))));
      expect(find.text('Compatibility'), findsOneWidget);
    });

    testWidgets('keeps the minimum 44 px height and button target',
        (tester) async {
      await tester.pumpWidget(host(SizedBox(
          width: 340, child: oneLine(body: 'Short', onAction: () {}))));
      expect(tester.getSize(find.byType(HarborBanner)).height,
          greaterThanOrEqualTo(44));
      final button = tester.getSize(find.byType(TextButton));
      expect(button.height, greaterThanOrEqualTo(44));
      expect(button.width, greaterThanOrEqualTo(44));
    });

    testWidgets('tapping the row or the button both run onAction',
        (tester) async {
      var calls = 0;
      await tester.pumpWidget(
          host(SizedBox(width: 340, child: oneLine(onAction: () => calls++))));
      await tester.tap(find.text(longBody));
      expect(calls, 1);
      await tester.tap(find.widgetWithText(TextButton, 'Show'));
      expect(calls, 2);
      // The leading icon is part of the row too.
      await tester.tap(find.byIcon(Icons.warning_amber_outlined));
      expect(calls, 3);
    });

    testWidgets('a banner without an action label has no button',
        (tester) async {
      var calls = 0;
      await tester.pumpWidget(host(SizedBox(
          width: 340,
          child: oneLine(actionLabel: null, onAction: () => calls++))));
      expect(find.byType(TextButton), findsNothing);
      await tester.tap(find.text(longBody));
      expect(calls, 1);
    });

    testWidgets('without onAction nothing is tappable', (tester) async {
      await tester.pumpWidget(host(SizedBox(width: 340, child: oneLine())));
      final button = tester.widget<TextButton>(find.byType(TextButton));
      expect(button.onPressed, isNull);
      await tester.tap(find.text(longBody));
      expect(tester.takeException(), isNull);
    });

    testWidgets('large text still keeps one line and does not overflow',
        (tester) async {
      size(tester, 360, 640);
      await tester.pumpWidget(host(
          SizedBox(width: 340, child: oneLine(onAction: () {})),
          textScale: 2));
      expect(tester.widget<Text>(find.text(longBody)).maxLines, 1);
      expect(tester.takeException(), isNull);
      expect(tester.getSize(find.widgetWithText(TextButton, 'Show')).height,
          greaterThanOrEqualTo(44));
    });

    testWidgets('RTL: the icon starts on the right, the button on the left',
        (tester) async {
      size(tester, 360, 640);
      await tester.pumpWidget(host(
          SizedBox(
              width: 340,
              child:
                  oneLine(body: 'تعذّر عرض بعض الأجزاء', actionLabel: 'عرض')),
          dir: TextDirection.rtl));
      final icon = tester.getCenter(find.byIcon(Icons.warning_amber_outlined));
      final button = tester.getCenter(find.widgetWithText(TextButton, 'عرض'));
      final text = tester.getCenter(find.text('تعذّر عرض بعض الأجزاء'));
      expect(icon.dx, greaterThan(text.dx));
      expect(button.dx, lessThan(text.dx));
      expect(tester.takeException(), isNull);
    });

    testWidgets('semantics: a labelled button; live region unless info',
        (tester) async {
      final handle = tester.ensureSemantics();
      await tester.pumpWidget(host(SizedBox(
          width: 340,
          child: oneLine(body: 'Parts preserved', onAction: () {}))));
      expect(
          tester.getSemantics(find.widgetWithText(TextButton, 'Show')),
          matchesSemantics(
              label: 'Show',
              isButton: true,
              isFocusable: true,
              hasEnabledState: true,
              isEnabled: true,
              hasTapAction: true,
              hasFocusAction: true));
      final container = tester.getSemantics(find.byType(HarborBanner));
      expect(container.getSemanticsData().flagsCollection.isLiveRegion, isTrue);
      expect(find.bySemanticsLabel(RegExp('Parts preserved')), findsWidgets);

      await tester.pumpWidget(host(SizedBox(
          width: 340,
          child: oneLine(tone: HarborBannerTone.info, onAction: () {}))));
      expect(
          tester
              .getSemantics(find.byType(HarborBanner))
              .getSemanticsData()
              .flagsCollection
              .isLiveRegion,
          isFalse);
      handle.dispose();
    });

    testWidgets('the stacked banner is unchanged without oneLine',
        (tester) async {
      size(tester, 360, 640);
      await tester.pumpWidget(host(SizedBox(
          width: 340,
          child: HarborBanner(
              tone: HarborBannerTone.warning,
              title: 'Compatibility',
              body: longBody,
              action:
                  TextButton(onPressed: () {}, child: const Text('Show'))))));
      expect(find.text('Compatibility'), findsOneWidget);
      expect(tester.getSize(find.byType(HarborBanner)).height, greaterThan(80));
    });
  });
}
