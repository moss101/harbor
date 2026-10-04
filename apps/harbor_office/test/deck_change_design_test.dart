import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart' show SemanticsNode;
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_office/l10n/app_localizations.dart';
import 'package:harbor_office/services/harbor_service.dart';
import 'package:harbor_office/surfaces/work/deck_view.dart';
import 'package:harbor_ui/deck_designer.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:harbor_ui/slides.dart';

import 'package:harbor_ui/deck_designer_testing.dart';

Map<String, dynamic> _scene(int slides) => {
      'width': 12192000,
      'height': 6858000,
      'slides': [
        for (var i = 1; i <= slides; i++)
          {
            'index': i,
            'background': {'k': 'solid', 'c': '#112233', 'a': 1},
            'items': <dynamic>[],
          },
      ],
    };

Map<String, dynamic> preview({
  Object? design = const {
    'theme': 'aurora',
    'theme_name': 'Aurora',
    'palette': null,
    'motion': 'subtle',
    'rtl': false,
    'restylable': true,
  },
  int slides = 3,
}) =>
    {
      'kind': 'deck',
      'title': 'Deck',
      'slides': [
        for (var i = 1; i <= slides; i++)
          {'index': i, 'title': 'Slide $i', 'bullets': <String>[]},
      ],
      'scene': _scene(slides),
      'design': design,
    };

Widget host(Widget Function() child, {Locale locale = const Locale('en')}) {
  return MaterialApp(
    locale: locale,
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: AppLocalizations.supportedLocales,
    theme: harborThemeData(dark: false, arabic: locale.languageCode == 'ar'),
    builder: (context, c) => HarborTheme(
      colors: HarborColors.light,
      text: HarborType(arabic: locale.languageCode == 'ar'),
      child: HarborServiceProvider(service: null, failed: false, child: c!),
    ),
    home: Scaffold(body: child()),
  );
}

void size(WidgetTester tester, double w, double h) {
  tester.view.physicalSize = Size(w, h);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

Future<void> settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 50));
}

const disabledText =
    "This deck was saved by another app, so its design can't be changed here.";

void main() {
  late FakeDeckDesigner designer;
  setUp(() => designer = FakeDeckDesigner());

  Finder designButton() => find.byKey(const ValueKey('deck-change-design'));
  VoidCallback? onPressed(WidgetTester tester) =>
      tester.widget<ButtonStyleButton>(designButton()).onPressed;

  testWidgets('disabled with an explanation when the deck is not restylable',
      (tester) async {
    size(tester, 1200, 800);
    for (final design in <Object?>[
      null,
      const {'theme': 'aurora', 'restylable': false},
    ]) {
      await tester.pumpWidget(host(() =>
          DeckView(preview: preview(design: design), designer: designer)));
      await tester.pump();
      expect(find.text('Design'), findsOneWidget);
      expect(onPressed(tester), isNull, reason: '$design');
      expect(find.byTooltip(disabledText), findsOneWidget);
      await tester.tap(designButton(), warnIfMissed: false);
      await tester.pump();
      expect(find.text('Change design'), findsNothing);
    }
    expect(designer.catalogCalls, 0);
  });

  testWidgets('enabled for a restylable deck, with the current design shown',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(
        host(() => DeckView(preview: preview(), designer: designer)));
    await tester.pump();
    expect(onPressed(tester), isNotNull);
    expect(find.byTooltip('Change design'), findsOneWidget);
    await tester.tap(designButton());
    await settle(tester);
    await settle(tester);
    expect(find.text('Change design'), findsWidgets);
    expect(find.text('Current design: Aurora'), findsOneWidget);
    // Aurora is preselected; Apply is disabled until something changes.
    expect(
        find.descendant(
            of: find.byKey(const ValueKey('deck-design-aurora')),
            matching: find.byIcon(Icons.check_circle)),
        findsOneWidget);
    final apply = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Apply design'));
    expect(apply.onPressed, isNull);
    expect(designer.restyles, isEmpty);
  });

  testWidgets('applying calls restyle and keeps the slide position',
      (tester) async {
    size(tester, 1200, 800);
    var current = preview();
    late StateSetter set;
    await tester.pumpWidget(host(() => StatefulBuilder(builder: (c, s) {
          set = s;
          return DeckView(preview: current, designer: designer);
        })));
    await tester.pump();
    // Go to slide 3.
    await tester.tap(find.byType(SlideThumbnail).at(2));
    await tester.pump();
    expect(find.text('Slide 3 · 3 slides'), findsOneWidget);

    await tester.tap(designButton());
    await settle(tester);
    await settle(tester);
    await tester.tap(find.byKey(const ValueKey('deck-design-ocean')));
    await tester.tap(find.byKey(const ValueKey('deck-palette-mint')));
    await tester.pump();
    await tester.tap(find.widgetWithText(FilledButton, 'Apply design'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(designer.restyles, hasLength(1));
    expect(
        designer.restyles.single,
        const DeckStyleChoice(
            design: 'ocean', palette: 'mint', motion: 'subtle', rtl: false));
    expect(
        find.text('Design changed. Save a copy to keep it.'), findsOneWidget);

    // The service swapped the preview underneath: the same slide is shown.
    set(() => current = preview(design: {
          'theme': 'ocean',
          'theme_name': 'Ocean',
          'palette': 'mint',
          'motion': 'subtle',
          'rtl': false,
          'restylable': true,
        }));
    await tester.pump();
    expect(find.text('Slide 3 · 3 slides'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('keeps the deck\'s own palette, motion and direction as the base',
      (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(() => DeckView(
        preview: preview(design: {
          'theme': 'slate',
          'theme_name': 'Slate',
          'palette': 'terracotta',
          'motion': 'dynamic',
          'rtl': true,
          'restylable': true,
        }),
        designer: designer)));
    await tester.pump();
    await tester.tap(designButton());
    await settle(tester);
    await settle(tester);
    await tester.tap(find.byKey(const ValueKey('deck-design-ivory')));
    await tester.pump();
    await tester.tap(find.widgetWithText(FilledButton, 'Apply design'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(
        designer.restyles.single,
        const DeckStyleChoice(
            design: 'ivory',
            palette: 'terracotta',
            motion: 'dynamic',
            rtl: true));
  });

  testWidgets('a restyle error is shown and nothing changes', (tester) async {
    size(tester, 1200, 800);
    designer.failRestyle = 1;
    await tester.pumpWidget(
        host(() => DeckView(preview: preview(), designer: designer)));
    await tester.pump();
    await tester.tap(designButton());
    await settle(tester);
    await settle(tester);
    await tester.tap(find.byKey(const ValueKey('deck-design-slate')));
    await tester.pump();
    await tester.tap(find.widgetWithText(FilledButton, 'Apply design'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.textContaining("Couldn't change the design"), findsOneWidget);
    expect(find.textContaining('restyle refused'), findsOneWidget);
    expect(onPressed(tester), isNotNull, reason: 'the user can try again');
  });

  testWidgets('cancel leaves the deck alone', (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(
        host(() => DeckView(preview: preview(), designer: designer)));
    await tester.pump();
    await tester.tap(designButton());
    await settle(tester);
    await settle(tester);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    expect(designer.restyles, isEmpty);
    expect(find.text('Current design: Aurora'), findsNothing);
  });

  testWidgets('catalogue failure inside the dialog offers a retry',
      (tester) async {
    size(tester, 1200, 800);
    designer.failCatalog = 1;
    await tester.pumpWidget(
        host(() => DeckView(preview: preview(), designer: designer)));
    await tester.pump();
    await tester.tap(designButton());
    await settle(tester);
    expect(find.textContaining('catalogue offline'), findsOneWidget);
    await tester.tap(find.text('Try again'));
    await settle(tester);
    await settle(tester);
    expect(find.text('Current design: Aurora'), findsOneWidget);
  });

  testWidgets('phone width and Arabic: the dialog lays out', (tester) async {
    for (final locale in [const Locale('en'), const Locale('ar')]) {
      size(tester, 360, 740);
      await tester.pumpWidget(KeyedSubtree(
          key: UniqueKey(),
          child: host(() => DeckView(preview: preview(), designer: designer),
              locale: locale)));
      await tester.pump();
      await tester.tap(designButton());
      await settle(tester);
      await settle(tester);
      expect(tester.takeException(), isNull, reason: locale.languageCode);
      expect(find.byKey(const ValueKey('deck-design-ocean')), findsOneWidget);
    }
  });

  testWidgets('the Design action is labelled for assistive tech',
      (tester) async {
    final handle = tester.ensureSemantics();
    size(tester, 1200, 800);
    await tester.pumpWidget(host(
        () => DeckView(preview: preview(design: null), designer: designer)));
    await tester.pump();
    var node = tester.getSemantics(find.text('Design'));
    final seen = StringBuffer();
    for (SemanticsNode? n = node; n != null; n = n.parent) {
      final d = n.getSemanticsData();
      seen.write('${d.label} ${d.tooltip} ${d.hint}|');
    }
    expect(seen.toString(), contains('Design'));
    expect(seen.toString(), contains("can't be changed here"));
    handle.dispose();
  });
}
