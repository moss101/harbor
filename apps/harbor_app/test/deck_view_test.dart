import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/surfaces/work/deck_view.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:harbor_ui/slides.dart';

Map<String, dynamic> _shape(int id, String text) => {
      't': 'shape',
      'id': id,
      'name': 'Shape $id',
      'x': 914400,
      'y': 914400,
      'w': 8000000,
      'h': 1400000,
      'rot': 0,
      'flip_h': false,
      'flip_v': false,
      'geom': {'k': 'preset', 'name': 'rect', 'adj': []},
      'fill': {'k': 'none'},
      'text': {
        'ins': [91440, 45720, 91440, 45720],
        'anchor': 'ctr',
        'wrap': true,
        'autofit': 'none',
        'font_scale': 1,
        'line_reduction': 0,
        'paras': [
          {
            'algn': 'l',
            'runs': [
              {'t': text, 'sz': 32, 'c': '#000000', 'a': 1}
            ]
          }
        ],
      },
    };

/// A three-slide deck: slide 2 carries a transition and animations, slide 3
/// an unsupported feature.
Map<String, dynamic> sceneJson() => {
      'width': 12192000,
      'height': 6858000,
      'slides': [
        {
          'index': 1,
          'background': {'k': 'solid', 'c': '#FFFFFF', 'a': 1},
          'items': [_shape(1, 'Quarterly review')],
        },
        {
          'index': 2,
          'background': {'k': 'solid', 'c': '#DDEEFF', 'a': 1},
          'items': [_shape(2, 'Growth'), _shape(3, 'Costs')],
          'transition': {
            'kind': 'fade',
            'dir': null,
            'dur_ms': 400,
            'on_click': true,
            'after_ms': null
          },
          'animations': [
            {
              'shape_id': 3,
              'para': null,
              'effect': 'fade',
              'phase': 'entrance',
              'dir': null,
              'trigger': 'click',
              'delay_ms': 0,
              'dur_ms': 300
            }
          ],
          'notes': 'Say hello',
        },
        {
          'index': 3,
          'background': {'k': 'solid', 'c': '#EEFFEE', 'a': 1},
          'items': [_shape(4, 'Appendix')],
          'unsupported': ['SmartArt (Content diagram)'],
        },
      ],
    };

Map<String, dynamic> previewWith(Object? scene) => {
      'kind': 'deck',
      'title': 'Deck',
      'slides': [
        {
          'index': 1,
          'title': 'Quarterly review',
          'bullets': ['Revenue up']
        },
        {'index': 2, 'title': 'Growth', 'bullets': <String>[]},
        {'index': 3, 'title': 'Appendix', 'bullets': <String>[]},
      ],
      'scene': scene,
    };

Widget host(
  Widget Function() child, {
  Locale locale = const Locale('en'),
  bool disableAnimations = false,
}) {
  return MaterialApp(
    locale: locale,
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: const [Locale('en'), Locale('ar')],
    theme: harborThemeData(dark: false, arabic: locale.languageCode == 'ar'),
    builder: (context, c) => MediaQuery(
      data:
          MediaQuery.of(context).copyWith(disableAnimations: disableAnimations),
      child: HarborTheme(
        colors: HarborColors.light,
        text: HarborType(arabic: locale.languageCode == 'ar'),
        child: c!,
      ),
    ),
    home: Scaffold(body: child()),
  );
}

void size(WidgetTester tester, double w, double h) {
  tester.view.physicalSize = Size(w, h);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

void main() {
  testWidgets('with a scene: a big SlideView and a thumbnail per slide',
      (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    expect(tester.takeException(), isNull);
    expect(find.byType(SlideThumbnail), findsNWidgets(3));
    // Thumbnails wrap SlideViews too; the canvas is the one that is not
    // inside a SlideThumbnail.
    final canvasViews = find.byWidgetPredicate(
        (w) => w is SlideView && w.scene.index == 1,
        description: 'slide 1 view');
    expect(canvasViews, findsNWidgets(2)); // canvas + its thumbnail
    // The text fallback is not shown.
    expect(find.text('Revenue up'), findsNothing);
  });

  testWidgets('tapping a thumbnail selects that slide', (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    await tester.tap(find.byType(SlideThumbnail).at(1));
    await tester.pump();
    expect(find.text('Slide 2 · 3 slides'), findsOneWidget);
    // Slide 2 has motion: the preview button appears.
    expect(find.text('Preview motion'), findsOneWidget);
    await tester.tap(find.byTooltip('Slide 3'));
    await tester.pump();
    expect(find.text('Slide 3 · 3 slides'), findsOneWidget);
    expect(find.text('Preview motion'), findsNothing);
  });

  testWidgets('semantics: thumbnails are labelled slide buttons',
      (tester) async {
    final handle = tester.ensureSemantics();
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    expect(find.bySemanticsLabel('Slide 2'), findsWidgets);
    handle.dispose();
  });

  testWidgets('unsupported content on a slide is stated honestly',
      (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    expect(find.textContaining("can't be previewed"), findsNothing);
    await tester.tap(find.byType(SlideThumbnail).at(2));
    await tester.pump();
    expect(
        find.text(
            "Some content on this slide can't be previewed: SmartArt (Content diagram)"),
        findsOneWidget);
  });

  group('text fallback', () {
    testWidgets('null scene keeps the text rendering', (tester) async {
      size(tester, 1200, 800);
      await tester.pumpWidget(host(() => DeckView(preview: previewWith(null))));
      await tester.pump();
      expect(find.byType(SlideView), findsNothing);
      expect(find.text('Revenue up'), findsOneWidget);
    });

    testWidgets('absent, malformed or empty scenes fall back too',
        (tester) async {
      size(tester, 1200, 800);
      for (final scene in <Object?>[
        'garbage',
        <String, dynamic>{'slides': 'nope'},
        <String, dynamic>{'width': 1, 'height': 1, 'slides': <dynamic>[]},
        <String, dynamic>{'slides': 3},
      ]) {
        final preview = previewWith(scene);
        await tester.pumpWidget(host(() => DeckView(preview: preview)));
        await tester.pump();
        expect(find.byType(SlideView), findsNothing, reason: '$scene');
        expect(find.text('Revenue up'), findsOneWidget, reason: '$scene');
      }
      final noKey = previewWith(null)..remove('scene');
      await tester.pumpWidget(host(() => DeckView(preview: noKey)));
      await tester.pump();
      expect(find.text('Revenue up'), findsOneWidget);
    });
  });

  testWidgets('the scene is parsed once per scene map', (tester) async {
    size(tester, 1200, 800);
    final scene = sceneJson();
    late StateSetter set;
    var preview = previewWith(scene);
    await tester.pumpWidget(host(() => StatefulBuilder(builder: (c, s) {
          set = s;
          return DeckView(preview: preview);
        })));
    await tester.pump();
    SlideScene firstOf() => tester
        .widgetList<SlideView>(find.byType(SlideView))
        .firstWhere((v) => v.scene.index == 1)
        .scene;
    final a = firstOf();
    set(() => preview = previewWith(scene)); // new outer map, same scene map
    await tester.pump();
    expect(identical(firstOf(), a), isTrue);
    set(() => preview = previewWith(sceneJson())); // a different scene map
    await tester.pump();
    expect(identical(firstOf(), a), isFalse);
  });

  testWidgets('Present opens the presenter at the selected slide',
      (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    await tester.tap(find.byType(SlideThumbnail).at(1));
    await tester.pump();
    await tester.tap(find.text('Present'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    expect(find.byType(PresentView), findsOneWidget);
    expect(find.text('Slide 2 of 3'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(PresentView), findsNothing);
  });

  testWidgets('From beginning starts at slide 1', (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    expect(find.text('From beginning'), findsNothing); // already on slide 1
    await tester.tap(find.byType(SlideThumbnail).at(2));
    await tester.pump();
    await tester.tap(find.text('From beginning'));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
    expect(find.text('Slide 1 of 3'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
  });

  testWidgets('Preview motion plays inline and then returns', (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    await tester.tap(find.byType(SlideThumbnail).at(1));
    await tester.pump();
    await tester.tap(find.text('Preview motion'));
    await tester.pump();
    expect(find.text('Stop preview'), findsOneWidget);
    // Transition in (400 ms), then the click step (300 ms) after a pause.
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.byType(TransitionView), findsOneWidget);
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byType(TransitionView), findsNothing);
    await tester.pump(const Duration(seconds: 3));
    await tester.pump();
    expect(find.text('Preview motion'), findsOneWidget);
    expect(find.text('Stop preview'), findsNothing);
  });

  testWidgets('Stop preview ends it early', (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    await tester.tap(find.byType(SlideThumbnail).at(1));
    await tester.pump();
    await tester.tap(find.text('Preview motion'));
    await tester.pump();
    await tester.tap(find.text('Stop preview'));
    await tester.pump();
    expect(find.text('Preview motion'), findsOneWidget);
    expect(find.byType(TransitionView), findsNothing);
  });

  testWidgets('reduce motion disables the inline preview', (tester) async {
    size(tester, 1200, 800);
    await tester.pumpWidget(host(
        () => DeckView(preview: previewWith(sceneJson())),
        disableAnimations: true));
    await tester.pump();
    await tester.tap(find.byType(SlideThumbnail).at(1));
    await tester.pump();
    final btn = tester.widget<ButtonStyleButton>(find.ancestor(
        of: find.text('Preview motion'),
        matching: find.bySubtype<ButtonStyleButton>()));
    expect(btn.onPressed, isNull);
  });

  testWidgets('the read-only deck view offers no edit actions', (tester) async {
    size(tester, 1200, 800);
    await tester
        .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
    await tester.pump();
    expect(find.byTooltip('Edit slide 1 title'), findsNothing);
    await tester.longPress(find.byType(SlideView).first);
    await tester.pump();
    expect(tester.takeException(), isNull);
  });

  group('layout', () {
    testWidgets('phone width: strip filmstrip, no overflow', (tester) async {
      size(tester, 360, 740);
      await tester
          .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(find.byType(SlideThumbnail), findsNWidgets(3));
      final list = tester.widget<ListView>(find.byType(ListView).last);
      expect(list.scrollDirection, Axis.horizontal);
      // The strip scrolls: bring the last thumbnail in, then select it.
      await tester.ensureVisible(find.byType(SlideThumbnail).at(2));
      await tester.pump();
      await tester.tap(find.byType(SlideThumbnail).at(2));
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(find.text('Slide 3 · 3 slides'), findsOneWidget);
    });

    testWidgets('desktop width: vertical filmstrip', (tester) async {
      size(tester, 1400, 900);
      await tester
          .pumpWidget(host(() => DeckView(preview: previewWith(sceneJson()))));
      await tester.pump();
      final list = tester.widget<ListView>(find.byType(ListView).first);
      expect(list.scrollDirection, Axis.vertical);
      expect(tester.takeException(), isNull);
    });

    testWidgets('right-to-left (Arabic) lays out without overflow',
        (tester) async {
      size(tester, 360, 740);
      await tester.pumpWidget(host(
          () => DeckView(preview: previewWith(sceneJson())),
          locale: const Locale('ar')));
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(find.byType(SlideThumbnail), findsNWidgets(3));
      size(tester, 1200, 800);
      await tester.pumpWidget(host(
          () => DeckView(preview: previewWith(sceneJson())),
          locale: const Locale('ar')));
      await tester.pump();
      expect(tester.takeException(), isNull);
    });

    testWidgets('a 4:3 deck keeps its aspect ratio on a phone', (tester) async {
      size(tester, 360, 740);
      final scene = sceneJson()
        ..['width'] = 9144000
        ..['height'] = 6858000;
      await tester
          .pumpWidget(host(() => DeckView(preview: previewWith(scene))));
      await tester.pump();
      expect(tester.takeException(), isNull);
    });
  });
}
