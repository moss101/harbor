// Preset + custom path geometry (owner: SlideView agent).
//
// Turns a [SceneGeom] and the frame [Rect] it fills into `dart:ui` paths.
// Preset names are DrawingML names; adjust values arrive in OOXML units
// (1/100000 of the short side for most, 1/60000 degree for angles).
import 'dart:math' as math;
import 'dart:ui';

import 'slide_scene.dart';

/// The fill and stroke outlines of one geometry. They differ only for
/// open shapes (`arc`: the fill is a pie wedge, the stroke the bare arc).
class GeomPaths {
  const GeomPaths(this.fill, this.stroke);
  final Path fill;
  final Path stroke;
}

/// Where an open line starts/ends and which way it points there. [startDir]
/// and [endDir] point *outwards* (away from the line), which is the way an
/// arrowhead faces.
class LineEnds {
  const LineEnds(this.start, this.startDir, this.end, this.endDir);
  final Offset start, startDir, end, endDir;
}

const Set<String> _knownPresets = {
  'rect',
  'flowChartProcess',
  'roundRect',
  'flowChartAlternateProcess',
  'flowChartTerminator',
  'ellipse',
  'flowChartConnector',
  'line',
  'straightConnector1',
  'bentConnector2',
  'bentConnector3',
  'curvedConnector3',
  'triangle',
  'rtTriangle',
  'diamond',
  'flowChartDecision',
  'parallelogram',
  'trapezoid',
  'chevron',
  'homePlate',
  'pentagon',
  'hexagon',
  'octagon',
  'plus',
  'rightArrow',
  'leftArrow',
  'upArrow',
  'downArrow',
  'leftRightArrow',
  'star4',
  'star5',
  'star6',
  'star8',
  'arc',
  'pie',
  'blockArc',
  'donut',
};

/// Whether [name] is a preset this painter draws faithfully. Anything else
/// is drawn as a rectangle and reported as unsupported.
bool isKnownPreset(String name) => _knownPresets.contains(name);

const Set<String> _openPresets = {
  'line',
  'straightConnector1',
  'bentConnector2',
  'bentConnector3',
  'curvedConnector3',
  'arc',
};

/// True for geometries that are lines (no interior to fill).
bool isLineGeometry(SceneGeom g) {
  if (g is PresetGeom) return _openPresets.contains(g.name);
  if (g is PathGeom) {
    return g.cmds.isNotEmpty && !g.cmds.any((c) => c.op == 'Z');
  }
  return false;
}

/// The path to fill/stroke for [g] inside [r].
Path geometryPath(SceneGeom g, Rect r) => geometryPaths(g, r).fill;

GeomPaths geometryPaths(SceneGeom g, Rect r) {
  if (g is PathGeom) {
    final p = _customPath(g, r);
    return GeomPaths(p, p);
  }
  final preset = g as PresetGeom;
  if (preset.name == 'arc') return _arc(preset.adj, r);
  final p = _preset(preset.name, preset.adj, r);
  return GeomPaths(p, p);
}

/// The text box inside [r] for geometry [g] (ellipses and rounded shapes
/// inscribe their text so it does not touch the curves).
Rect geometryTextRect(SceneGeom g, Rect r) {
  if (g is! PresetGeom) return r;
  switch (g.name) {
    case 'ellipse':
    case 'flowChartConnector':
      final dx = r.width * 0.14645, dy = r.height * 0.14645;
      return Rect.fromLTRB(
          r.left + dx, r.top + dy, r.right - dx, r.bottom - dy);
    case 'roundRect':
    case 'flowChartAlternateProcess':
      final inset = _roundRadius(g.adj, r, g.name) * 0.29289;
      return r.deflate(inset);
    case 'flowChartTerminator':
      final inset = r.shortestSide / 2 * 0.29289;
      return r.deflate(inset);
    case 'diamond':
    case 'flowChartDecision':
      return Rect.fromLTRB(r.left + r.width / 4, r.top + r.height / 4,
          r.right - r.width / 4, r.bottom - r.height / 4);
    case 'triangle':
      return Rect.fromLTRB(r.left + r.width / 4, r.top + r.height / 2,
          r.right - r.width / 4, r.bottom);
  }
  return r;
}

/// Endpoints and outward tangents of an open geometry, for arrowheads.
LineEnds? geometryLineEnds(SceneGeom g, Rect r) {
  if (!isLineGeometry(g)) return null;
  final path = geometryPaths(g, r).stroke;
  final metrics = path.computeMetrics().toList();
  if (metrics.isEmpty) return null;
  final first = metrics.first, last = metrics.last;
  if (first.length <= 0 && last.length <= 0) return null;
  final a = first.getTangentForOffset(0);
  final b = last.getTangentForOffset(last.length);
  if (a == null || b == null) return null;
  return LineEnds(a.position, -a.vector, b.position, b.vector);
}

// ---------------------------------------------------------------------------
// Dashes

/// Dash pattern (on/off lengths in px) for an OOXML preset dash at stroke
/// width [w] px. Null for solid.
List<double>? dashPattern(String dash, double w) {
  final u = math.max(w, 1.0);
  switch (dash) {
    case 'dash':
      return [4 * u, 3 * u];
    case 'dot':
      return [1 * u, 3 * u];
    case 'dashDot':
      return [4 * u, 3 * u, 1 * u, 3 * u];
    case 'lgDash':
      return [8 * u, 3 * u];
    case 'lgDashDot':
      return [8 * u, 3 * u, 1 * u, 3 * u];
    case 'sysDash':
      return [3 * u, 1 * u];
    case 'sysDot':
      return [1 * u, 1 * u];
  }
  return null;
}

/// Splits [source] into dashes following [pattern] (alternating on/off).
Path dashPath(Path source, List<double> pattern) {
  final out = Path();
  if (pattern.isEmpty || pattern.any((v) => !(v > 0))) return source;
  for (final m in source.computeMetrics()) {
    var d = 0.0;
    var i = 0;
    var on = true;
    while (d < m.length) {
      final len = pattern[i % pattern.length];
      final end = math.min(d + len, m.length);
      if (on) out.addPath(m.extractPath(d, end), Offset.zero);
      d = end;
      i++;
      on = !on;
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// Custom path

Path _customPath(PathGeom g, Rect r) {
  final path = Path();
  final sx = g.w > 0 ? r.width / g.w : 1.0;
  final sy = g.h > 0 ? r.height / g.h : 1.0;
  double x(double v) => r.left + v * sx;
  double y(double v) => r.top + v * sy;
  for (final c in g.cmds) {
    switch (c.op) {
      case 'M':
        path.moveTo(x(c.x), y(c.y));
      case 'L':
        path.lineTo(x(c.x), y(c.y));
      case 'C':
        path.cubicTo(x(c.x1), y(c.y1), x(c.x2), y(c.y2), x(c.x), y(c.y));
      case 'Q':
        path.quadraticBezierTo(x(c.x1), y(c.y1), x(c.x), y(c.y));
      case 'Z':
        path.close();
    }
  }
  return path;
}

// ---------------------------------------------------------------------------
// Presets

double _adj(List<double> a, int i, double def) => i < a.length ? a[i] : def;

double _roundRadius(List<double> adj, Rect r, [String name = 'roundRect']) {
  final a = _adj(adj, 0, 16667).clamp(0.0, 50000.0);
  return a / 100000 * r.shortestSide;
}

Path _poly(List<Offset> pts) {
  final p = Path()..moveTo(pts.first.dx, pts.first.dy);
  for (var i = 1; i < pts.length; i++) {
    p.lineTo(pts[i].dx, pts[i].dy);
  }
  return p..close();
}

Path _preset(String name, List<double> adj, Rect r) {
  final l = r.left, t = r.top, rt = r.right, b = r.bottom;
  final w = r.width, h = r.height;
  final hc = r.center.dx, vc = r.center.dy;
  final ss = r.shortestSide;
  switch (name) {
    case 'rect':
    case 'flowChartProcess':
      return Path()..addRect(r);
    case 'roundRect':
    case 'flowChartAlternateProcess':
      final rad = _roundRadius(adj, r, name);
      return Path()..addRRect(RRect.fromRectXY(r, rad, rad));
    case 'flowChartTerminator':
      final rad = ss / 2;
      return Path()..addRRect(RRect.fromRectXY(r, rad, rad));
    case 'ellipse':
    case 'flowChartConnector':
      return Path()..addOval(r);
    case 'line':
    case 'straightConnector1':
      return Path()
        ..moveTo(l, t)
        ..lineTo(rt, b);
    case 'bentConnector2':
      return Path()
        ..moveTo(l, t)
        ..lineTo(rt, t)
        ..lineTo(rt, b);
    case 'bentConnector3':
      final x1 = l + w * _adj(adj, 0, 50000) / 100000;
      return Path()
        ..moveTo(l, t)
        ..lineTo(x1, t)
        ..lineTo(x1, b)
        ..lineTo(rt, b);
    case 'curvedConnector3':
      final x1 = l + w * _adj(adj, 0, 50000) / 100000;
      return Path()
        ..moveTo(l, t)
        ..cubicTo(x1, t, x1, b, rt, b);
    case 'triangle':
      final a = _adj(adj, 0, 50000).clamp(0.0, 100000.0);
      return _poly(
          [Offset(l, b), Offset(l + w * a / 100000, t), Offset(rt, b)]);
    case 'rtTriangle':
      return _poly([Offset(l, t), Offset(l, b), Offset(rt, b)]);
    case 'diamond':
    case 'flowChartDecision':
      return _poly(
          [Offset(hc, t), Offset(rt, vc), Offset(hc, b), Offset(l, vc)]);
    case 'parallelogram':
      final maxA = ss == 0 ? 0.0 : 100000 * w / ss;
      final a = _adj(adj, 0, 25000).clamp(0.0, maxA);
      final x2 = ss * a / 100000;
      return _poly(
          [Offset(l, b), Offset(l + x2, t), Offset(rt, t), Offset(rt - x2, b)]);
    case 'trapezoid':
      final maxA = ss == 0 ? 0.0 : 50000 * w / ss;
      final a = _adj(adj, 0, 25000).clamp(0.0, maxA);
      final x2 = ss * a / 100000;
      return _poly(
          [Offset(l, b), Offset(l + x2, t), Offset(rt - x2, t), Offset(rt, b)]);
    case 'chevron':
      final maxA = ss == 0 ? 0.0 : 100000 * w / ss;
      final a = _adj(adj, 0, 50000).clamp(0.0, maxA);
      final x1 = ss * a / 100000;
      return _poly([
        Offset(l, t),
        Offset(rt - x1, t),
        Offset(rt, vc),
        Offset(rt - x1, b),
        Offset(l, b),
        Offset(l + x1, vc),
      ]);
    case 'homePlate':
      final maxA = ss == 0 ? 0.0 : 100000 * w / ss;
      final a = _adj(adj, 0, 50000).clamp(0.0, maxA);
      final x1 = rt - ss * a / 100000;
      return _poly([
        Offset(l, t),
        Offset(x1, t),
        Offset(rt, vc),
        Offset(x1, b),
        Offset(l, b)
      ]);
    case 'pentagon':
      return _unitPolygon(
        const [
          Offset(0, -1),
          Offset(0.95106, -0.30902),
          Offset(0.58779, 0.80902),
          Offset(-0.58779, 0.80902),
          Offset(-0.95106, -0.30902),
        ],
        r,
      );
    case 'hexagon':
      final maxA = ss == 0 ? 0.0 : 50000 * w / ss;
      final a = _adj(adj, 0, 25000).clamp(0.0, maxA);
      final x1 = ss * a / 100000;
      return _poly([
        Offset(l, vc),
        Offset(l + x1, t),
        Offset(rt - x1, t),
        Offset(rt, vc),
        Offset(rt - x1, b),
        Offset(l + x1, b),
      ]);
    case 'octagon':
      final a = _adj(adj, 0, 29289).clamp(0.0, 50000.0);
      final x1 = ss * a / 100000;
      return _poly([
        Offset(l, t + x1),
        Offset(l + x1, t),
        Offset(rt - x1, t),
        Offset(rt, t + x1),
        Offset(rt, b - x1),
        Offset(rt - x1, b),
        Offset(l + x1, b),
        Offset(l, b - x1),
      ]);
    case 'plus':
      final a = _adj(adj, 0, 25000).clamp(0.0, 50000.0);
      final x1 = ss * a / 100000;
      return _poly([
        Offset(l, t + x1),
        Offset(l + x1, t + x1),
        Offset(l + x1, t),
        Offset(rt - x1, t),
        Offset(rt - x1, t + x1),
        Offset(rt, t + x1),
        Offset(rt, b - x1),
        Offset(rt - x1, b - x1),
        Offset(rt - x1, b),
        Offset(l + x1, b),
        Offset(l + x1, b - x1),
        Offset(l, b - x1),
      ]);
    case 'rightArrow':
    case 'leftArrow':
      final a1 = _adj(adj, 0, 50000).clamp(0.0, 100000.0);
      final maxA2 = ss == 0 ? 0.0 : 100000 * w / ss;
      final a2 = _adj(adj, 1, 50000).clamp(0.0, maxA2);
      final dx = ss * a2 / 100000;
      final dy = h * a1 / 200000;
      final y1 = vc - dy, y2 = vc + dy;
      if (name == 'rightArrow') {
        final x1 = rt - dx;
        return _poly([
          Offset(l, y1),
          Offset(x1, y1),
          Offset(x1, t),
          Offset(rt, vc),
          Offset(x1, b),
          Offset(x1, y2),
          Offset(l, y2),
        ]);
      }
      final x1 = l + dx;
      return _poly([
        Offset(rt, y1),
        Offset(x1, y1),
        Offset(x1, t),
        Offset(l, vc),
        Offset(x1, b),
        Offset(x1, y2),
        Offset(rt, y2),
      ]);
    case 'upArrow':
    case 'downArrow':
      final a1 = _adj(adj, 0, 50000).clamp(0.0, 100000.0);
      final maxA2 = ss == 0 ? 0.0 : 100000 * h / ss;
      final a2 = _adj(adj, 1, 50000).clamp(0.0, maxA2);
      final dy = ss * a2 / 100000;
      final dx = w * a1 / 200000;
      final x1 = hc - dx, x2 = hc + dx;
      if (name == 'upArrow') {
        final y1 = t + dy;
        return _poly([
          Offset(x1, b),
          Offset(x1, y1),
          Offset(l, y1),
          Offset(hc, t),
          Offset(rt, y1),
          Offset(x2, y1),
          Offset(x2, b),
        ]);
      }
      final y1 = b - dy;
      return _poly([
        Offset(x1, t),
        Offset(x1, y1),
        Offset(l, y1),
        Offset(hc, b),
        Offset(rt, y1),
        Offset(x2, y1),
        Offset(x2, t),
      ]);
    case 'leftRightArrow':
      final a1 = _adj(adj, 0, 50000).clamp(0.0, 100000.0);
      final maxA2 = ss == 0 ? 0.0 : 50000 * w / ss;
      final a2 = _adj(adj, 1, 50000).clamp(0.0, maxA2);
      final dx = ss * a2 / 100000;
      final dy = h * a1 / 200000;
      final y1 = vc - dy, y2 = vc + dy;
      return _poly([
        Offset(l, vc),
        Offset(l + dx, t),
        Offset(l + dx, y1),
        Offset(rt - dx, y1),
        Offset(rt - dx, t),
        Offset(rt, vc),
        Offset(rt - dx, b),
        Offset(rt - dx, y2),
        Offset(l + dx, y2),
        Offset(l + dx, b),
      ]);
    case 'star4':
      return _star(4, _adj(adj, 0, 12500), r);
    case 'star5':
      return _star(5, _adj(adj, 0, 19098), r);
    case 'star6':
      return _star(6, _adj(adj, 0, 28868), r);
    case 'star8':
      return _star(8, _adj(adj, 0, 37500), r);
    case 'pie':
      final st = _angle(_adj(adj, 0, 0));
      final en = _angle(_adj(adj, 1, 16200000));
      final sweep = _sweep(st, en);
      return Path()
        ..moveTo(hc, vc)
        ..arcTo(r, _param(st, w, h), _sweepParam(st, sweep, w, h), false)
        ..close();
    case 'blockArc':
      final st = _angle(_adj(adj, 0, 10800000));
      final en = _angle(_adj(adj, 1, 0));
      final dr = ss * _adj(adj, 2, 25000).clamp(0.0, 50000.0) / 100000;
      final inner = r.deflate(dr);
      final sweep = _sweep(st, en);
      final p = Path()
        ..arcTo(r, _param(st, w, h), _sweepParam(st, sweep, w, h), true);
      if (inner.width > 0 && inner.height > 0) {
        p.arcTo(inner, _param(en, inner.width, inner.height),
            -_sweepParam(st, sweep, inner.width, inner.height), false);
      }
      return p..close();
    case 'donut':
      final dr = ss * _adj(adj, 0, 25000).clamp(0.0, 50000.0) / 100000;
      final p = Path()
        ..fillType = PathFillType.evenOdd
        ..addOval(r);
      final inner = r.deflate(dr);
      if (inner.width > 0 && inner.height > 0) p.addOval(inner);
      return p;
  }
  // Unknown preset: the frame as a rectangle (reported via `unsupported`).
  return Path()..addRect(r);
}

Path _unitPolygon(List<Offset> unit, Rect r) {
  var minX = double.infinity, maxX = -double.infinity;
  var minY = double.infinity, maxY = -double.infinity;
  for (final p in unit) {
    minX = math.min(minX, p.dx);
    maxX = math.max(maxX, p.dx);
    minY = math.min(minY, p.dy);
    maxY = math.max(maxY, p.dy);
  }
  final sx = r.width / (maxX - minX), sy = r.height / (maxY - minY);
  return _poly([
    for (final p in unit)
      Offset(r.left + (p.dx - minX) * sx, r.top + (p.dy - minY) * sy),
  ]);
}

Path _star(int n, double adj, Rect r) {
  final inner = (adj / 50000).clamp(0.05, 1.0);
  final pts = <Offset>[];
  for (var i = 0; i < n * 2; i++) {
    final rad = i.isEven ? 1.0 : inner;
    final a = -math.pi / 2 + i * math.pi / n;
    pts.add(Offset(rad * math.cos(a), rad * math.sin(a)));
  }
  return _unitPolygon(pts, r);
}

// OOXML angles are 60000ths of a degree, clockwise from +x.
double _angle(double v) => v / 60000 * math.pi / 180;

double _sweep(double st, double en) {
  var s = (en - st) % (2 * math.pi);
  if (s <= 1e-9) s += 2 * math.pi;
  return s;
}

/// A visual angle on an ellipse of size [w]x[h] as the parametric angle
/// `Path.arcTo` expects.
double _param(double a, double w, double h) {
  if ((w - h).abs() < 1e-6) return a;
  return math.atan2(w * math.sin(a), h * math.cos(a));
}

double _sweepParam(double st, double sweep, double w, double h) {
  if ((w - h).abs() < 1e-6) return sweep;
  final p0 = _param(st, w, h);
  var p1 = _param(st + sweep, w, h);
  var s = p1 - p0;
  while (s < 0) {
    s += 2 * math.pi;
  }
  // A full-circle sweep collapses to 0 after wrapping.
  if (sweep > 2 * math.pi - 1e-6) s = 2 * math.pi;
  return s;
}

GeomPaths _arc(List<double> adj, Rect r) {
  final st = _angle(_adj(adj, 0, 16200000));
  final en = _angle(_adj(adj, 1, 0));
  final sweep = _sweep(st, en);
  final p0 = _param(st, r.width, r.height);
  final ps = _sweepParam(st, sweep, r.width, r.height);
  final stroke = Path()..arcTo(r, p0, ps, true);
  final fill = Path()
    ..moveTo(r.center.dx, r.center.dy)
    ..arcTo(r, p0, ps, false)
    ..close();
  return GeomPaths(fill, stroke);
}

// ---------------------------------------------------------------------------
// Fonts
//
// Office fonts (Calibri, Cambria, Gill Sans MT, ...) are usually not
// installed on iOS/Android. A run keeps the family it was authored with (a
// device that has it uses it) and always carries a fallback chain so a
// missing font degrades to a sensible sans/serif/mono/Arabic face instead
// of throwing or rendering tofu.

/// A resolved font request: [family] as authored (may be null) plus the
/// [fallback] chain handed to `TextStyle.fontFamilyFallback`.
class FontChoice {
  const FontChoice(this.family, this.fallback);
  final String? family;
  final List<String> fallback;
}

const List<String> _sansFallback = [
  'Carlito',
  'Helvetica Neue',
  'Arial',
  'Roboto',
  'Noto Sans',
  'sans-serif',
];
const List<String> _serifFallback = [
  'Cambria',
  'Georgia',
  'Times New Roman',
  'Noto Serif',
  'Roboto Serif',
  'serif',
];
const List<String> _monoFallback = [
  'Menlo',
  'Courier New',
  'Roboto Mono',
  'monospace',
];
const List<String> _arabicFallback = [
  'Geeza Pro',
  'Noto Naskh Arabic',
  'Noto Sans Arabic',
  'Arial',
  'Tahoma',
  'Roboto',
  'sans-serif',
];

const Set<String> _serifFamilies = {
  'cambria',
  'georgia',
  'times new roman',
  'times',
  'garamond',
  'palatino',
  'palatino linotype',
  'book antiqua',
  'baskerville',
  'bookman old style',
  'century schoolbook',
  'constantia',
  'didot',
  'rockwell',
  'perpetua',
  'sitka text',
  'sitka heading',
  'minion pro',
  'noto serif',
};
const Set<String> _monoFamilies = {
  'consolas',
  'courier new',
  'courier',
  'menlo',
  'monaco',
  'lucida console',
  'andale mono',
  'roboto mono',
  'sf mono',
  'cascadia code',
  'cascadia mono',
};

final RegExp _complexScript = RegExp(r'[֐-ࣿיִ-﷿ﹰ-﻿]');

/// Whether [text] contains right-to-left / complex-script letters
/// (Hebrew, Arabic, Syriac, Thaana, presentation forms).
bool hasComplexScript(String text) => _complexScript.hasMatch(text);

/// Resolves [requested] to a family + fallback chain. With
/// [complexScript] the Arabic chain is used (and the complex-script family
/// [cs], when given, takes precedence).
FontChoice resolveSlideFont(String? requested,
    {String? cs, bool complexScript = false}) {
  if (complexScript) {
    final fam = (cs != null && cs.isNotEmpty) ? cs : requested;
    return FontChoice(
        (fam == null || fam.isEmpty) ? null : fam, _arabicFallback);
  }
  final name = (requested ?? '').trim();
  if (name.isEmpty) return const FontChoice(null, _sansFallback);
  final key = name.toLowerCase();
  if (_monoFamilies.contains(key)) return FontChoice(name, _monoFallback);
  if (_serifFamilies.contains(key)) return FontChoice(name, _serifFallback);
  return FontChoice(name, _sansFallback);
}
