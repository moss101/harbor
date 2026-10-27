// Animation timeline engine (owner: Presenter agent).
//
// Pure Dart, deterministic, no widgets: [SlidePlayer] turns a slide's authored
// animation list into per-instant [PlayState]s that a [SlideView] paints.
//
// ## Steps
//
// Animations are in play order. Each starts a *group*:
//
// * group 0, the **auto group**: animations before the first `click` trigger
//   (`with_prev` / `after_prev`). They run on their own as soon as the slide
//   has appeared. Group 0 may be empty.
// * group k (1..[SlidePlayer.steps]): one `click` animation plus the
//   `with_prev` / `after_prev` animations that follow it, until the next
//   `click`. Each waits for the presenter.
//
// [SlidePlayer.steps] is the number of click groups. `stateAt(step, t)` is
// cumulative: every group before [step] is complete, group [step] is [t] ms
// in, later groups have not begun. `step == -1` is "before anything": every
// entrance target is hidden and the auto group has not started. `step == 0`
// at `t == 0` is the same picture unless group 0 has an animation with zero
// delay, whose first frame is already in progress.
//
// ## Timing inside a group
//
// PowerPoint's model: `click` and `with_prev` start at the start of the
// current "par" plus their own delay; `after_prev` opens a new par that starts
// when everything so far in the group has finished. start = parBase + delay,
// end = start + dur.
//
// ## Visibility
//
// A shape whose first visibility-changing animation (entrance/exit, whole
// shape) is an *entrance* is hidden before it; one whose first is an *exit* is
// visible before it. A finished entrance leaves it visible, a finished exit
// hides it. Emphasis never changes visibility and leaves nothing behind.
//
// Build-by-paragraph (`para` range) is tracked per paragraph and reported in
// [PlayState.hiddenParas]; the shape itself stays painted. [ShapeEffect] is
// per shape, so a paragraph that is mid-animation is simply shown (appearing
// at its start; an exiting one disappears at its end). No per-paragraph fade.
//
// ## Effects (see [ShapeEffect]; `dir` is the side the shape comes from /
// leaves to: `l r u d lu ru ld rd`)
//
//  * fade                 opacity
//  * fly                  translate in from beyond the slide edge + fade
//  * float                gentle rise (5 % of slide height) + fade
//  * wipe                 wipe fraction from [dir]
//  * zoom / grow          scale 0.5 -> 1 + fade (emphasis `grow`: breathing scale)
//  * appear               instant
//  * pulse / spin (emph.) transient scale; ShapeEffect has no rotation, so a
//                         spin is approximated by the same breathing scale
//  * split / wheel / other / unknown names -> fade
//
// Easing is ease-out cubic.
import 'dart:math' as math;

import 'slide_scene.dart';
import 'slide_view.dart' show ShapeEffect;

/// What to paint at one instant: per-shape effects, shapes not painted at all
/// and paragraphs not painted. Equivalent to the matching [SlideView]
/// parameters.
class PlayState {
  const PlayState({
    this.effects = const {},
    this.hidden = const {},
    this.hiddenParas = const {},
  });

  /// Nothing animated, nothing hidden: the slide exactly as authored.
  static const PlayState empty = PlayState();

  final Map<int, ShapeEffect> effects;
  final Set<int> hidden;
  final Map<int, Set<int>> hiddenParas;

  bool get isEmpty => effects.isEmpty && hidden.isEmpty && hiddenParas.isEmpty;
}

/// Ease-out cubic.
double slideEaseOut(double p) {
  final q = 1 - p.clamp(0.0, 1.0);
  return 1 - q * q * q;
}

class _Timed {
  _Timed(this.anim, this.group, this.start);
  final SceneAnimation anim;
  final int group;
  final int start;
  int get dur => anim.effect == 'appear' ? 0 : math.max(0, anim.durMs);
  int get end => start + dur;
}

class SlidePlayer {
  /// [deckWidth] / [deckHeight] (EMU) size the fly-in distance; they default
  /// to a 16:9 deck.
  SlidePlayer(
    this.scene, {
    this.deckWidth = 12192000,
    this.deckHeight = 6858000,
  }) {
    _build();
  }

  final SlideScene scene;
  final double deckWidth, deckHeight;

  final List<_Timed> _timed = [];
  int _steps = 0;
  final List<int> _groupEnd = [0];

  // Initial visibility, decided from the first entrance/exit on each target.
  final Set<int> _initiallyHiddenShapes = {};
  final Map<int, Set<int>> _initiallyHiddenParas = {};

  /// Number of click-triggered steps (0 = nothing to click).
  int get steps => _steps;

  /// Whether the auto group (step 0) contains any animation.
  bool get hasAutoPlay => _timed.any((t) => t.group == 0);

  /// Whether the slide has any animation at all.
  bool get hasAnimations => _timed.isNotEmpty;

  /// Duration of group [step] in ms (0 for an empty group or out of range).
  int durationMs(int step) => step < 0 || step > _steps ? 0 : _groupEnd[step];

  /// Sum of every group's duration (what a non-interactive preview needs).
  int get totalMs => _groupEnd.fold(0, (a, b) => a + b);

  void _build() {
    var group = 0;
    var parBase = 0; // start of the current par within the group
    var groupMax = 0; // latest end so far within the group
    for (final a in scene.animations) {
      if (a.trigger == 'click') {
        group++;
        parBase = 0;
        groupMax = 0;
        _groupEnd.add(0);
      } else if (a.trigger == 'after_prev') {
        parBase = groupMax;
      } // with_prev (and anything unknown): same par as the previous step
      final t = _Timed(a, group, parBase + math.max(0, a.delayMs));
      _timed.add(t);
      groupMax = math.max(groupMax, t.end);
      _groupEnd[group] = groupMax;
    }
    _steps = group;

    // Initial visibility from the first visibility-changing animation.
    final seenShape = <int>{};
    final seenPara = <int, Set<int>>{};
    for (final t in _timed) {
      final a = t.anim;
      if (a.phase != 'entrance' && a.phase != 'exit') continue;
      final range = _paraRange(a);
      if (range == null) {
        if (seenShape.add(a.shapeId) && a.phase == 'entrance') {
          _initiallyHiddenShapes.add(a.shapeId);
        }
      } else {
        final seen = seenPara.putIfAbsent(a.shapeId, () => {});
        for (var p = range.$1; p <= range.$2; p++) {
          if (seen.add(p) && a.phase == 'entrance') {
            _initiallyHiddenParas.putIfAbsent(a.shapeId, () => {}).add(p);
          }
        }
      }
    }
  }

  static (int, int)? _paraRange(SceneAnimation a) {
    final p = a.para;
    if (p == null || p.length < 2) return null;
    final lo = math.min(p[0], p[1]);
    final hi = math.max(p[0], p[1]);
    // Guard against absurd ranges from a hostile file.
    return (lo, math.min(hi, lo + 4096));
  }

  /// The state [tMs] ms into [step]. `step == -1` is before anything;
  /// steps above [steps] clamp to the end.
  PlayState stateAt(int step, int tMs) {
    if (_timed.isEmpty) return PlayState.empty;
    final cur = step > _steps ? _steps : step;
    if (step > _steps) tMs = 1 << 30; // beyond the last step: its end state
    final shapeVisible = <int, bool>{
      for (final id in _initiallyHiddenShapes) id: false,
    };
    final paraVisible = <int, Map<int, bool>>{
      for (final e in _initiallyHiddenParas.entries)
        e.key: {for (final p in e.value) p: false},
    };
    final effects = <int, ShapeEffect>{};

    for (final t in _timed) {
      final a = t.anim;
      // Progress: <0 not started, in [0,1) running, >=1 done.
      double p;
      if (cur < 0 || t.group > cur) {
        p = -1;
      } else if (t.group < cur) {
        p = 1;
      } else {
        final el = tMs - t.start;
        if (el < 0) {
          p = -1;
        } else if (t.dur == 0 || el >= t.dur) {
          p = 1;
        } else {
          p = el / t.dur;
        }
      }
      if (p < 0) continue;
      final running = p < 1;
      final range = _paraRange(a);

      if (range != null) {
        if (a.phase == 'emphasis') continue;
        final map = paraVisible.putIfAbsent(a.shapeId, () => {});
        for (var i = range.$1; i <= range.$2; i++) {
          if (a.phase == 'entrance') {
            map[i] = true; // revealed at its start
          } else {
            map[i] = running; // still shown while it exits
          }
        }
        continue;
      }

      switch (a.phase) {
        case 'entrance':
          shapeVisible[a.shapeId] = true;
          if (running) _addEffect(effects, a, slideEaseOut(p), entering: true);
        case 'exit':
          if (running) {
            shapeVisible[a.shapeId] = true;
            _addEffect(effects, a, slideEaseOut(p), entering: false);
          } else {
            shapeVisible[a.shapeId] = false;
          }
        default: // emphasis
          if (running) _addEmphasis(effects, a, p);
      }
    }

    final hidden = <int>{
      for (final e in shapeVisible.entries)
        if (!e.value) e.key,
    };
    final hiddenParas = <int, Set<int>>{};
    paraVisible.forEach((id, m) {
      final hs = {
        for (final e in m.entries)
          if (!e.value) e.key,
      };
      if (hs.isNotEmpty) hiddenParas[id] = hs;
    });
    // Hidden shapes carry no effect.
    for (final id in hidden) {
      effects.remove(id);
    }
    return PlayState(
      effects: effects,
      hidden: hidden,
      hiddenParas: hiddenParas,
    );
  }

  // ---------------------------------------------------------------------
  // Effect maths

  /// (dx, dy) in EMU from which [dir] flies in: the shape fully beyond the
  /// slide edge on that side.
  (double, double) _edgeOffset(SceneAnimation a, String? dir) {
    final item = scene.itemById(a.shapeId);
    final x = item?.x ?? deckWidth * 0.4;
    final y = item?.y ?? deckHeight * 0.4;
    final w = item?.w ?? deckWidth * 0.2;
    final h = item?.h ?? deckHeight * 0.2;
    var dx = 0.0, dy = 0.0;
    final d = dir ?? 'd';
    if (d.contains('l')) dx = -(x + w);
    if (d.contains('r')) dx = deckWidth - x;
    if (d.contains('u')) dy = -(y + h);
    if (d.contains('d')) dy = deckHeight - y;
    if (dx == 0 && dy == 0) dy = deckHeight - y; // unknown dir: from below
    return (dx, dy);
  }

  (double, double) _floatOffset(String? dir) {
    final s = deckHeight * 0.05;
    final d = dir ?? 'd';
    var dx = 0.0, dy = 0.0;
    if (d.contains('l')) dx = -s;
    if (d.contains('r')) dx = s;
    if (d.contains('u')) dy = -s;
    if (d.contains('d')) dy = s;
    if (dx == 0 && dy == 0) dy = s;
    return (dx, dy);
  }

  void _addEffect(
    Map<int, ShapeEffect> out,
    SceneAnimation a,
    double e, {
    required bool entering,
  }) {
    // [k] is how "shown" the shape is: 0 = fully away, 1 = at rest.
    final k = entering ? e : 1 - e;
    ShapeEffect fx;
    switch (a.effect) {
      case 'appear':
        fx = const ShapeEffect();
      case 'fly':
        final (ox, oy) = _edgeOffset(a, a.dir);
        fx = ShapeEffect(opacity: k, dx: ox * (1 - k), dy: oy * (1 - k));
      case 'float':
        final (ox, oy) = _floatOffset(a.dir);
        fx = ShapeEffect(opacity: k, dx: ox * (1 - k), dy: oy * (1 - k));
      case 'wipe':
        fx = ShapeEffect(wipe: k, wipeDir: a.dir ?? 'd');
      case 'zoom':
      case 'grow':
        fx = ShapeEffect(opacity: k, scale: 0.5 + 0.5 * k);
      default: // fade, split, wheel, spin, pulse, other, unknown
        fx = ShapeEffect(opacity: k);
    }
    _merge(out, a.shapeId, fx);
  }

  void _addEmphasis(Map<int, ShapeEffect> out, SceneAnimation a, double p) {
    final bump = math.sin(math.pi * p); // 0 -> 1 -> 0
    ShapeEffect fx;
    switch (a.effect) {
      case 'grow':
        fx = ShapeEffect(scale: 1 + 0.25 * bump);
      case 'pulse':
      case 'spin':
        fx = ShapeEffect(scale: 1 + 0.08 * bump);
      case 'appear':
        fx = const ShapeEffect();
      default: // unknown -> fade: dip to 40 % opacity and back
        fx = ShapeEffect(opacity: 1 - 0.6 * bump);
    }
    _merge(out, a.shapeId, fx);
  }

  static void _merge(Map<int, ShapeEffect> out, int id, ShapeEffect fx) {
    final prev = out[id];
    if (prev == null) {
      out[id] = fx;
      return;
    }
    out[id] = ShapeEffect(
      opacity: prev.opacity * fx.opacity,
      dx: prev.dx + fx.dx,
      dy: prev.dy + fx.dy,
      scale: prev.scale * fx.scale,
      wipe: fx.wipe ?? prev.wipe,
      wipeDir: fx.wipe != null ? fx.wipeDir : prev.wipeDir,
    );
  }
}
