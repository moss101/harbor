import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/slides.dart';

SceneAnimation anim(
  int id, {
  String effect = 'fade',
  String phase = 'entrance',
  String trigger = 'click',
  int delay = 0,
  int dur = 500,
  String? dir,
  List<int>? para,
}) =>
    SceneAnimation(
      shapeId: id,
      para: para,
      effect: effect,
      phase: phase,
      dir: dir,
      trigger: trigger,
      delayMs: delay,
      durMs: dur,
    );

SlidePlayer player(List<SceneAnimation> a) =>
    SlidePlayer(SlideScene(index: 1, animations: a));

const big = 1 << 30;

void main() {
  group('empty', () {
    test('no animations: no steps, nothing hidden', () {
      final p = player(const []);
      expect(p.steps, 0);
      expect(p.hasAutoPlay, isFalse);
      expect(p.durationMs(0), 0);
      for (final s in [-1, 0, 5]) {
        final st = p.stateAt(s, 100);
        expect(st.hidden, isEmpty);
        expect(st.effects, isEmpty);
        expect(st.hiddenParas, isEmpty);
      }
    });
  });

  group('click grouping', () {
    test('click starts a step; with/after_prev join it', () {
      final p = player([
        anim(1),
        anim(2, trigger: 'with_prev'),
        anim(3),
        anim(4, trigger: 'after_prev'),
        anim(5, trigger: 'with_prev'),
      ]);
      expect(p.steps, 2);
      expect(p.hasAutoPlay, isFalse);
      // Before anything every entrance target is hidden.
      expect(p.stateAt(-1, 0).hidden, {1, 2, 3, 4, 5});
      // After step 1: 1 and 2 shown, 3..5 still hidden.
      expect(p.stateAt(1, big).hidden, {3, 4, 5});
      expect(p.stateAt(2, big).hidden, isEmpty);
    });

    test('animations before the first click are the auto group (step 0)', () {
      final p = player([
        anim(1, trigger: 'with_prev'),
        anim(2, trigger: 'after_prev'),
        anim(3),
      ]);
      expect(p.steps, 1);
      expect(p.hasAutoPlay, isTrue);
      expect(p.stateAt(-1, 0).hidden, {1, 2, 3});
      // Auto group: 1 at 0..500, 2 after it at 500..1000.
      expect(p.durationMs(0), 1000);
      expect(p.stateAt(0, 250).hidden, {2, 3});
      expect(p.stateAt(0, 600).hidden, {3});
      expect(p.stateAt(0, big).hidden, {3});
      expect(p.stateAt(1, big).hidden, isEmpty);
    });
  });

  group('timing', () {
    test('delays, with_prev and after_prev chain as in PowerPoint', () {
      final p = player([
        anim(1, delay: 100, dur: 500), // 100..600
        anim(2, trigger: 'with_prev', delay: 200, dur: 300), // 200..500
        anim(3, trigger: 'after_prev', delay: 50, dur: 100), // 650..750
      ]);
      expect(p.durationMs(1), 750);
      expect(p.stateAt(1, 99).effects[1]?.opacity ?? 0, 0);
      expect(p.stateAt(1, 99).hidden, {1, 2, 3});
      expect(p.stateAt(1, 100).hidden, {2, 3}); // 1 begun
      expect(p.stateAt(1, 200).hidden, {3});
      expect(p.stateAt(1, 649).hidden, {3});
      expect(p.stateAt(1, 650).hidden, isEmpty);
      expect(p.stateAt(1, 750).effects, isEmpty);
    });

    test('totalMs sums all groups', () {
      final p = player([
        anim(1, trigger: 'with_prev', dur: 300),
        anim(2, dur: 400),
        anim(3, dur: 500),
      ]);
      expect(p.durationMs(0), 300);
      expect(p.totalMs, 300 + 400 + 500);
    });

    test('steps beyond the end clamp', () {
      final p = player([anim(1)]);
      expect(p.stateAt(9, big).hidden, isEmpty);
      expect(p.stateAt(9, big).effects, isEmpty);
    });
  });

  group('entrance / exit / emphasis', () {
    test('entrance: hidden before, easing in, fully visible after', () {
      final p = player([anim(1, dur: 1000)]);
      expect(p.stateAt(-1, 0).hidden, {1});
      final mid = p.stateAt(1, 500);
      expect(mid.hidden, isEmpty);
      // Ease-out cubic: 1 - (1 - .5)^3.
      expect(mid.effects[1]!.opacity, closeTo(0.875, 1e-9));
      final done = p.stateAt(1, 1000);
      expect(done.hidden, isEmpty);
      expect(done.effects, isEmpty);
    });

    test('exit: visible before, fading, hidden after', () {
      final p = player([anim(1, phase: 'exit', dur: 1000)]);
      expect(p.stateAt(-1, 0).hidden, isEmpty);
      final mid = p.stateAt(1, 500);
      expect(mid.hidden, isEmpty);
      expect(mid.effects[1]!.opacity, closeTo(0.125, 1e-9));
      expect(p.stateAt(1, 1000).hidden, {1});
      expect(p.stateAt(1000, 0).hidden, {1});
    });

    test('exit after an entrance of the same shape', () {
      final p = player([
        anim(1, dur: 100),
        anim(1, phase: 'exit', dur: 100),
      ]);
      expect(p.stateAt(-1, 0).hidden, {1});
      expect(p.stateAt(1, big).hidden, isEmpty);
      expect(p.stateAt(2, big).hidden, {1});
    });

    test('emphasis is transient and never hides', () {
      final p = player([anim(1, phase: 'emphasis', effect: 'pulse')]);
      expect(p.stateAt(-1, 0).hidden, isEmpty);
      final mid = p.stateAt(1, 250);
      expect(mid.hidden, isEmpty);
      expect(mid.effects[1]!.scale, greaterThan(1));
      expect(p.stateAt(1, 500).effects, isEmpty);
    });

    test('appear is instant', () {
      final p = player([anim(1, effect: 'appear', dur: 900)]);
      expect(p.stateAt(1, 0).hidden, isEmpty);
      expect(p.stateAt(1, 0).effects, isEmpty);
      expect(p.durationMs(1), 0);
    });
  });

  group('effects', () {
    test('fly comes in from the named side', () {
      final p = player([anim(1, effect: 'fly', dir: 'l', dur: 1000)]);
      final start = p.stateAt(1, 0).effects[1]!;
      expect(start.dx, lessThan(0));
      expect(start.dy, 0);
      expect(start.opacity, 0);
      final late = p.stateAt(1, 900).effects[1]!;
      expect(late.dx.abs(), lessThan(start.dx.abs()));
      final up = player([anim(1, effect: 'fly', dir: 'u')]);
      expect(up.stateAt(1, 0).effects[1]!.dy, lessThan(0));
    });

    test('float rises gently (smaller than fly)', () {
      final f = player([anim(1, effect: 'float', dir: 'd')]).stateAt(1, 0);
      final y = f.effects[1]!.dy;
      expect(y, greaterThan(0));
      expect(y, lessThan(6858000 * 0.1));
    });

    test('wipe reports a fraction and direction', () {
      final p = player([anim(1, effect: 'wipe', dir: 'r', dur: 1000)]);
      final fx = p.stateAt(1, 500).effects[1]!;
      expect(fx.wipe, closeTo(0.875, 1e-9));
      expect(fx.wipeDir, 'r');
    });

    test('zoom and grow scale up with fade', () {
      for (final e in ['zoom', 'grow']) {
        final fx = player([anim(1, effect: e)]).stateAt(1, 0).effects[1]!;
        expect(fx.scale, closeTo(0.5, 1e-9));
        expect(fx.opacity, 0);
      }
    });

    test('unknown, split, wheel and other effects fall back to fade', () {
      for (final e in ['sparkle', 'split', 'wheel', 'other', '']) {
        final fx = player([anim(1, effect: e)]).stateAt(1, 250).effects[1]!;
        expect(fx.opacity, inExclusiveRange(0, 1), reason: e);
        expect(fx.scale, 1, reason: e);
        expect(fx.dx, 0, reason: e);
        expect(fx.dy, 0, reason: e);
        expect(fx.wipe, isNull, reason: e);
      }
    });

    test('overlapping animations on one shape combine', () {
      final p = player([
        anim(1, dur: 1000),
        anim(1,
            trigger: 'with_prev', phase: 'emphasis', effect: 'grow', dur: 1000),
      ]);
      final fx = p.stateAt(1, 500).effects[1]!;
      expect(fx.opacity, closeTo(0.875, 1e-9));
      expect(fx.scale, greaterThan(1));
    });
  });

  group('paragraph build', () {
    test('reveals paragraph by paragraph; the shape stays painted', () {
      final p = player([
        anim(5, para: [0, 0]),
        anim(5, para: [1, 1]),
        anim(5, para: [2, 3]),
      ]);
      expect(p.steps, 3);
      expect(p.stateAt(-1, 0).hidden, isEmpty);
      expect(p.stateAt(-1, 0).hiddenParas[5], {0, 1, 2, 3});
      expect(p.stateAt(1, big).hiddenParas[5], {1, 2, 3});
      expect(p.stateAt(2, big).hiddenParas[5], {2, 3});
      expect(p.stateAt(3, 0).hiddenParas[5], isNull);
      expect(p.stateAt(3, big).hiddenParas, isEmpty);
    });

    test('a paragraph exit hides it at the end', () {
      final p = player([
        anim(5, phase: 'exit', para: [1, 1], dur: 200)
      ]);
      expect(p.stateAt(-1, 0).hiddenParas, isEmpty);
      expect(p.stateAt(1, 100).hiddenParas, isEmpty);
      expect(p.stateAt(1, 200).hiddenParas[5], {1});
    });
  });
}
