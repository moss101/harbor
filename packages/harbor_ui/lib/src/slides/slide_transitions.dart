// Transition rendering between two slides (owner: Presenter agent).
//
// [TransitionView] paints the outgoing slide [from] and the incoming slide
// [to] at a given progress (0..1). Both are ordinary [SlideView]s held in
// repaint boundaries, so a frame only moves/clips/fades two cached layers.
//
// Every authored transition name maps to one of ten renderers
// ([TransitionEffect]); anything unknown falls back to a cross-fade and
// nothing here can throw on a name from a newer producer. See
// [resolveTransition] for the table. Notable approximations:
//
//  * `morph` (PowerPoint's object-matching transition) is a cross-fade with a
//    slight scale (outgoing grows 3 %, incoming settles from 97 %). Harbor
//    does not match shapes between slides.
//  * Exotic 3D/particle kinds (`vortex`, `prism`, `honeycomb`, `flip`,
//    `ferris`, ...) are mapped to the closest planar move or reveal.
//
// `dir` follows OOXML: it is the direction of travel. `push` with `l` moves
// the content leftwards, the incoming slide entering from the right edge.
// Default is `l`. `cover` slides the incoming slide over the old one,
// `uncover` slides the old one away, `pull` is `uncover` with the incoming
// slide drifting up slowly underneath. `wipe` moves a hard edge in the
// direction of travel. `split`: `horz`/`vert` choose the bar orientation,
// `in` closes from the edges, anything else opens from the centre. `zoom`:
// `out` enlarges the old slide away, anything else zooms the new slide in.
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import 'slide_player.dart' show PlayState;
import 'slide_scene.dart';
import 'slide_view.dart';

enum TransitionEffect {
  fade,
  fadeThroughBlack,
  push,
  wipe,
  cover,
  uncover,
  pull,
  split,
  zoom,
  morph,
}

/// The renderer chosen for an authored transition and the direction it uses.
class ResolvedTransition {
  const ResolvedTransition(this.effect, this.dir);
  final TransitionEffect effect;
  final String? dir;

  @override
  bool operator ==(Object other) =>
      other is ResolvedTransition && other.effect == effect && other.dir == dir;

  @override
  int get hashCode => Object.hash(effect, dir);
}

/// Maps an authored transition [kind] (+ [dir]) to the closest renderer.
/// Never fails: unknown kinds become a fade.
ResolvedTransition resolveTransition(String kind, String? dir) {
  ResolvedTransition r(TransitionEffect e, [String? d]) =>
      ResolvedTransition(e, d ?? dir);
  switch (kind) {
    case 'fade':
      return r(TransitionEffect.fade);
    case 'fade_through_black':
      return r(TransitionEffect.fadeThroughBlack);
    case 'push':
      return r(TransitionEffect.push);
    case 'wipe':
      return r(TransitionEffect.wipe);
    case 'cover':
      return r(TransitionEffect.cover);
    case 'uncover':
      return r(TransitionEffect.uncover);
    case 'pull':
      return r(TransitionEffect.pull);
    case 'split':
      return r(TransitionEffect.split);
    case 'zoom':
      return r(TransitionEffect.zoom);
    case 'morph':
      return r(TransitionEffect.morph);
    // Closest-match approximations.
    case 'ripple':
    case 'vortex':
    case 'warp':
    case 'flythrough':
      return r(TransitionEffect.zoom);
    case 'reveal':
      return r(TransitionEffect.uncover);
    case 'flip':
    case 'prism':
    case 'ferris':
    case 'gallery':
    case 'conveyor':
    case 'pan':
      return r(TransitionEffect.push);
    case 'doors':
      return r(TransitionEffect.split, 'vert');
    case 'window':
      return r(TransitionEffect.split, 'in');
    case 'shred':
      return r(TransitionEffect.wipe);
    case 'flash':
      return r(TransitionEffect.fadeThroughBlack);
    case 'glitter':
    case 'honeycomb':
    case 'random':
    default:
      return r(TransitionEffect.fade);
  }
}

/// Unit vector of travel for an OOXML direction (default: leftwards).
Offset transitionTravel(String? dir) {
  switch (dir) {
    case 'r':
      return const Offset(1, 0);
    case 'u':
      return const Offset(0, -1);
    case 'd':
      return const Offset(0, 1);
    case 'lu':
      return const Offset(-1, -1);
    case 'ru':
      return const Offset(1, -1);
    case 'ld':
      return const Offset(-1, 1);
    case 'rd':
      return const Offset(1, 1);
    default:
      return const Offset(-1, 0);
  }
}

/// The part of a [size] rect on the trailing side of a wipe front that has
/// travelled [p] (0..1) along [travel]: at 0 nothing, at 1 everything.
Path wipeRevealPath(Size size, Offset travel, double p) {
  final len = travel.distance;
  final v = len == 0 ? const Offset(-1, 0) : travel / len;
  final corners = [
    Offset.zero,
    Offset(size.width, 0),
    Offset(size.width, size.height),
    Offset(0, size.height),
  ];
  double proj(Offset o) => o.dx * v.dx + o.dy * v.dy;
  final ps = corners.map(proj).toList();
  final pmin = ps.reduce(math.min);
  final pmax = ps.reduce(math.max);
  final f = pmin + (pmax - pmin) * p.clamp(0.0, 1.0);
  final out = <Offset>[];
  for (var i = 0; i < 4; i++) {
    final a = corners[i];
    final b = corners[(i + 1) % 4];
    final pa = ps[i];
    final pb = ps[(i + 1) % 4];
    final ain = pa <= f;
    final bin = pb <= f;
    if (ain) out.add(a);
    if (ain != bin) {
      final t = (f - pa) / (pb - pa);
      out.add(Offset.lerp(a, b, t)!);
    }
  }
  final path = Path();
  if (out.length >= 3) path.addPolygon(out, true);
  return path;
}

/// Reveal region of a `split`: bars of the given orientation opening from the
/// centre (`inward == false`) or closing in from the edges.
Path splitRevealPath(Size size,
    {required bool vertical, required bool inward, required double p}) {
  final q = p.clamp(0.0, 1.0);
  final path = Path();
  if (vertical) {
    final w = size.width;
    if (inward) {
      path
        ..addRect(Rect.fromLTWH(0, 0, w * q / 2, size.height))
        ..addRect(Rect.fromLTWH(w - w * q / 2, 0, w * q / 2, size.height));
    } else {
      path.addRect(Rect.fromLTWH(w / 2 - w * q / 2, 0, w * q, size.height));
    }
  } else {
    final h = size.height;
    if (inward) {
      path
        ..addRect(Rect.fromLTWH(0, 0, size.width, h * q / 2))
        ..addRect(Rect.fromLTWH(0, h - h * q / 2, size.width, h * q / 2));
    } else {
      path.addRect(Rect.fromLTWH(0, h / 2 - h * q / 2, size.width, h * q));
    }
  }
  return path;
}

class _PathClipper extends CustomClipper<Path> {
  _PathClipper(this.build, this.key);
  final Path Function(Size) build;
  final Object key;

  @override
  Path getClip(Size size) => build(size);

  @override
  bool shouldReclip(covariant _PathClipper old) => old.key != key;
}

/// Paints [from] -> [to] at [progress] (0 = [from], 1 = [to]).
///
/// [fromState] / [toState] optionally apply a [PlayState] to each slide while
/// it transitions (the presenter passes "everything entrance-animated is
/// still hidden" for the incoming slide).
class TransitionView extends StatefulWidget {
  const TransitionView({
    super.key,
    required this.from,
    required this.to,
    required this.deckWidth,
    required this.deckHeight,
    required this.transition,
    required this.progress,
    this.fromState = PlayState.empty,
    this.toState = PlayState.empty,
  });

  final SlideScene from;
  final SlideScene to;
  final double deckWidth;
  final double deckHeight;

  /// The incoming slide's authored transition; null behaves as a fade.
  final SceneTransition? transition;

  /// 0..1.
  final double progress;
  final PlayState fromState;
  final PlayState toState;

  @override
  State<TransitionView> createState() => _TransitionViewState();
}

class _TransitionViewState extends State<TransitionView> {
  // The two slide layers are rebuilt only when their inputs change, so a
  // progress tick re-uses the very same widgets and their painted layers.
  Object? _key;
  Widget? _fromLayer;
  Widget? _toLayer;

  Widget _slide(SlideScene s, PlayState st, Size size) => RepaintBoundary(
        child: SizedBox(
          width: size.width,
          height: size.height,
          child: SlideView(
            scene: s,
            deckWidth: widget.deckWidth,
            deckHeight: widget.deckHeight,
            effects: st.effects,
            hidden: st.hidden,
            hiddenParas: st.hiddenParas,
          ),
        ),
      );

  void _ensureLayers(Size size) {
    final key = (
      size,
      widget.from,
      widget.to,
      widget.fromState,
      widget.toState,
      widget.deckWidth,
      widget.deckHeight,
    );
    if (_key == key) return;
    _key = key;
    _fromLayer = _slide(widget.from, widget.fromState, size);
    _toLayer = _slide(widget.to, widget.toState, size);
  }

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: widget.deckWidth / widget.deckHeight,
      child: LayoutBuilder(builder: (context, c) {
        final size = c.biggest;
        _ensureLayers(size);
        final p = widget.progress.clamp(0.0, 1.0);
        final from = _fromLayer!;
        final to = _toLayer!;
        final Widget body;
        if (p <= 0) {
          body = from;
        } else if (p >= 1) {
          body = to;
        } else {
          final spec = resolveTransition(
              widget.transition?.kind ?? 'fade', widget.transition?.dir);
          body = ClipRect(
            child: ColoredBox(
              color: const Color(0xFF000000),
              child: Stack(
                fit: StackFit.expand,
                children: _layers(spec, p, size, from, to),
              ),
            ),
          );
        }
        return ExcludeSemantics(child: body);
      }),
    );
  }

  static Widget _opacity(double o, Widget child) =>
      o >= 1 ? child : Opacity(opacity: o.clamp(0.0, 1.0), child: child);

  static Widget _move(Offset o, Widget child) =>
      Transform.translate(offset: o, child: child);

  List<Widget> _layers(
    ResolvedTransition spec,
    double p,
    Size size,
    Widget from,
    Widget to,
  ) {
    final v = transitionTravel(spec.dir);
    final travel = Offset(v.dx * size.width, v.dy * size.height);
    switch (spec.effect) {
      case TransitionEffect.fade:
        return [from, _opacity(p, to)];
      case TransitionEffect.fadeThroughBlack:
        return p < 0.5
            ? [_opacity(1 - 2 * p, from)]
            : [_opacity(2 * p - 1, to)];
      case TransitionEffect.push:
        return [
          _move(travel * p, from),
          _move(-travel * (1 - p), to),
        ];
      case TransitionEffect.cover:
        return [from, _move(-travel * (1 - p), to)];
      case TransitionEffect.uncover:
        return [to, _move(travel * p, from)];
      case TransitionEffect.pull:
        return [
          _move(-travel * (1 - p) * 0.3, to),
          _move(travel * p, from),
        ];
      case TransitionEffect.wipe:
        final tv = v;
        return [
          from,
          ClipPath(
            clipBehavior: Clip.hardEdge,
            clipper: _PathClipper((s) => wipeRevealPath(s, tv, p), p),
            child: to,
          ),
        ];
      case TransitionEffect.split:
        final vertical = spec.dir == 'vert';
        final inward = spec.dir == 'in';
        return [
          from,
          ClipPath(
            clipBehavior: Clip.hardEdge,
            clipper: _PathClipper(
                (s) => splitRevealPath(s,
                    vertical: vertical, inward: inward, p: p),
                p),
            child: to,
          ),
        ];
      case TransitionEffect.zoom:
        if (spec.dir == 'out') {
          return [
            to,
            _opacity(
              1 - p,
              Transform.scale(scale: 1 + 0.6 * p, child: from),
            ),
          ];
        }
        return [
          from,
          _opacity(
            math.min(1, p * 2),
            Transform.scale(scale: 0.4 + 0.6 * p, child: to),
          ),
        ];
      case TransitionEffect.morph:
        return [
          Transform.scale(scale: 1 + 0.03 * p, child: from),
          _opacity(p, Transform.scale(scale: 0.97 + 0.03 * p, child: to)),
        ];
    }
  }
}
