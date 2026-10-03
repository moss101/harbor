import 'dart:ui' show SemanticsAction, SemanticsFlags, Tristate;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart' show SemanticsNode;
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:harbor_ui/deck_designer.dart';
import 'package:harbor_ui/slides.dart';

Widget host(Widget child,
    {TextDirection dir = TextDirection.ltr, double textScale = 1}) {
  return HarborTheme(
    colors: HarborColors.light,
    text: HarborType(arabic: dir == TextDirection.rtl),
    child: MaterialApp(
      theme: harborThemeData(dark: false, arabic: dir == TextDirection.rtl),
      builder: (context, app) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: Directionality(textDirection: dir, child: app!),
      ),
      home: Scaffold(body: child),
    ),
  );
}

SemanticsFlags flags(SemanticsNode n) => n.getSemanticsData().flagsCollection;
bool isSel(SemanticsNode n) => flags(n).isSelected == Tristate.isTrue;

void size(WidgetTester tester, double w, double h) {
  tester.view.physicalSize = Size(w, h);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

SlideScene sceneOf(String colour) => SlideScene(
    index: 1, background: ScenePaint(kind: PaintKind.solid, c: colour, a: 1));

const designs = [
  DeckDesignInfo(
      id: 'aurora',
      name: 'Aurora',
      tagline: 'Glowing gradients',
      dark: true,
      swatches: [
        '#111111',
        '#222222',
        '#333333',
        '#444444',
        '#555555',
        '#666666',
        '#0A0A14',
        '#FFFFFF'
      ]),
  DeckDesignInfo(id: 'ivory', name: 'Ivory', tagline: 'Quiet editorial'),
  DeckDesignInfo(id: 'slate', name: 'Slate', tagline: 'Calm and corporate'),
  DeckDesignInfo(id: 'ocean', name: 'Ocean', tagline: 'Deep blues'),
];

const palettes = [
  DeckPaletteInfo(
      id: 'terracotta',
      name: 'Terracotta',
      mood: 'warm',
      swatches: [
        '#C2410C',
        '#EA580C',
        '#F59E0B',
        '#65A30D',
        '#0F766E',
        '#1D4ED8',
        '#FFF7ED',
        '#1C1917'
      ]),
  DeckPaletteInfo(id: 'mint', name: 'Mint', mood: 'fresh', swatches: [
    '#10B981',
    '#34D399',
    '#059669',
    '#064E3B',
    '#A7F3D0',
    '#ECFDF5',
    '#F0FDF4',
    '#052E16'
  ]),
];

const motions = [
  DeckMotionInfo(id: 'none', description: 'No motion'),
  DeckMotionInfo(id: 'subtle', description: 'Gentle fades'),
  DeckMotionInfo(id: 'dynamic', description: 'Pushes, floats and zooms'),
  DeckMotionInfo(id: 'cinematic', description: 'Ripple and vortex'),
];

const purposes = [
  DeckPurposeInfo(
      id: 'pitch_deck', name: 'Pitch deck', summary: 'Raise a round.'),
  DeckPurposeInfo(
      id: 'project_report', name: 'Project report', summary: 'Status.'),
];

Widget gallery({
  String? selected,
  ValueChanged<String>? onSelected,
  required DeckThumbnailBuilder builder,
  Object? thumbnailKey,
  Set<String> recommended = const {},
}) =>
    DeckDesignGallery(
      designs: designs,
      selected: selected,
      onSelected: onSelected ?? (_) {},
      thumbnailBuilder: builder,
      thumbnailKey: thumbnailKey,
      darkLabel: 'Dark',
      lightLabel: 'Light',
      recommended: recommended,
      recommendedLabel: 'Suggested',
      thumbnailUnavailableLabel: 'No preview',
      semanticsLabel: 'Designs',
    );

void main() {
  group('models', () {
    test('catalog parses the core shape (themes -> designs)', () {
      final json = <String, dynamic>{
        'themes': [
          {
            'id': 'aurora',
            'name': 'Aurora',
            'tagline': 't',
            'dark': true,
            'swatches': ['#112233'],
            'suits': ['pitch'],
            'major_font': 'A',
            'minor_font': 'B'
          }
        ],
        'palettes': [
          {'id': 'p', 'name': 'P', 'mood': 'm', 'dark': false, 'swatches': []}
        ],
        'purposes': [
          {
            'id': 'pitch_deck',
            'name': 'Pitch deck',
            'summary': 's',
            'theme': 'aurora',
            'alt_themes': ['obsidian'],
            'motion': 'cinematic',
            'progressive': false,
            'beats': [
              {'kicker': 'Problem', 'layout': 'statement', 'says': 'x'}
            ]
          }
        ],
        'motions': [
          {'id': 'none', 'description': 'No motion'}
        ],
        'layouts': ['cover'],
      };
      final c = DeckCatalog.fromJson(json);
      expect(c.designs.single.dark, isTrue);
      expect(c.design('aurora')!.majorFont, 'A');
      expect(c.palette('p')!.mood, 'm');
      expect(c.purpose('pitch_deck')!.altThemes, ['obsidian']);
      expect(c.purpose('pitch_deck')!.beats.single.kicker, 'Problem');
      expect(c.motions.single.id, 'none');
      expect(c.layouts, ['cover']);
    });

    test('tolerates missing fields', () {
      final empty = <String, dynamic>{};
      final c = DeckCatalog.fromJson(empty);
      expect(c.designs, isEmpty);
      final one = <String, dynamic>{'id': 'x'};
      final d = DeckDesignInfo.fromJson(one);
      expect(d.name, 'x');
      expect(parseSwatch('#GGGGGG'), isNull);
      expect(parseSwatch('#102030'), const Color(0xFF102030));
    });

    test('style choice equality and cache key', () {
      const a = DeckStyleChoice(design: 'aurora', palette: '');
      const b = DeckStyleChoice(design: 'aurora');
      expect(a, b);
      expect(a.copyWith(rtl: true).cacheKey, isNot(a.cacheKey));
      expect(a.copyWith(palette: 'mint').palette, 'mint');
      expect(a.copyWith(palette: 'mint').copyWith(clearPalette: true).palette,
          isNull);
    });
  });

  group('DeckDesignGallery', () {
    testWidgets('shows name, tagline, mode chip and a live SlideView',
        (tester) async {
      size(tester, 900, 700);
      await tester.pumpWidget(host(gallery(
        selected: 'aurora',
        builder: (id) async => sceneOf('#101010'),
        recommended: {'ivory'},
      )));
      await tester.pump();
      expect(find.text('Aurora'), findsOneWidget);
      expect(find.text('Glowing gradients'), findsOneWidget);
      expect(find.text('Dark'), findsOneWidget);
      expect(find.text('Light'), findsNWidgets(3));
      expect(find.text('Suggested'), findsOneWidget);
      expect(find.byType(SlideView), findsNWidgets(4));
      expect(tester.takeException(), isNull);
    });

    testWidgets('tap selects; selection is reported by id', (tester) async {
      size(tester, 900, 700);
      String? picked;
      await tester.pumpWidget(host(gallery(
          builder: (id) async => sceneOf('#FFFFFF'),
          onSelected: (id) => picked = id)));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('deck-design-slate')));
      expect(picked, 'slate');
    });

    testWidgets('keyboard: focus then Enter and Space select', (tester) async {
      size(tester, 900, 700);
      final picked = <String>[];
      await tester.pumpWidget(host(gallery(
          builder: (id) async => sceneOf('#FFFFFF'), onSelected: picked.add)));
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(picked, ['aurora']);
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.space);
      await tester.pump();
      expect(picked, ['aurora', 'ivory']);
    });

    testWidgets('semantics: image label and selected state', (tester) async {
      final handle = tester.ensureSemantics();
      size(tester, 900, 700);
      await tester.pumpWidget(host(gallery(
          selected: 'ivory', builder: (id) async => sceneOf('#FFFFFF'))));
      await tester.pump();
      expect(
          find.bySemanticsLabel('Aurora: Glowing gradients'), findsOneWidget);
      final img =
          tester.getSemantics(find.bySemanticsLabel('Ivory: Quiet editorial'));
      expect(flags(img).isImage, isTrue);
      final card =
          tester.getSemantics(find.byKey(const ValueKey('deck-design-ivory')));
      expect(isSel(card), isTrue);
      expect(flags(card).isButton, isTrue);
      final other =
          tester.getSemantics(find.byKey(const ValueKey('deck-design-ocean')));
      expect(isSel(other), isFalse);
      handle.dispose();
    });

    testWidgets('thumbnails are lazy: only built rows are requested',
        (tester) async {
      size(tester, 420, 300); // one column, only the first rows visible
      final many = [
        for (var i = 0; i < 14; i++)
          DeckDesignInfo(id: 'd$i', name: 'Design $i', tagline: 'tag $i'),
      ];
      final asked = <String>[];
      await tester.pumpWidget(host(DeckDesignGallery(
        designs: many,
        selected: null,
        onSelected: (_) {},
        thumbnailBuilder: (id) async {
          asked.add(id);
          return sceneOf('#FFFFFF');
        },
        darkLabel: 'Dark',
        lightLabel: 'Light',
      )));
      await tester.pump();
      expect(asked, contains('d0'));
      expect(asked.length, lessThan(many.length));
    });

    testWidgets('a new thumbnailKey re-requests and recolours', (tester) async {
      size(tester, 900, 700);
      var colour = '#101010';
      var calls = 0;
      Future<SlideScene?> build(String id) async {
        calls++;
        return sceneOf(colour);
      }

      await tester.pumpWidget(host(gallery(builder: build, thumbnailKey: 'a')));
      await tester.pump();
      final first = calls;
      expect(first, 4);
      // Same key, rebuilt: no new requests.
      await tester.pumpWidget(host(gallery(builder: build, thumbnailKey: 'a')));
      await tester.pump();
      expect(calls, first);
      colour = '#C2410C';
      await tester.pumpWidget(host(gallery(builder: build, thumbnailKey: 'b')));
      await tester.pump();
      expect(calls, first * 2);
      final bg = tester
          .widgetList<SlideView>(find.byType(SlideView))
          .first
          .scene
          .background
          .c;
      expect(bg, '#C2410C');
    });

    testWidgets('a failing thumbnail says so instead of throwing',
        (tester) async {
      size(tester, 900, 700);
      await tester.pumpWidget(
          host(gallery(builder: (id) async => throw StateError('boom'))));
      await tester.pump();
      expect(find.text('No preview'), findsNWidgets(4));
      expect(tester.takeException(), isNull);
    });

    testWidgets('RTL: the first design sits at the right', (tester) async {
      size(tester, 900, 700);
      await tester.pumpWidget(host(
          gallery(builder: (id) async => sceneOf('#FFFFFF')),
          dir: TextDirection.rtl));
      await tester.pump();
      final a =
          tester.getCenter(find.byKey(const ValueKey('deck-design-aurora')));
      final b =
          tester.getCenter(find.byKey(const ValueKey('deck-design-ivory')));
      expect(a.dx, greaterThan(b.dx));
      expect(tester.takeException(), isNull);
    });

    testWidgets('large text on a phone does not overflow', (tester) async {
      size(tester, 360, 700);
      await tester.pumpWidget(host(
          gallery(builder: (id) async => sceneOf('#FFFFFF')),
          textScale: 2));
      await tester.pump();
      expect(tester.takeException(), isNull);
    });
  });

  group('DeckPaletteStrip', () {
    Widget strip(String? selected, ValueChanged<String?> on,
            {bool own = true, bool horizontal = false}) =>
        DeckPaletteStrip(
          palettes: palettes,
          selected: selected,
          onSelected: on,
          ownColoursLabel: own ? "Design's own" : null,
          semanticsLabel: 'Palettes',
          horizontal: horizontal,
        );

    testWidgets('select a palette and return to the design colours',
        (tester) async {
      size(tester, 600, 400);
      final picked = <String?>[];
      await tester.pumpWidget(host(strip('mint', picked.add)));
      await tester.tap(find.byKey(const ValueKey('deck-palette-terracotta')));
      await tester.tap(find.byKey(const ValueKey('deck-palette-own')));
      expect(picked, ['terracotta', null]);
    });

    testWidgets('semantics: name + mood, selected, button', (tester) async {
      final handle = tester.ensureSemantics();
      size(tester, 600, 400);
      await tester.pumpWidget(host(strip('mint', (_) {})));
      final mint = tester.getSemantics(find.bySemanticsLabel('Mint, fresh'));
      expect(isSel(mint), isTrue);
      final t = tester.getSemantics(find.bySemanticsLabel('Terracotta, warm'));
      expect(isSel(t), isFalse);
      expect(flags(t).isButton, isTrue);
      expect(t.getSemanticsData().hasAction(SemanticsAction.tap), isTrue);
      handle.dispose();
    });

    testWidgets('null selection marks the own-colours chip', (tester) async {
      final handle = tester.ensureSemantics();
      size(tester, 600, 400);
      await tester.pumpWidget(host(strip(null, (_) {})));
      final own = tester.getSemantics(find.bySemanticsLabel("Design's own"));
      expect(isSel(own), isTrue);
      handle.dispose();
    });

    testWidgets('touch targets are at least 44 px', (tester) async {
      size(tester, 600, 400);
      await tester.pumpWidget(host(strip(null, (_) {})));
      for (final k in ['terracotta', 'mint', 'own']) {
        final s = tester.getSize(find.byKey(ValueKey('deck-palette-$k')));
        expect(s.width, greaterThanOrEqualTo(44));
        expect(s.height, greaterThanOrEqualTo(44));
      }
    });

    testWidgets('RTL and horizontal scrolling lay out cleanly', (tester) async {
      size(tester, 300, 400);
      await tester.pumpWidget(host(strip('mint', (_) {}, horizontal: true),
          dir: TextDirection.rtl));
      await tester.pump();
      final own =
          tester.getCenter(find.byKey(const ValueKey('deck-palette-own')));
      final t = tester
          .getCenter(find.byKey(const ValueKey('deck-palette-terracotta')));
      expect(own.dx, greaterThan(t.dx));
      expect(tester.takeException(), isNull);
    });
  });

  group('DeckMotionPicker', () {
    Widget picker(String selected, ValueChanged<String> on) => DeckMotionPicker(
          motions: motions,
          selected: selected,
          onSelected: on,
          names: const {
            'none': 'None',
            'subtle': 'Subtle',
            'dynamic': 'Dynamic',
            'cinematic': 'Cinematic'
          },
          semanticsLabel: 'Motion',
        );

    testWidgets('four choices with a one-line description each',
        (tester) async {
      size(tester, 500, 600);
      await tester.pumpWidget(host(picker('subtle', (_) {})));
      for (final s in ['None', 'Subtle', 'Dynamic', 'Cinematic']) {
        expect(find.text(s), findsOneWidget);
      }
      expect(find.text('Gentle fades'), findsOneWidget);
      expect(find.text('Pushes, floats and zooms'), findsOneWidget);
    });

    testWidgets('selection and semantics', (tester) async {
      final handle = tester.ensureSemantics();
      size(tester, 500, 600);
      String? picked;
      await tester.pumpWidget(host(picker('subtle', (m) => picked = m)));
      await tester.tap(find.byKey(const ValueKey('deck-motion-dynamic')));
      expect(picked, 'dynamic');
      final sel =
          tester.getSemantics(find.bySemanticsLabel('Subtle, Gentle fades'));
      expect(isSel(sel), isTrue);
      expect(sel.getSemanticsData().hasAction(SemanticsAction.tap), isTrue);
      handle.dispose();
    });

    testWidgets('RTL puts the radio at the right', (tester) async {
      size(tester, 500, 600);
      await tester
          .pumpWidget(host(picker('subtle', (_) {}), dir: TextDirection.rtl));
      final icon = tester.getCenter(find.byIcon(Icons.radio_button_checked));
      final label = tester.getCenter(find.text('Subtle'));
      expect(icon.dx, greaterThan(label.dx));
    });
  });

  group('DeckPurposePicker', () {
    testWidgets('lists name + description; tap selects', (tester) async {
      size(tester, 500, 600);
      String? picked;
      await tester.pumpWidget(host(DeckPurposePicker(
        purposes: purposes,
        selected: 'pitch_deck',
        onSelected: (p) => picked = p,
        semanticsLabel: 'Purpose',
      )));
      expect(find.text('Pitch deck'), findsOneWidget);
      expect(find.text('Raise a round.'), findsOneWidget);
      expect(find.byIcon(Icons.check_circle), findsOneWidget);
      await tester
          .tap(find.byKey(const ValueKey('deck-purpose-project_report')));
      expect(picked, 'project_report');
    });

    testWidgets('semantics: selected + label with description', (tester) async {
      final handle = tester.ensureSemantics();
      size(tester, 500, 600);
      await tester.pumpWidget(host(DeckPurposePicker(
          purposes: purposes, selected: 'pitch_deck', onSelected: (_) {})));
      final n = tester
          .getSemantics(find.bySemanticsLabel('Pitch deck, Raise a round.'));
      expect(isSel(n), isTrue);
      handle.dispose();
    });

    testWidgets('RTL and large text', (tester) async {
      size(tester, 320, 600);
      await tester.pumpWidget(host(
          DeckPurposePicker(
              purposes: purposes, selected: null, onSelected: (_) {}),
          dir: TextDirection.rtl,
          textScale: 2));
      expect(tester.takeException(), isNull);
    });
  });
}
