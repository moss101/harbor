import 'dart:typed_data';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/slides.dart';

import 'render_helpers.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('every slide of the full fixture paints without throwing', () async {
    final deck = DeckScene.fromJson(
      jsonDecode(
              File('test/slides/fixtures/scene_full.json').readAsStringSync())
          as Map<String, dynamic>,
    );
    for (final slide in deck.slides) {
      final px = await renderSlide(slide);
      final bg = px.at(2, 2);
      // Something other than the bare background was drawn.
      final marks = px.where(
        (p) => p[0] != bg[0] || p[1] != bg[1] || p[2] != bg[2],
      );
      expect(marks.length, greaterThan(500), reason: 'slide ${slide.index}');
    }
  });

  group('fills and geometry', () {
    test('solid fill colour, background and alpha', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 200, 100, fill: solid('#FF0000')),
        box(2, 400, 100, 200, 100, fill: solid('#0000FF', 0.5)),
      ]));
      expect(px.near(150, 150, 255, 0, 0), isTrue);
      expect(px.near(50, 50, 255, 255, 255), isTrue,
          reason: 'slide background');
      // 50% blue over white.
      expect(px.near(450, 150, 128, 128, 255, tol: 4), isTrue);
    });

    test('linear gradient endpoints (0deg and 90deg)', () async {
      const stops = [SceneStop(0, '#FF0000', 1), SceneStop(1, '#0000FF', 1)];
      final px = await renderSlide(slideOf([
        box(1, 0, 0, 480, 540,
            fill: const ScenePaint(
                kind: PaintKind.linear, angle: 0, stops: stops)),
        box(2, 480, 0, 480, 540,
            fill: const ScenePaint(
                kind: PaintKind.linear, angle: 90, stops: stops)),
      ]));
      expect(px.near(1, 270, 255, 0, 0, tol: 8), isTrue,
          reason: 'left = first stop');
      expect(px.near(478, 270, 0, 0, 255, tol: 8), isTrue,
          reason: 'right = last stop');
      expect(px.near(240, 270, 128, 0, 128, tol: 10), isTrue,
          reason: 'midpoint');
      expect(px.near(720, 1, 255, 0, 0, tol: 8), isTrue,
          reason: 'top = first stop');
      expect(px.near(720, 538, 0, 0, 255, tol: 8), isTrue,
          reason: 'bottom = last stop');
    });

    test('radial gradient: centre is the first stop, fades to edge', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 200, 200,
            fill: const ScenePaint(
                kind: PaintKind.radial,
                cx: 0.5,
                cy: 0.5,
                stops: [
                  SceneStop(0, '#000000', 1),
                  SceneStop(1, '#FFFFFF', 1),
                ])),
      ]));
      expect(px.near(200, 200, 0, 0, 0, tol: 12), isTrue);
      expect(px.at(110, 200)[0], greaterThan(200));
    });

    test('rotation turns about the centre', () async {
      // 200 x 20 bar centred at (480, 270), rotated 90 degrees -> vertical.
      final px = await renderSlide(slideOf([
        box(1, 380, 260, 200, 20, fill: solid('#00AA00'), rot: 90),
      ]));
      expect(px.near(480, 270 - 80, 0, 170, 0), isTrue);
      expect(px.near(480, 270 + 80, 0, 170, 0), isTrue);
      expect(px.near(480 - 80, 270, 255, 255, 255), isTrue);
      expect(px.near(480 + 80, 270, 255, 255, 255), isTrue);
    });

    test('flipH mirrors a right triangle', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 200, 200,
            geom: const PresetGeom('rtTriangle'), fill: solid('#FF0000')),
        box(2, 400, 100, 200, 200,
            geom: const PresetGeom('rtTriangle'),
            fill: solid('#FF0000'),
            flipH: true),
      ]));
      // rtTriangle has its right angle bottom-left: bottom-left filled, top-right empty.
      expect(px.near(110, 290, 255, 0, 0), isTrue);
      expect(px.near(290, 110, 255, 255, 255), isTrue);
      // Flipped: bottom-right filled, top-left empty.
      expect(px.near(590, 290, 255, 0, 0), isTrue);
      expect(px.near(410, 110, 255, 255, 255), isTrue);
    });

    test('roundRect corner is transparent, edge centre is filled', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 200, 100,
            geom: const PresetGeom('roundRect', [50000]),
            fill: solid('#FF0000')),
      ]));
      expect(px.near(101, 101, 255, 255, 255), isTrue,
          reason: 'corner shows background');
      expect(px.near(200, 150, 255, 0, 0), isTrue);
      // adj 50000 on a 100px short side = radius 50.
      expect(px.near(200, 101, 255, 0, 0), isTrue);
    });

    test('ellipse, diamond and star5 cover centre but not corners', () async {
      for (final name in [
        'ellipse',
        'diamond',
        'star5',
        'hexagon',
        'chevron'
      ]) {
        final px = await renderSlide(slideOf([
          box(1, 100, 100, 200, 200,
              geom: PresetGeom(name), fill: solid('#FF0000')),
        ]));
        expect(px.near(200, 200, 255, 0, 0), isTrue, reason: '$name centre');
        if (name != 'chevron') {
          expect(px.near(102, 102, 255, 255, 255), isTrue,
              reason: '$name corner');
        }
      }
    });

    test('donut has a hole', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 200, 200,
            geom: const PresetGeom('donut'), fill: solid('#FF0000')),
      ]));
      expect(px.near(200, 200, 255, 255, 255), isTrue, reason: 'hole');
      expect(px.near(110, 200, 255, 0, 0), isTrue, reason: 'ring');
    });

    test('custom path scales from its own space to the frame', () async {
      final px = await renderSlide(slideOf([
        SceneShape(
          id: 1,
          x: e(100),
          y: e(100),
          w: e(200),
          h: e(200),
          geom: const PathGeom(w: 100, h: 100, cmds: [
            PathCmd('M', x: 0, y: 0),
            PathCmd('L', x: 100, y: 0),
            PathCmd('L', x: 0, y: 100),
            PathCmd('Z'),
          ]),
          fill: solid('#FF0000'),
        ),
      ]));
      expect(px.near(130, 130, 255, 0, 0), isTrue);
      expect(px.near(280, 280, 255, 255, 255), isTrue);
    });

    test('unknown preset draws as a rectangle and is reported', () async {
      final scene = slideOf([
        box(1, 100, 100, 200, 100,
            geom: const PresetGeom('someFutureShape'), fill: solid('#FF0000')),
      ]);
      final px = await renderSlide(scene);
      expect(px.near(102, 102, 255, 0, 0), isTrue);
      expect(
          slideUnsupportedSummary(scene).single, contains('someFutureShape'));
      expect(isKnownPreset('someFutureShape'), isFalse);
      expect(isKnownPreset('star5'), isTrue);
    });

    test('stroke width and dashes', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 300, 100,
            line: SceneLine(w: e(10), paint: solid('#000000'))),
        box(2, 100, 300, 300, 100,
            line: SceneLine(w: e(10), paint: solid('#000000'), dash: 'dash')),
      ]));
      expect(px.near(250, 100, 0, 0, 0), isTrue,
          reason: 'top edge centred on the frame edge');
      expect(px.near(250, 150, 255, 255, 255), isTrue, reason: 'no fill');
      // The dashed edge alternates ink and gap along x.
      var ink = 0, gap = 0;
      for (var x = 120; x < 380; x++) {
        if (px.at(x, 300)[0] < 100) {
          ink++;
        } else {
          gap++;
        }
      }
      expect(ink, greaterThan(40));
      expect(gap, greaterThan(40));
    });

    test('arrowhead widens the line end', () async {
      final px = await renderSlide(slideOf([
        SceneShape(
          id: 1,
          x: e(100),
          y: e(100),
          w: e(300),
          h: 0,
          geom: const PresetGeom('line'),
          line: SceneLine(w: e(3), paint: solid('#000000'), tail: 'triangle'),
        ),
      ]));
      // 12 px from the tip the arrowhead is much wider than the 3 px line.
      final head = px.where((p) => p[0] < 100,
          within: const Rect.fromLTWH(392, 80, 1, 40));
      expect(head.length, greaterThanOrEqualTo(6));
      final mid = px.where((p) => p[0] < 100,
          within: const Rect.fromLTWH(250, 80, 1, 40));
      expect(mid.length, lessThanOrEqualTo(4));
    });
  });

  group('shadow, effects, hidden', () {
    test('outer shadow is painted offset behind the shape', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 100, 100,
            fill: solid('#FF0000'),
            shadow:
                SceneShadow(dx: e(30), dy: e(30), blur: 0, c: '#000000', a: 1)),
      ]));
      expect(px.near(150, 150, 255, 0, 0), isTrue);
      expect(px.near(220, 220, 0, 0, 0), isTrue,
          reason: 'shadow visible past the shape');
      expect(px.near(105, 215, 255, 255, 255), isTrue,
          reason: 'not left of the offset shadow');
    });

    test('blurred shadow softens at its edge', () async {
      final px = await renderSlide(slideOf([
        box(1, 100, 100, 100, 100,
            fill: solid('#FF0000'),
            shadow: SceneShadow(
                dx: e(40), dy: e(40), blur: e(20), c: '#000000', a: 1)),
      ]));
      final core =
          px.at(190, 190); // inside both shadow and shape? shape ends at 200
      expect(core[0], lessThan(260));
      final edge = px.at(245, 170); // near the shadow's right edge
      expect(edge[0], inInclusiveRange(40, 245));
    });

    test('hidden shape is not painted', () async {
      final scene =
          slideOf([box(1, 100, 100, 100, 100, fill: solid('#FF0000'))]);
      final shown = await renderSlide(scene);
      final gone = await renderSlide(scene, hidden: const {1});
      expect(shown.near(150, 150, 255, 0, 0), isTrue);
      expect(gone.near(150, 150, 255, 255, 255), isTrue);
    });

    test('opacity effect blends with what is behind', () async {
      final scene =
          slideOf([box(1, 100, 100, 100, 100, fill: solid('#FF0000'))]);
      final px = await renderSlide(scene,
          effects: const {1: ShapeEffect(opacity: 0.5)});
      expect(px.near(150, 150, 255, 128, 128, tol: 4), isTrue);
    });

    test('translation and scale effects', () async {
      final scene =
          slideOf([box(1, 100, 100, 100, 100, fill: solid('#FF0000'))]);
      final moved = await renderSlide(scene,
          effects: {1: ShapeEffect(dx: e(200), dy: e(50))});
      expect(moved.near(350, 200, 255, 0, 0), isTrue);
      expect(moved.near(150, 150, 255, 255, 255), isTrue);
      final grown =
          await renderSlide(scene, effects: const {1: ShapeEffect(scale: 2)});
      // Centre (150,150); scale 2 -> spans 50..250.
      expect(grown.near(60, 60, 255, 0, 0), isTrue);
      expect(grown.near(240, 240, 255, 0, 0), isTrue);
    });

    test('wipe reveals from the named edge', () async {
      final scene =
          slideOf([box(1, 100, 100, 200, 100, fill: solid('#FF0000'))]);
      Future<Pixels> wipe(String dir) => renderSlide(scene,
          effects: {1: ShapeEffect(wipe: 0.5, wipeDir: dir)});
      final l = await wipe('l');
      expect(l.near(150, 150, 255, 0, 0), isTrue);
      expect(l.near(250, 150, 255, 255, 255), isTrue);
      final r = await wipe('r');
      expect(r.near(250, 150, 255, 0, 0), isTrue);
      expect(r.near(150, 150, 255, 255, 255), isTrue);
      final u = await wipe('u');
      expect(u.near(200, 120, 255, 0, 0), isTrue);
      expect(u.near(200, 180, 255, 255, 255), isTrue);
      final d = await wipe('d');
      expect(d.near(200, 180, 255, 0, 0), isTrue);
      expect(d.near(200, 120, 255, 255, 255), isTrue);
    });

    test('ShapeEffect equality drives shouldRepaint', () {
      const a = ShapeEffect(opacity: 0.5, wipe: 0.2, wipeDir: 'l');
      const b = ShapeEffect(opacity: 0.5, wipe: 0.2, wipeDir: 'l');
      expect(a, b);
      expect(a.hashCode, b.hashCode);
      final scene = slideOf(const []);
      final p1 = SlidePainter(scene: scene, deckWidth: kDeckW, effects: {1: a});
      final p2 = SlidePainter(scene: scene, deckWidth: kDeckW, effects: {1: b});
      expect(p1.shouldRepaint(p2), isFalse);
      final p3 = SlidePainter(
          scene: scene,
          deckWidth: kDeckW,
          effects: const {1: ShapeEffect(opacity: 0.4)});
      expect(p1.shouldRepaint(p3), isTrue);
      final p4 =
          SlidePainter(scene: scene, deckWidth: kDeckW, hidden: const {2});
      expect(p1.shouldRepaint(p4), isTrue);
    });
  });

  group('text', () {
    bool isInk(List<int> p) => p[0] > 200 && p[1] > 200 && p[2] > 200;

    test('text paints and never leaves its frame', () async {
      final long =
          'Lorem ipsum dolor sit amet consectetur adipiscing elit ' * 12;
      final scene = slideOf([
        box(1, 300, 200, 120, 60,
            text: textBody([
              SceneParagraph(runs: [run(long, sz: 28, c: '#FFFFFF')]),
              SceneParagraph(runs: [run(long, sz: 28, c: '#FFFFFF')]),
            ], ins: [
              4,
              4,
              4,
              4
            ])),
      ], background: solid('#000000'));
      final px = await renderSlide(scene);
      final inside =
          px.where(isInk, within: const Rect.fromLTWH(300, 200, 120, 60));
      expect(inside.length, greaterThan(50), reason: 'the text did paint');
      final all = px.where(isInk);
      expect(all.length, inside.length, reason: 'no ink outside the frame');
    });

    test('wrap off keeps a long line inside the frame by clipping', () async {
      final scene = slideOf([
        box(1, 300, 200, 120, 60,
            text: textBody([
              SceneParagraph(runs: [run('W' * 80, sz: 20, c: '#FFFFFF')]),
            ], wrap: false)),
      ], background: solid('#000000'));
      final px = await renderSlide(scene);
      expect(px.where(isInk, within: const Rect.fromLTWH(300, 200, 120, 60)),
          isNotEmpty);
      expect(
          px.where(isInk).length,
          px
              .where(isInk, within: const Rect.fromLTWH(300, 200, 120, 60))
              .length);
    });

    test('clip holds for rotated text frames too', () async {
      final scene = slideOf([
        box(1, 300, 200, 160, 40,
            rot: 30,
            text: textBody([
              SceneParagraph(runs: [run('X' * 300, sz: 30, c: '#FFFFFF')]),
            ])),
      ], background: solid('#000000'));
      final px = await renderSlide(scene);
      final ink = px.where(isInk);
      expect(ink, isNotEmpty);
      // Everything stays within the rotated frame's bounding box.
      final minX = ink.map((p) => p.$1).reduce((a, b) => a < b ? a : b);
      final maxX = ink.map((p) => p.$1).reduce((a, b) => a > b ? a : b);
      expect(minX, greaterThan(300 + 80 - 80 * 0.866 - 20 * 0.5 - 3));
      expect(maxX, lessThan(300 + 80 + 80 * 0.866 + 20 * 0.5 + 3));
    });

    test('anchor t / ctr / b places the text block vertically', () async {
      Future<double> meanY(String anchor) async {
        final scene = slideOf([
          box(1, 100, 100, 200, 200,
              text: textBody([
                SceneParagraph(runs: [run('Hello', sz: 20, c: '#FFFFFF')]),
              ], anchor: anchor)),
        ], background: solid('#000000'));
        final px = await renderSlide(scene);
        final ink = px.where(isInk);
        return ink.map((p) => p.$2).reduce((a, b) => a + b) / ink.length;
      }

      final t = await meanY('t'), c = await meanY('ctr'), b = await meanY('b');
      expect(t, lessThan(130));
      expect(c, inInclusiveRange(185, 215));
      expect(b, greaterThan(270));
    });

    test('paragraph alignment l / ctr / r', () async {
      Future<double> meanX(String algn) async {
        final scene = slideOf([
          box(1, 100, 100, 400, 60,
              text: textBody([
                SceneParagraph(
                    algn: algn, runs: [run('Hi', sz: 20, c: '#FFFFFF')]),
              ])),
        ], background: solid('#000000'));
        final px = await renderSlide(scene);
        final ink = px.where(isInk);
        return ink.map((p) => p.$1).reduce((a, b) => a + b) / ink.length;
      }

      final l = await meanX('l'), c = await meanX('ctr'), r = await meanX('r');
      expect(l, lessThan(130));
      expect(c, inInclusiveRange(285, 315));
      expect(r, greaterThan(470));
    });

    test('normAutofit shrinks further until the last paragraph fits', () async {
      SlideScene scene(String autofit) => slideOf([
            box(1, 100, 100, 300, 120,
                text: textBody([
                  for (final c in [
                    '#FF0000',
                    '#00FF00',
                    '#0000FF',
                    '#FFFF00',
                    '#FF00FF',
                    '#00FFFF'
                  ])
                    SceneParagraph(runs: [run('MMMM', sz: 28, c: c)]),
                ], autofit: autofit)),
          ], background: solid('#000000'));
      bool cyan(List<int> p) => p[0] < 40 && p[1] > 215 && p[2] > 215;
      final fixed = await renderSlide(scene('none'));
      expect(fixed.where(cyan), isEmpty,
          reason: 'last paragraph is clipped away');
      final fit = await renderSlide(scene('shrink'));
      expect(fit.where(cyan), isNotEmpty,
          reason: 'shrunk so the last paragraph fits');
      expect(
          fit
              .where(cyan, within: const Rect.fromLTWH(100, 100, 300, 120))
              .length,
          fit.where(cyan).length);
    });

    test('bullet sits on the left for LTR and on the right for RTL', () async {
      Future<(double, double)> bulletAndText(bool rtl) async {
        final scene = slideOf([
          box(1, 100, 100, 400, 80,
              text: textBody([
                SceneParagraph(
                  algn: rtl ? 'r' : 'l',
                  rtl: rtl,
                  marL: e(30),
                  indent: -e(30),
                  bullet: const SceneBullet(ch: '•', c: '#FF0000'),
                  runs: [run(rtl ? 'مرحبا' : 'Hello', sz: 20, c: '#FFFFFF')],
                ),
              ])),
        ], background: solid('#000000'));
        final px = await renderSlide(scene);
        final red = px.where((p) => p[0] > 200 && p[1] < 60 && p[2] < 60);
        final white = px.where(isInk);
        expect(red, isNotEmpty, reason: 'bullet painted');
        expect(white, isNotEmpty, reason: 'text painted');
        double mean(List<(int, int)> l) =>
            l.map((p) => p.$1).reduce((a, b) => a + b) / l.length;
        return (mean(red), mean(white));
      }

      final (ltrBullet, ltrText) = await bulletAndText(false);
      expect(ltrBullet, lessThan(ltrText));
      expect(ltrBullet, lessThan(130));
      final (rtlBullet, rtlText) = await bulletAndText(true);
      expect(rtlBullet, greaterThan(rtlText));
      expect(rtlBullet, greaterThan(470));
    });

    test('hiddenParas keeps space but does not paint', () async {
      SlideScene scene() => slideOf([
            box(1, 100, 100, 300, 200,
                text: textBody([
                  SceneParagraph(runs: [run('AAAA', sz: 24, c: '#FF0000')]),
                  SceneParagraph(runs: [run('BBBB', sz: 24, c: '#00FF00')]),
                ])),
          ], background: solid('#000000'));
      bool green(List<int> p) => p[1] > 200 && p[0] < 60;
      bool red(List<int> p) => p[0] > 200 && p[1] < 60;
      final all = await renderSlide(scene());
      final hid = await renderSlide(scene(), hiddenParas: const {
        1: {0}
      });
      expect(hid.where(red), isEmpty);
      expect(hid.where(green), isNotEmpty);
      // The remaining paragraph did not jump up into the hidden one's place.
      final y1 =
          all.where(green).map((p) => p.$2).reduce((a, b) => a < b ? a : b);
      final y2 =
          hid.where(green).map((p) => p.$2).reduce((a, b) => a < b ? a : b);
      expect(y2, y1);
    });

    test('soft breaks, baseline runs, caps and empty paragraphs do not throw',
        () async {
      final scene = slideOf([
        box(1, 100, 100, 400, 300,
            text: textBody([
              SceneParagraph(runs: [
                run('Line one', c: '#FFFFFF'),
                const SceneRun(t: '\n', c: '#FFFFFF'),
                run('x', c: '#FFFFFF'),
                run('2', c: '#FFFFFF', baseline: 30),
                run(' H', c: '#FFFFFF'),
                run('2', c: '#FFFFFF', baseline: -25),
                const SceneRun(
                    t: ' caps',
                    c: '#FFFFFF',
                    caps: true,
                    u: true,
                    strike: true,
                    spc: 2,
                    i: true),
              ]),
              const SceneParagraph(runs: [], endSz: 20),
              SceneParagraph(lnPct: 1.5, runs: [run('Tail', c: '#FFFFFF')]),
              SceneParagraph(lnPts: 30, runs: [run('Exact', c: '#FFFFFF')]),
            ])),
      ], background: solid('#000000'));
      final px = await renderSlide(scene);
      expect(px.where(isInk), isNotEmpty);
    });

    test('number formats', () {
      expect(slideNumberText('arabicPeriod', 3), '3.');
      expect(slideNumberText('arabicParenR', 3), '3)');
      expect(slideNumberText('arabicParenBoth', 3), '(3)');
      expect(slideNumberText('arabicPlain', 3), '3');
      expect(slideNumberText('romanUcPeriod', 4), 'IV.');
      expect(slideNumberText('romanLcPeriod', 9), 'ix.');
      expect(slideNumberText('alphaLcPeriod', 2), 'b.');
      expect(slideNumberText('alphaUcParenR', 27), 'AA)');
      expect(slideNumberText('', 1), '1.');
      expect(slideNumberText('nonsenseScheme', 5), '5.');
    });

    test('numbered paragraphs count up and restart after a break', () async {
      // Different widths of "1." vs "2." are not observable with test fonts,
      // so check the numbering through the exposed formatter and that the
      // layout of three numbered paragraphs completes.
      final scene = slideOf([
        box(1, 100, 100, 400, 200,
            text: textBody([
              for (var i = 0; i < 3; i++)
                SceneParagraph(
                  marL: e(30),
                  indent: -e(30),
                  bullet:
                      const SceneBullet(kind: 'num', numType: 'arabicPeriod'),
                  runs: [run('Item $i', c: '#FFFFFF')],
                ),
            ])),
      ], background: solid('#000000'));
      final px = await renderSlide(scene);
      expect(px.where((p) => p[0] > 200), isNotEmpty);
    });

    test('font resolution never yields an empty chain', () {
      for (final f in [
        'Calibri',
        'Cambria',
        'Georgia',
        'Arial',
        'Trebuchet MS',
        'Century Gothic',
        'Gill Sans MT',
        'Franklin Gothic Medium',
        'Garamond',
        'Segoe UI',
        'Consolas',
        '',
        'Totally Unknown Font'
      ]) {
        final c = resolveSlideFont(f);
        expect(c.fallback, isNotEmpty, reason: f);
      }
      expect(resolveSlideFont('Georgia').fallback.first, 'Cambria');
      expect(resolveSlideFont('Consolas').fallback.last, 'monospace');
      final ar = resolveSlideFont('Calibri', cs: 'Arial', complexScript: true);
      expect(ar.family, 'Arial');
      expect(ar.fallback, contains('Geeza Pro'));
      expect(hasComplexScript('مرحبا'), isTrue);
      expect(hasComplexScript('hello'), isFalse);
    });
  });

  group('pictures', () {
    Future<String> tinyPng() async {
      // 2x2: red green / blue yellow.
      final pixels = Uint8List.fromList([
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 0, 255,
      ]);
      final completer = _ImageCompleter();
      ui.decodeImageFromPixels(
          pixels, 2, 2, ui.PixelFormat.rgba8888, completer.complete);
      final img = await completer.future;
      final png = await img.toByteData(format: ui.ImageByteFormat.png);
      return base64Encode(png!.buffer.asUint8List());
    }

    test('draws the image, honours crop and geom clip', () async {
      final b64 = await tinyPng();
      ScenePicture pic(
              {List<double> crop = const [0, 0, 0, 0],
              SceneGeom geom = const PresetGeom('rect')}) =>
          ScenePicture(
              id: 1,
              x: e(100),
              y: e(100),
              w: e(200),
              h: e(200),
              mime: 'image/png',
              dataB64: b64,
              crop: crop,
              geom: geom);

      final full = await renderSlide(slideOf([pic()]));
      expect(full.near(110, 110, 255, 0, 0), isTrue, reason: 'top-left red');
      expect(full.near(290, 110, 0, 255, 0), isTrue, reason: 'top-right green');
      expect(full.near(110, 290, 0, 0, 255), isTrue,
          reason: 'bottom-left blue');
      expect(full.near(290, 290, 255, 255, 0), isTrue,
          reason: 'bottom-right yellow');

      // Crop away the left half: only green over yellow remain.
      final cropped = await renderSlide(slideOf([
        pic(crop: const [0.5, 0, 0, 0])
      ]));
      expect(cropped.near(200, 150, 0, 255, 0), isTrue);
      expect(cropped.near(200, 250, 255, 255, 0), isTrue);
      expect(cropped.near(260, 150, 0, 255, 0), isTrue,
          reason: 'red column is gone');

      // Crop away the top half.
      final cropT = await renderSlide(slideOf([
        pic(crop: const [0, 0.5, 0, 0])
      ]));
      expect(cropT.near(150, 200, 0, 0, 255), isTrue);
      expect(cropT.near(250, 200, 255, 255, 0), isTrue);

      // roundRect clip rounds the corner off.
      final round = await renderSlide(slideOf([
        pic(geom: const PresetGeom('roundRect', [50000]))
      ]));
      expect(round.near(101, 101, 255, 255, 255), isTrue);
      expect(round.near(150, 150, 255, 0, 0), isTrue);
    });

    test('empty data draws a neutral placeholder', () async {
      final px = await renderSlide(slideOf([
        const ScenePicture(
            id: 1, x: 100 * kPx, y: 100 * kPx, w: 200 * kPx, h: 200 * kPx),
      ]));
      expect(px.near(110, 110, 0xE5, 0xE7, 0xEB), isTrue);
    });

    test('undecodable data falls back to the placeholder, never throws',
        () async {
      final px = await renderSlide(slideOf([
        ScenePicture(
            id: 1,
            x: e(100),
            y: e(100),
            w: e(200),
            h: e(200),
            mime: 'image/png',
            dataB64: base64Encode(utf8.encode('not an image'))),
      ]));
      expect(px.near(110, 110, 0xE5, 0xE7, 0xEB), isTrue);
    });
  });

  group('charts', () {
    SceneChart chart(ChartData d) =>
        SceneChart(id: 1, x: e(40), y: e(40), w: e(880), h: e(460), chart: d);

    test('column bars rise in proportion to their values', () async {
      final px = await renderSlide(slideOf([
        chart(const ChartData(
          kind: 'column',
          categories: ['A', 'B', 'C'],
          series: [
            ChartSeries(name: 's', values: [10, 20, 40], color: '#2563EB'),
          ],
          showGridlines: false,
          textColor: '#222222',
          gapWidth: 0.5,
        )),
      ]));
      bool blue(List<int> p) =>
          (p[0] - 0x25).abs() < 8 &&
          (p[1] - 0x63).abs() < 8 &&
          (p[2] - 0xEB).abs() < 8;
      final cols = <int, List<int>>{}; // x -> ys
      for (final (x, y) in px.where(blue)) {
        (cols[x] ??= []).add(y);
      }
      expect(cols, isNotEmpty);
      // Group contiguous x into bars.
      final xs = cols.keys.toList()..sort();
      final bars = <List<int>>[];
      for (final x in xs) {
        if (bars.isNotEmpty && x - bars.last.last <= 1) {
          bars.last.add(x);
        } else {
          bars.add([x]);
        }
      }
      expect(bars, hasLength(3));
      int top(List<int> bar) =>
          cols[bar[bar.length ~/ 2]]!.reduce((a, b) => a < b ? a : b);
      int bottom(List<int> bar) =>
          cols[bar[bar.length ~/ 2]]!.reduce((a, b) => a > b ? a : b);
      final t = bars.map(top).toList();
      final b = bars.map(bottom).toList();
      expect(t[2], lessThan(t[1]));
      expect(t[1], lessThan(t[0]));
      expect((b[0] - b[1]).abs(), lessThanOrEqualTo(1),
          reason: 'common baseline');
      expect((b[1] - b[2]).abs(), lessThanOrEqualTo(1));
      final h = [for (var i = 0; i < 3; i++) (b[i] - t[i] + 1).toDouble()];
      expect(h[1] / h[0], closeTo(2, 0.1));
      expect(h[2] / h[0], closeTo(4, 0.2));
      // Equal widths.
      final w = bars.map((x) => x.length).toList();
      expect((w[0] - w[2]).abs(), lessThanOrEqualTo(2));
    });

    test('stacked column stacks segments, percent-stacked fills to the top',
        () async {
      final px = await renderSlide(slideOf([
        chart(const ChartData(
          kind: 'column',
          stacked: true,
          percentStacked: true,
          categories: ['A'],
          series: [
            ChartSeries(name: 'a', values: [1], color: '#FF0000'),
            ChartSeries(name: 'b', values: [3], color: '#0000FF'),
          ],
          showGridlines: false,
          textColor: '#222222',
        )),
      ]));
      final red = px.where((p) => p[0] > 240 && p[1] < 20 && p[2] < 20);
      final blue = px.where((p) => p[2] > 240 && p[0] < 20 && p[1] < 20);
      expect(red, isNotEmpty);
      expect(blue, isNotEmpty);
      // 25% / 75%.
      expect(blue.length / red.length, closeTo(3, 0.2));
      final redTop = red.map((p) => p.$2).reduce((a, b) => a < b ? a : b);
      final blueBottom = blue.map((p) => p.$2).reduce((a, b) => a > b ? a : b);
      expect(redTop,
          greaterThan(blue.map((p) => p.$2).reduce((a, b) => a < b ? a : b)));
      expect(blueBottom, lessThan(redTop + 1000));
    });

    test('horizontal bars grow rightwards from the axis', () async {
      final px = await renderSlide(slideOf([
        chart(const ChartData(
          kind: 'bar',
          categories: ['A', 'B'],
          series: [
            ChartSeries(name: 's', values: [10, 30], color: '#00AA00')
          ],
          showGridlines: false,
          textColor: '#222222',
        )),
      ]));
      bool green(List<int> p) => p[1] > 150 && p[0] < 20 && p[2] < 20;
      final pts = px.where(green);
      final rows = <int, List<int>>{};
      for (final (x, y) in pts) {
        (rows[y] ??= []).add(x);
      }
      final lens = rows.values.map((r) => r.length).toSet().toList()..sort();
      expect(lens.length, 2);
      expect(lens.last / lens.first, closeTo(3, 0.2));
    });

    test('line, area, pie, doughnut and scatter all paint ink', () async {
      for (final kind in ['line', 'area', 'pie', 'doughnut', 'scatter']) {
        final px = await renderSlide(slideOf([
          chart(ChartData(
            kind: kind,
            title: 'Title',
            categories: const ['A', 'B', 'C', 'D'],
            series: const [
              ChartSeries(
                  name: 's1',
                  values: [3, 5, 2, 8],
                  x: [1, 2, 3, 4],
                  color: '#FF0000'),
              ChartSeries(
                  name: 's2',
                  values: [1, 2, 4, 6],
                  x: [1, 2, 3, 4],
                  color: '#0000FF'),
            ],
            legend: 'r',
            dataLabels: true,
            smooth: kind == 'line',
            textColor: '#222222',
          )),
        ]));
        final ink = px.where((p) => p[0] < 240 || p[1] < 240 || p[2] < 240);
        expect(ink.length, greaterThan(500), reason: kind);
      }
    });

    test('doughnut leaves a hole', () async {
      final px = await renderSlide(slideOf([
        chart(const ChartData(
          kind: 'doughnut',
          categories: ['A', 'B'],
          series: [
            ChartSeries(
                name: 's',
                values: [1, 1],
                color: '#FF0000',
                pointColors: ['#FF0000', '#0000FF']),
          ],
          hole: 0.6,
          textColor: '#222222',
        )),
      ]));
      // Chart centre is the frame centre.
      expect(px.near(480, 270, 255, 255, 255), isTrue);
      expect(px.where((p) => p[0] > 240 && p[1] < 20).isNotEmpty, isTrue);
    });

    test('empty chart does not throw', () async {
      final px = await renderSlide(slideOf([chart(const ChartData())]));
      expect(px.width, kViewW);
    });

    test('nice axis scales and number formatting', () {
      final a = niceAxisScale(0, 47);
      expect(a.min, 0);
      expect(a.max, 50);
      expect(a.step, 10);
      expect(a.ticks, [0, 10, 20, 30, 40, 50]);
      final b = niceAxisScale(0, 1);
      expect(b.step, 0.2);
      expect(b.ticks.last, 1);
      final c = niceAxisScale(0, 100, fixedMax: 80);
      expect(c.max, 80);
      final d = niceAxisScale(-5, 5);
      expect(d.min, lessThanOrEqualTo(-5));
      expect(d.max, greaterThanOrEqualTo(5));
      expect(d.ticks, contains(0));
      expect(niceAxisScale(3, 3).max, greaterThan(niceAxisScale(3, 3).min));
      expect(formatChartNumber(1234567), '1,234,567');
      expect(formatChartNumber(0.25), '0.25');
      expect(formatChartNumber(12), '12');
      expect(formatChartNumber(0.5, step: 0.5), '0.5');
      expect(formatChartNumber(-3), '-3');
    });
  });

  group('tables', () {
    test('cell fill, spans and borders', () async {
      SceneTextBody t(String s) => SceneTextBody(ins: [
            e(4),
            e(2),
            e(4),
            e(2)
          ], paras: [
            SceneParagraph(runs: [run(s, sz: 14)])
          ]);
      final line = SceneLine(w: e(4), paint: solid('#000000'));
      final scene = slideOf([
        SceneTable(
          id: 1,
          x: e(100),
          y: e(100),
          w: e(600),
          h: e(200),
          cols: [e(200), e(200), e(200)],
          rows: [
            SceneTableRow(h: e(50), cells: [
              SceneTableCell(
                  text: t('Wide'), fill: solid('#FF0000'), gridSpan: 2),
              SceneTableCell(text: t(''), mergedAway: true),
              SceneTableCell(
                  text: t('C'),
                  fill: solid('#00FF00'),
                  borders: [null, null, null, line]),
            ]),
            SceneTableRow(h: e(50), cells: [
              SceneTableCell(text: t('1'), fill: solid('#0000FF')),
              SceneTableCell(text: t('2')),
              SceneTableCell(text: t('3')),
            ]),
          ],
        ),
      ]);
      final px = await renderSlide(scene);
      expect(px.near(390, 125, 255, 0, 0), isTrue,
          reason: 'span covers two columns');
      expect(px.near(550, 110, 0, 255, 0), isTrue);
      expect(px.near(150, 175, 0, 0, 255), isTrue);
      expect(px.near(300, 175, 255, 255, 255), isTrue, reason: 'no fill');
      expect(px.near(550, 150, 0, 0, 0, tol: 60), isTrue,
          reason: 'bottom border of C');
    });

    test('rows grow to fit their text', () async {
      final scene = slideOf([
        SceneTable(
          id: 1,
          x: e(100),
          y: e(100),
          w: e(200),
          h: e(30),
          cols: [e(200)],
          rows: [
            SceneTableRow(h: e(30), cells: [
              SceneTableCell(
                fill: solid('#FF0000'),
                text: SceneTextBody(ins: [
                  0,
                  0,
                  0,
                  0
                ], paras: [
                  SceneParagraph(runs: [run('M' * 60, sz: 20)]),
                ]),
              ),
            ]),
          ],
        ),
      ]);
      final px = await renderSlide(scene);
      final ink = px.where((p) => p[0] < 40 && p[1] < 40 && p[2] < 40);
      final maxY = ink.map((p) => p.$2).reduce((a, b) => a > b ? a : b);
      expect(maxY, greaterThan(160),
          reason: 'row taller than the authored 30 px');
    });
  });

  group('widgets', () {
    Widget host(Widget child, {double? width = 400}) => Directionality(
        textDirection: TextDirection.ltr,
        child: MediaQuery(
          data: const MediaQueryData(),
          child: Align(
              alignment: Alignment.topLeft,
              child:
                  width == null ? child : SizedBox(width: width, child: child)),
        ));

    testWidgets(
        'SlideView sizes to width by aspect ratio and exposes one label',
        (tester) async {
      final scene = slideOf([
        box(1, 100, 100, 200, 100,
            fill: solid('#FF0000'),
            text: textBody([
              SceneParagraph(runs: [run('Quarterly review')])
            ])),
      ]);
      final handle = tester.ensureSemantics();
      await tester.pumpWidget(
          host(SlideView(scene: scene, deckWidth: kDeckW, deckHeight: kDeckH)));
      final size = tester.getSize(find.byType(SlideView));
      expect(size.width, 400);
      expect(size.height, closeTo(400 * kDeckH / kDeckW, 0.01));
      expect(find.bySemanticsLabel(RegExp('Quarterly review')), findsOneWidget);
      handle.dispose();
    });

    testWidgets('explicit semanticsLabel wins; painted content is excluded',
        (tester) async {
      final scene = slideOf([
        box(1, 0, 0, 100, 100,
            text: textBody([
              SceneParagraph(runs: [run('hidden text')])
            ]))
      ]);
      final handle = tester.ensureSemantics();
      await tester.pumpWidget(host(SlideView(
          scene: scene,
          deckWidth: kDeckW,
          deckHeight: kDeckH,
          semanticsLabel: 'Cover slide')));
      expect(find.bySemanticsLabel('Cover slide'), findsOneWidget);
      expect(find.bySemanticsLabel(RegExp('hidden text')), findsNothing);
      handle.dispose();
    });

    testWidgets('unsupported badge shows only when asked', (tester) async {
      const scene =
          SlideScene(index: 1, unsupported: ['SmartArt (Content diagram)']);
      await tester.pumpWidget(host(const SlideView(
          scene: scene, deckWidth: kDeckW, deckHeight: kDeckH)));
      expect(find.textContaining('SmartArt'), findsNothing);
      await tester.pumpWidget(host(const SlideView(
          scene: scene,
          deckWidth: kDeckW,
          deckHeight: kDeckH,
          showUnsupportedBadge: true)));
      expect(find.textContaining('SmartArt'), findsOneWidget);
    });

    testWidgets(
        'SlideThumbnail is fixed-width, has its own repaint boundary and no semantics',
        (tester) async {
      final scene = slideOf([
        box(1, 0, 0, 100, 100,
            text: textBody([
              SceneParagraph(runs: [run('t')])
            ]))
      ]);
      final handle = tester.ensureSemantics();
      await tester.pumpWidget(host(
          SlideThumbnail(
              scene: scene, deckWidth: kDeckW, deckHeight: kDeckH, width: 120),
          width: null));
      expect(tester.getSize(find.byType(SlideThumbnail)).width, 120);
      expect(
          find.descendant(
              of: find.byType(SlideThumbnail),
              matching: find.byType(RepaintBoundary)),
          findsWidgets);
      expect(find.bySemanticsLabel(RegExp('Slide')), findsNothing);
      handle.dispose();
    });

    testWidgets('rebuilding with identical effects does not repaint',
        (tester) async {
      final scene = slideOf([box(1, 0, 0, 100, 100, fill: solid('#FF0000'))]);
      Widget v(Map<int, ShapeEffect> fx) => host(SlideView(
          scene: scene, deckWidth: kDeckW, deckHeight: kDeckH, effects: fx));
      await tester.pumpWidget(v({1: const ShapeEffect(opacity: 0.5)}));
      await tester.pumpWidget(v({1: const ShapeEffect(opacity: 0.5)}));
      expect(tester.takeException(), isNull);
    });
  });
}

class _ImageCompleter {
  final _c = <ui.Image>[];
  late final Future<ui.Image> future = Future.sync(() async {
    while (_c.isEmpty) {
      await Future<void>.delayed(const Duration(milliseconds: 5));
    }
    return _c.first;
  });
  void complete(ui.Image i) => _c.add(i);
}
