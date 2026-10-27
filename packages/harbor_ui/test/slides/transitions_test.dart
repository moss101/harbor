import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/slides.dart';

// 16:9 deck rendered at 320 x 180 logical pixels.
const w = 320, h = 180;

SlideScene solid(String hex) => SlideScene(
      index: 1,
      background: ScenePaint(kind: PaintKind.solid, c: hex, a: 1),
    );

final red = solid('#FF0000');
final blue = solid('#0000FF');

SceneTransition tr(String kind, [String? dir]) =>
    SceneTransition(kind: kind, dir: dir, durMs: 500);

class Shot {
  Shot(this.bytes);
  final List<int> bytes;
  (int, int, int) at(int x, int y) {
    final i = (y * w + x) * 4;
    return (bytes[i], bytes[i + 1], bytes[i + 2]);
  }

  bool isRed(int x, int y) {
    final (r, g, b) = at(x, y);
    return r > 240 && g < 15 && b < 15;
  }

  bool isBlue(int x, int y) {
    final (r, g, b) = at(x, y);
    return b > 240 && r < 15 && g < 15;
  }
}

Future<Shot> shoot(
  WidgetTester tester,
  SceneTransition? t,
  double progress, {
  SlideScene? from,
  SlideScene? to,
}) async {
  final key = GlobalKey();
  await tester.pumpWidget(Directionality(
    textDirection: TextDirection.ltr,
    child: Align(
      alignment: Alignment.topLeft,
      child: SizedBox(
        width: w.toDouble(),
        height: h.toDouble(),
        child: RepaintBoundary(
          key: key,
          child: TransitionView(
            from: from ?? red,
            to: to ?? blue,
            deckWidth: 16,
            deckHeight: 9,
            transition: t,
            progress: progress,
          ),
        ),
      ),
    ),
  ));
  final bytes = await tester.runAsync(() async {
    final b = tester.renderObject<RenderRepaintBoundary>(find.byKey(key));
    final ui.Image img = await b.toImage();
    final data = await img.toByteData();
    return data!.buffer.asUint8List().toList();
  });
  return Shot(bytes!);
}

void main() {
  const kinds = [
    'fade',
    'fade_through_black',
    'push',
    'wipe',
    'cover',
    'uncover',
    'pull',
    'split',
    'zoom',
    'morph',
    'ripple',
    'reveal',
    'flip',
    'vortex',
    'prism',
    'glitter',
    'honeycomb',
    'doors',
    'window',
    'ferris',
    'gallery',
    'conveyor',
    'pan',
    'warp',
    'flythrough',
    'flash',
    'shred',
    'random',
    'other',
    'a-kind-from-the-future',
  ];

  group('every kind', () {
    for (final kind in kinds) {
      testWidgets('$kind: 0 shows from, 1 shows to, middle differs',
          (tester) async {
        final a = await shoot(tester, tr(kind, 'l'), 0);
        expect(a.isRed(5, 5) && a.isRed(160, 90) && a.isRed(315, 175), isTrue,
            reason: 'progress 0');
        final c = await shoot(tester, tr(kind, 'l'), 1);
        expect(
            c.isBlue(5, 5) && c.isBlue(160, 90) && c.isBlue(315, 175), isTrue,
            reason: 'progress 1');
        final m = await shoot(tester, tr(kind, 'l'), 0.5);
        expect(m.bytes, isNot(a.bytes), reason: 'middle equals from');
        expect(m.bytes, isNot(c.bytes), reason: 'middle equals to');
      });
    }

    testWidgets('a null transition behaves as a fade', (tester) async {
      final m = await shoot(tester, null, 0.5);
      final (r, _, b) = m.at(160, 90);
      expect(r, inInclusiveRange(100, 160));
      expect(b, inInclusiveRange(100, 160));
    });
  });

  group('geometry', () {
    testWidgets('fade cross-fades', (tester) async {
      final m = await shoot(tester, tr('fade'), 0.5);
      final (r, g, b) = m.at(160, 90);
      expect(g, 0);
      expect(r, greaterThan(100));
      expect(b, greaterThan(100));
    });

    testWidgets('fade through black passes through black', (tester) async {
      final m = await shoot(tester, tr('fade_through_black'), 0.5);
      expect(m.at(160, 90), (0, 0, 0));
      final early = await shoot(tester, tr('fade_through_black'), 0.25);
      final (r, _, b) = early.at(160, 90);
      expect(r, inInclusiveRange(100, 160));
      expect(b, 0);
    });

    testWidgets('push l moves both slides left', (tester) async {
      final m = await shoot(tester, tr('push', 'l'), 0.5);
      expect(m.isRed(10, 90), isTrue);
      expect(m.isBlue(310, 90), isTrue);
      final n = await shoot(tester, tr('push', 'r'), 0.5);
      expect(n.isBlue(10, 90), isTrue);
      expect(n.isRed(310, 90), isTrue);
    });

    testWidgets('cover and uncover split the frame at the travelled edge',
        (tester) async {
      for (final kind in ['cover', 'uncover']) {
        final m = await shoot(tester, tr(kind, 'l'), 0.25);
        expect(m.isRed(200, 90), isTrue, reason: kind);
        expect(m.isBlue(300, 90), isTrue, reason: kind);
      }
    });

    testWidgets('wipe reveals from the far edge of travel', (tester) async {
      final l = await shoot(tester, tr('wipe', 'l'), 0.25);
      expect(l.isRed(200, 90), isTrue);
      expect(l.isBlue(300, 90), isTrue);
      final d = await shoot(tester, tr('wipe', 'd'), 0.5);
      expect(d.isBlue(160, 10), isTrue);
      expect(d.isRed(160, 170), isTrue);
    });

    testWidgets('split opens from the centre, or closes in', (tester) async {
      final out = await shoot(tester, tr('split', 'horz'), 0.5);
      expect(out.isRed(160, 10), isTrue);
      expect(out.isBlue(160, 90), isTrue);
      final inn = await shoot(tester, tr('split', 'in'), 0.5);
      expect(inn.isBlue(160, 10), isTrue);
      expect(inn.isRed(160, 90), isTrue);
      final vert = await shoot(tester, tr('split', 'vert'), 0.5);
      expect(vert.isRed(10, 90), isTrue);
      expect(vert.isBlue(160, 90), isTrue);
    });

    testWidgets('zoom grows the incoming slide from the centre',
        (tester) async {
      final m = await shoot(tester, tr('zoom', 'in'), 0.25);
      expect(m.isRed(3, 3), isTrue);
      final (_, _, b) = m.at(160, 90);
      expect(b, greaterThan(100));
    });

    testWidgets('morph is a cross-fade with a slight scale', (tester) async {
      final m = await shoot(tester, tr('morph'), 0.5);
      final (r, _, b) = m.at(160, 90);
      expect(r, greaterThan(100));
      expect(b, greaterThan(100));
    });
  });

  group('resolveTransition', () {
    test('known kinds map to themselves, unknown to fade', () {
      expect(resolveTransition('push', 'r'),
          const ResolvedTransition(TransitionEffect.push, 'r'));
      expect(resolveTransition('nope', null).effect, TransitionEffect.fade);
      expect(resolveTransition('other', null).effect, TransitionEffect.fade);
    });

    test('exotic kinds pick the closest renderer', () {
      expect(resolveTransition('doors', null),
          const ResolvedTransition(TransitionEffect.split, 'vert'));
      expect(resolveTransition('reveal', 'l').effect, TransitionEffect.uncover);
      expect(resolveTransition('ripple', null).effect, TransitionEffect.zoom);
      expect(resolveTransition('flash', null).effect,
          TransitionEffect.fadeThroughBlack);
      expect(resolveTransition('conveyor', 'l').effect, TransitionEffect.push);
    });

    test('wipe reveal path grows with progress', () {
      const size = Size(100, 50);
      final none = wipeRevealPath(size, const Offset(-1, 0), 0);
      final half = wipeRevealPath(size, const Offset(-1, 0), 0.5);
      expect(half.contains(const Offset(90, 25)), isTrue);
      expect(half.contains(const Offset(10, 25)), isFalse);
      expect(none.contains(const Offset(90, 25)), isFalse);
      final diag = wipeRevealPath(size, const Offset(-1, 1), 0.5);
      expect(diag.getBounds().isEmpty, isFalse);
    });
  });
}
