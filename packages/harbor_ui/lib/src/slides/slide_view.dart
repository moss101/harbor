// SlideView / SlideThumbnail widgets and painters (owner: SlideView agent).
//
// A slide scene (`slide_scene.dart`) becomes pixels here. Nothing reads the
// app theme: a slide paints with its own colours, light or dark.
//
// Units. The deck is in EMU; the view is `W` px wide, so `scale = W /
// deckWidth` px per EMU and a font of `sz` points paints at
// `sz * 12700 * scale` px.
//
// Performance. Text is laid out once per (text body, scale, box) and cached
// on the body itself (an [Expando], so the cache dies with the scene); a
// repaint during an animation therefore only re-draws already shaped
// paragraphs. Decoded pictures live in [SlideImageCache], keyed on the
// base64 payload.
import 'dart:collection';
import 'dart:convert';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'slide_charts.dart';
import 'slide_geometry.dart';
import 'slide_scene.dart';

const double _kEmuPerPt = 12700;

// ---------------------------------------------------------------------------
// ShapeEffect

/// Per-shape runtime effect applied on top of the static scene (the
/// presenter animates these; static previews pass none).
///
/// * [opacity] 0..1, applied to the whole shape (saveLayer).
/// * [dx]/[dy] translation in EMU (slide space, applied after [scale]).
/// * [scale] about the shape's centre.
/// * [wipe] reveal fraction 0..1 along [wipeDir]; null or >= 1 = fully
///   visible. [wipeDir] names the edge the reveal **starts from** (as in
///   PowerPoint's "From Left"): `l` reveals left-to-right, `r` right-to-
///   left, `u` top-to-bottom (from the top edge), `d` bottom-to-top (from
///   the bottom edge). The reveal follows the shape's own axes (it turns
///   with the shape).
@immutable
class ShapeEffect {
  const ShapeEffect({
    this.opacity = 1,
    this.dx = 0,
    this.dy = 0,
    this.scale = 1,
    this.wipe,
    this.wipeDir,
  });

  final double opacity;
  final double dx, dy;
  final double scale;
  final double? wipe;
  final String? wipeDir;

  bool get isIdentity =>
      opacity >= 1 &&
      dx == 0 &&
      dy == 0 &&
      scale == 1 &&
      (wipe == null || wipe! >= 1);

  @override
  bool operator ==(Object other) =>
      other is ShapeEffect &&
      other.opacity == opacity &&
      other.dx == dx &&
      other.dy == dy &&
      other.scale == scale &&
      other.wipe == wipe &&
      other.wipeDir == wipeDir;

  @override
  int get hashCode => Object.hash(opacity, dx, dy, scale, wipe, wipeDir);

  @override
  String toString() =>
      'ShapeEffect(opacity: $opacity, dx: $dx, dy: $dy, scale: $scale, wipe: $wipe, wipeDir: $wipeDir)';
}

// ---------------------------------------------------------------------------
// Pictures

enum SlideImageState { loading, ready, failed }

/// Decoded-picture cache keyed on the base64 payload. Decoding is async;
/// painters call [peek] (which starts a load on a miss) and repaint when
/// this notifier fires. Large photos are decoded at a capped size.
class SlideImageCache extends ChangeNotifier {
  SlideImageCache._();

  static final SlideImageCache instance = SlideImageCache._();

  static const int maxEntries = 48;
  static const int maxDimension = 2400;

  final LinkedHashMap<String, _ImageEntry> _entries =
      LinkedHashMap<String, _ImageEntry>();

  _ImageEntry _entry(String b64) {
    var e = _entries.remove(b64);
    if (e == null) {
      e = _ImageEntry();
      e.future = _decode(b64).then((img) {
        e!.image = img;
        e.state = img == null ? SlideImageState.failed : SlideImageState.ready;
        notifyListeners();
        return img;
      });
      while (_entries.length >= maxEntries) {
        _entries.remove(_entries.keys.first);
      }
    }
    _entries[b64] = e;
    return e;
  }

  /// The decoded image if ready; otherwise starts loading and returns null.
  ui.Image? peek(String b64) => b64.isEmpty ? null : _entry(b64).image;

  SlideImageState stateOf(String b64) =>
      b64.isEmpty ? SlideImageState.failed : _entry(b64).state;

  /// Decodes (or reuses) [b64]. Null when it cannot be decoded.
  Future<ui.Image?> load(String b64) =>
      b64.isEmpty ? Future.value(null) : _entry(b64).future;

  /// Starts decoding every picture of [scene] ahead of the first paint.
  void precache(SlideScene scene) {
    for (final it in scene.items) {
      if (it is ScenePicture && it.dataB64.isNotEmpty) _entry(it.dataB64);
    }
  }

  void clear() {
    _entries.clear();
    notifyListeners();
  }

  Future<ui.Image?> _decode(String b64) async {
    try {
      final bytes = base64Decode(b64);
      final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
      try {
        final desc = await ui.ImageDescriptor.encoded(buffer);
        try {
          final w = desc.width, h = desc.height;
          int? tw, th;
          if (w > maxDimension || h > maxDimension) {
            if (w >= h) {
              tw = maxDimension;
            } else {
              th = maxDimension;
            }
          }
          final codec =
              await desc.instantiateCodec(targetWidth: tw, targetHeight: th);
          try {
            return (await codec.getNextFrame()).image;
          } finally {
            codec.dispose();
          }
        } finally {
          desc.dispose();
        }
      } finally {
        buffer.dispose();
      }
    } catch (_) {
      return null;
    }
  }
}

class _ImageEntry {
  ui.Image? image;
  SlideImageState state = SlideImageState.loading;
  late Future<ui.Image?> future;
}

// ---------------------------------------------------------------------------
// Text layout

class _SupBox {
  _SupBox(this.painter, this.placeholder);
  final TextPainter painter;
  final int placeholder;
}

class _ParaLayout {
  _ParaLayout({
    required this.painter,
    required this.height,
    required this.dx,
    this.bullet,
    this.bulletDx = 0,
    this.bulletDy = 0,
    this.sups = const [],
  });
  final TextPainter painter;
  final double height;
  double top = 0;
  final double dx;
  final TextPainter? bullet;
  final double bulletDx, bulletDy;
  final List<_SupBox> sups;
}

class _BodyLayout {
  _BodyLayout(this.paras, this.height);
  final List<_ParaLayout> paras;
  final double height;
}

final Expando<Map<String, _BodyLayout>> _layoutCache = Expando('slide text');

const Map<String, String> _wingdings = {
  '§': '▪',
  'Ø': '➢',
  'ü': '✓',
  'v': '❖',
  'l': '●',
  'n': '■',
  'u': '◆',
  'q': '❑',
  'p': '▫',
  'à': '→',
  'è': '→',
};

String _bulletGlyph(SceneBullet b) {
  final f = (b.font ?? '').toLowerCase();
  var ch = b.ch.isEmpty ? '•' : b.ch;
  if (f.startsWith('wingdings')) return _wingdings[ch] ?? '▪';
  if (f == 'symbol') {
    if (ch == '·' || ch == '') return '•';
    return ch;
  }
  if (f == 'courier new' && ch == 'o') return '○';
  if (ch.codeUnitAt(0) >= 0xF000 && ch.codeUnitAt(0) <= 0xF0FF) {
    return '•'; // private-use symbol-font code points
  }
  return ch;
}

String _roman(int n) {
  if (n <= 0 || n >= 4000) return '$n';
  const vals = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
  const syms = [
    'M',
    'CM',
    'D',
    'CD',
    'C',
    'XC',
    'L',
    'XL',
    'X',
    'IX',
    'V',
    'IV',
    'I'
  ];
  var out = '';
  for (var i = 0; i < vals.length; i++) {
    while (n >= vals[i]) {
      out += syms[i];
      n -= vals[i];
    }
  }
  return out;
}

String _alpha(int n, {required bool upper}) {
  if (n <= 0) return '$n';
  final letter = String.fromCharCode(97 + (n - 1) % 26);
  final s = letter * ((n - 1) ~/ 26 + 1);
  return upper ? s.toUpperCase() : s;
}

/// "1.", "a)", "(iv)" ... for an OOXML auto-number scheme.
String slideNumberText(String numType, int n) {
  final t = numType.isEmpty ? 'arabicPeriod' : numType;
  String body;
  String rest;
  if (t.startsWith('romanUc')) {
    body = _roman(n);
    rest = t.substring(7);
  } else if (t.startsWith('romanLc')) {
    body = _roman(n).toLowerCase();
    rest = t.substring(7);
  } else if (t.startsWith('alphaUc')) {
    body = _alpha(n, upper: true);
    rest = t.substring(7);
  } else if (t.startsWith('alphaLc')) {
    body = _alpha(n, upper: false);
    rest = t.substring(7);
  } else {
    body = '$n';
    rest = t.startsWith('arabic') ? t.substring(6) : 'Period';
  }
  switch (rest) {
    case 'ParenR':
      return '$body)';
    case 'ParenBoth':
      return '($body)';
    case 'Plain':
      return body;
    default:
      return '$body.';
  }
}

TextAlign _textAlign(String algn) {
  switch (algn) {
    case 'ctr':
      return TextAlign.center;
    case 'r':
      return TextAlign.right;
    case 'just':
      return TextAlign.justify;
  }
  return TextAlign.left;
}

TextStyle _runStyle(SceneRun r, double pxPerPt, double fit,
    {double sizeFactor = 1, double? height, Color? color}) {
  // A run in Arabic/Hebrew script uses its complex-script family (`cs`);
  // Latin runs keep `font` even inside a right-to-left paragraph.
  final font =
      resolveSlideFont(r.font, cs: r.cs, complexScript: hasComplexScript(r.t));
  final deco = <TextDecoration>[
    if (r.u) TextDecoration.underline,
    if (r.strike) TextDecoration.lineThrough,
  ];
  final c = color ?? r.color;
  return TextStyle(
    inherit: false,
    fontSize: math.max(r.sz * pxPerPt * fit * sizeFactor, 0.5),
    fontWeight: r.b ? FontWeight.w700 : FontWeight.w400,
    fontStyle: r.i ? FontStyle.italic : FontStyle.normal,
    color: c,
    decoration:
        deco.isEmpty ? TextDecoration.none : TextDecoration.combine(deco),
    decorationColor: c,
    fontFamily: font.family,
    fontFamilyFallback: font.fallback,
    letterSpacing: r.spc == 0 ? null : r.spc * pxPerPt,
    height: height,
    leadingDistribution: height == null ? null : TextLeadingDistribution.even,
  );
}

SceneRun? _firstTextRun(SceneParagraph p) {
  for (final r in p.runs) {
    if (r.t.isNotEmpty && !r.isBreak) return r;
  }
  return null;
}

bool _paraHasText(SceneParagraph p) =>
    p.runs.any((r) => r.t.isNotEmpty && !r.isBreak);

/// Lays out [body] for a content box of [contentW] x [contentH] px.
_BodyLayout _layoutBodyFor(
    SceneTextBody body, double pxPerEmu, double contentW, double contentH) {
  final shrink = body.autofit == 'shrink';
  final key =
      '$pxPerEmu|${contentW.toStringAsFixed(2)}|${shrink ? contentH.toStringAsFixed(2) : '-'}';
  final cache = _layoutCache[body] ??= <String, _BodyLayout>{};
  final hit = cache[key];
  if (hit != null) return hit;
  if (cache.length >= 6) {
    final oldest = cache.keys.first;
    final old = cache.remove(oldest);
    if (old != null) _disposeLayout(old);
  }
  var layout = _buildBody(body, pxPerEmu, contentW, body.fontScale, 1.0);
  if (shrink && layout.height > contentH + 0.5) {
    for (var step = 1; step <= 8; step++) {
      final m = 1.0 - step * 0.05; // 95% ... 60%
      _disposeLayout(layout);
      layout = _buildBody(body, pxPerEmu, contentW, body.fontScale * m, m);
      if (layout.height <= contentH + 0.5) break;
    }
  }
  cache[key] = layout;
  return layout;
}

void _disposeLayout(_BodyLayout l) {
  for (final p in l.paras) {
    p.painter.dispose();
    p.bullet?.dispose();
    for (final s in p.sups) {
      s.painter.dispose();
    }
  }
}

_BodyLayout _buildBody(SceneTextBody body, double pxPerEmu, double contentW,
    double fit, double shrink) {
  final pxPerPt = _kEmuPerPt * pxPerEmu;
  final out = <_ParaLayout>[];
  final counters = <int, int>{};
  var y = 0.0;
  for (var i = 0; i < body.paras.length; i++) {
    final p = body.paras[i];
    // Auto-number bookkeeping: consecutive numbered paragraphs of one level
    // count up; anything else at that level (or shallower) restarts it.
    final level = p.marL.round();
    var number = 0;
    final hasText = _paraHasText(p);
    if (hasText && p.bullet != null && p.bullet!.isNumbered) {
      counters.removeWhere((k, _) => k > level);
      number = (counters[level] ?? (p.bullet!.start - 1)) + 1;
      counters[level] = number;
    } else if (hasText) {
      counters.removeWhere((k, _) => k >= level);
    }
    final pl = _layoutPara(p, number, pxPerEmu, pxPerPt, fit, shrink, contentW,
        body.wrap, body.lineReduction);
    if (i > 0) y += p.spcBef * pxPerPt * shrink * (1 - body.lineReduction);
    pl.top = y;
    y += pl.height;
    if (i < body.paras.length - 1) {
      y += p.spcAft * pxPerPt * shrink * (1 - body.lineReduction);
    }
    out.add(pl);
  }
  return _BodyLayout(out, y);
}

_ParaLayout _layoutPara(
    SceneParagraph p,
    int number,
    double pxPerEmu,
    double pxPerPt,
    double fit,
    double shrink,
    double contentW,
    bool wrap,
    double lineReduction) {
  final dir = p.rtl ? TextDirection.rtl : TextDirection.ltr;
  final mar = math.max(0.0, p.marL) * pxPerEmu;
  final indent = p.indent * pxPerEmu;
  final avail = math.max(contentW - mar, 1.0);
  final hasText = _paraHasText(p);
  final align = _textAlign(p.algn);

  // Bullet.
  TextPainter? bulletTp;
  var lead = 0.0;
  var bulletX = 0.0;
  final first = _firstTextRun(p);
  if (hasText && p.bullet != null && first != null) {
    final b = p.bullet!;
    final text = b.isNumbered
        ? slideNumberText(b.numType, number == 0 ? b.start : number)
        : _bulletGlyph(b);
    if (text.isNotEmpty && (b.isNumbered || b.kind == 'char')) {
      final bulletRun = SceneRun(
        t: text,
        sz: first.sz,
        b: b.isNumbered ? first.b : false,
        i: false,
        c: b.c ?? first.c,
        a: b.c == null ? first.a : 1,
        font:
            (b.font != null && !(b.font!.toLowerCase().startsWith('wingdings')))
                ? b.font!
                : first.font,
        cs: first.cs,
      );
      bulletTp = TextPainter(
        text: TextSpan(
            text: text,
            style: _runStyle(bulletRun, pxPerPt, fit, sizeFactor: b.szPct)),
        textDirection: dir,
        maxLines: 1,
      )..layout();
      bulletX = math.max(0.0, mar + indent);
      final gap = bulletTp.height * 0.18;
      final firstStart = math.max(mar, bulletX + bulletTp.width + gap);
      lead = firstStart - mar;
    }
  } else if (indent > 0) {
    lead = indent;
  }
  final useLead = lead > 0.5 &&
      (p.algn == 'l' ||
          p.algn == 'just' ||
          (p.rtl && (p.algn == 'r' || p.algn == 'just')) ||
          bulletTp != null);

  // Line spacing.
  final pct = (p.lnPct ?? 1.0) * (1 - lineReduction);
  final double? height =
      (p.lnPts == null && (pct - 1).abs() > 0.001) ? pct * 1.2 : null;
  StrutStyle? strut;
  if (p.lnPts != null && p.lnPts! > 0) {
    strut = StrutStyle(
      fontSize: p.lnPts! * pxPerPt * fit,
      height: 1,
      forceStrutHeight: true,
      leading: 0,
    );
  }

  // Spans.
  final children = <InlineSpan>[];
  final dims = <PlaceholderDimensions>[];
  final sups = <_SupBox>[];
  void addPlaceholder(Size size, double baselineOffset) {
    children.add(const WidgetSpan(
      child: SizedBox.shrink(),
      alignment: PlaceholderAlignment.baseline,
      baseline: TextBaseline.alphabetic,
    ));
    dims.add(PlaceholderDimensions(
      size: size,
      alignment: PlaceholderAlignment.baseline,
      baseline: TextBaseline.alphabetic,
      baselineOffset: baselineOffset,
    ));
  }

  if (useLead) addPlaceholder(Size(lead, 0.01), 0);
  for (final r in p.runs) {
    if (r.t.isEmpty) continue;
    final text = r.caps ? r.t.toUpperCase() : r.t;
    if (r.isBreak) {
      children.add(TextSpan(
          text: '\n', style: _runStyle(r, pxPerPt, fit, height: height)));
      continue;
    }
    if (r.baseline != 0) {
      final style = _runStyle(r, pxPerPt, fit, sizeFactor: 0.66, height: null);
      if (text.length <= 24 && !text.contains('\n')) {
        final sub = TextPainter(
          text: TextSpan(text: text, style: style),
          textDirection: dir,
          maxLines: 1,
        )..layout();
        final baseDist =
            sub.computeDistanceToActualBaseline(TextBaseline.alphabetic);
        final shift = r.baseline / 100 * r.sz * pxPerPt * fit;
        addPlaceholder(Size(sub.width, sub.height), baseDist + shift);
        sups.add(_SupBox(sub, dims.length - 1));
      } else {
        children.add(TextSpan(text: text, style: style));
      }
      continue;
    }
    children.add(TextSpan(
        text: text, style: _runStyle(r, pxPerPt, fit, height: height)));
  }

  final empty = children.isEmpty || !hasText && sups.isEmpty;
  late final TextPainter tp;
  if (empty) {
    final run = SceneRun(t: ' ', sz: p.endSz > 0 ? p.endSz : 18);
    tp = TextPainter(
      text: TextSpan(
          text: ' ', style: _runStyle(run, pxPerPt, fit, height: height)),
      textDirection: dir,
      strutStyle: strut,
    )..layout(maxWidth: avail);
    return _ParaLayout(painter: tp, height: tp.height, dx: 0);
  }
  tp = TextPainter(
    text: TextSpan(children: children),
    textDirection: dir,
    textAlign: align,
    strutStyle: strut,
  );
  if (dims.isNotEmpty) tp.setPlaceholderDimensions(dims);
  if (wrap) {
    tp.layout(minWidth: avail, maxWidth: avail);
  } else {
    tp.layout();
  }

  // Horizontal placement of the paragraph box inside the content box.
  double dx;
  if (wrap) {
    dx = p.rtl ? 0 : mar;
  } else {
    final w = tp.width;
    switch (p.algn) {
      case 'ctr':
        dx = mar + (avail - w) / 2;
      case 'r':
        dx = p.rtl ? contentW - mar - w : contentW - w;
      default:
        dx = p.rtl ? contentW - mar - w : mar;
    }
  }

  var bDx = 0.0, bDy = 0.0;
  if (bulletTp != null) {
    final metrics = tp.computeLineMetrics();
    final base = metrics.isEmpty ? tp.height * 0.8 : metrics.first.baseline;
    bDy = base -
        bulletTp.computeDistanceToActualBaseline(TextBaseline.alphabetic);
    bDx = p.rtl ? contentW - bulletX - bulletTp.width : bulletX;
  }
  return _ParaLayout(
    painter: tp,
    height: tp.height,
    dx: dx,
    bullet: bulletTp,
    bulletDx: bDx,
    bulletDy: bDy,
    sups: sups,
  );
}

/// Paints [body] in [box] (px) clipped to [clip]. [insets] defaults to the
/// body's own `ins`. Paragraph indices in [hidden] keep their space but are
/// not painted.
void _paintTextBody(Canvas canvas, SceneTextBody body, Rect box, Rect clip,
    double pxPerEmu, Set<int>? hidden) {
  if (body.paras.isEmpty) return;
  final l = body.ins[0] * pxPerEmu, t = body.ins[1] * pxPerEmu;
  final r = body.ins[2] * pxPerEmu, b = body.ins[3] * pxPerEmu;
  final content = Rect.fromLTRB(
      box.left + l,
      box.top + t,
      math.max(box.right - r, box.left + l + 1),
      math.max(box.bottom - b, box.top + t + 1));
  final layout = _layoutBodyFor(body, pxPerEmu, content.width, content.height);
  double y0;
  switch (body.anchor) {
    case 'ctr':
      y0 = content.top + (content.height - layout.height) / 2;
    case 'b':
      y0 = content.bottom - layout.height;
    default:
      y0 = content.top;
  }
  canvas.save();
  canvas.clipRect(clip);
  for (var i = 0; i < layout.paras.length; i++) {
    if (hidden != null && hidden.contains(i)) continue;
    final pl = layout.paras[i];
    final origin = Offset(content.left + pl.dx, y0 + pl.top);
    pl.painter.paint(canvas, origin);
    if (pl.sups.isNotEmpty) {
      final boxes = pl.painter.inlinePlaceholderBoxes;
      if (boxes != null) {
        for (final s in pl.sups) {
          if (s.placeholder < boxes.length) {
            final bx = boxes[s.placeholder];
            s.painter.paint(canvas, origin + Offset(bx.left, bx.top));
          }
        }
      }
    }
    if (pl.bullet != null) {
      pl.bullet!.paint(canvas,
          Offset(content.left + pl.bulletDx, y0 + pl.top + pl.bulletDy));
    }
  }
  canvas.restore();
}

/// Height in px the text of [body] needs at [contentW] (for table rows).
double _bodyHeight(SceneTextBody body, double pxPerEmu, double contentW) {
  if (body.paras.isEmpty) return 0;
  return _layoutBodyFor(body, pxPerEmu, contentW, double.infinity).height;
}

// ---------------------------------------------------------------------------
// Paint helpers

Paint _paintFor(ScenePaint p, Rect r) {
  final paint = Paint()..isAntiAlias = true;
  switch (p.kind) {
    case PaintKind.solid:
      paint.color = p.color;
    case PaintKind.linear:
      final stops = _sortedStops(p.stops);
      if (stops.length == 1) {
        paint.color = stops.first.color;
        break;
      }
      final th = p.angle * math.pi / 180;
      final dx = math.cos(th), dy = math.sin(th);
      // The gradient line passes through the centre; it is long enough that
      // the 0% / 100% colours land exactly on the corners it first touches.
      final len = (r.width * dx).abs() + (r.height * dy).abs();
      final c = r.center;
      paint.shader = ui.Gradient.linear(
        Offset(c.dx - dx * len / 2, c.dy - dy * len / 2),
        Offset(c.dx + dx * len / 2, c.dy + dy * len / 2),
        [for (final s in stops) s.color],
        [for (final s in stops) s.p.clamp(0.0, 1.0).toDouble()],
      );
    case PaintKind.radial:
      final stops = _sortedStops(p.stops);
      if (stops.length == 1) {
        paint.color = stops.first.color;
        break;
      }
      final c = Offset(r.left + r.width * p.cx, r.top + r.height * p.cy);
      final radius = math.max(math.max(p.cx, 1 - p.cx) * r.width,
          math.max(p.cy, 1 - p.cy) * r.height);
      paint.shader = ui.Gradient.radial(
        c,
        math.max(radius, 0.5),
        [for (final s in stops) s.color],
        [for (final s in stops) s.p.clamp(0.0, 1.0).toDouble()],
      );
    case PaintKind.none:
      break;
  }
  return paint;
}

List<SceneStop> _sortedStops(List<SceneStop> stops) {
  final s = [...stops];
  s.sort((a, b) => a.p.compareTo(b.p));
  return s;
}

Paint? _strokePaint(SceneLine? line, Rect r, double pxPerEmu) {
  if (line == null || line.paint.isNone) return null;
  final w = math.max(line.w * pxPerEmu, 0.75);
  final paint = _paintFor(line.paint, r)
    ..style = PaintingStyle.stroke
    ..strokeWidth = w
    ..strokeCap = line.cap == 'round'
        ? StrokeCap.round
        : line.cap == 'square'
            ? StrokeCap.square
            : StrokeCap.butt
    ..strokeJoin = StrokeJoin.miter;
  return paint;
}

Path _dashed(Path path, SceneLine line, double strokeWidth) {
  final pattern = dashPattern(line.dash, strokeWidth);
  return pattern == null ? path : dashPath(path, pattern);
}

void _drawArrow(Canvas canvas, String kind, Offset tip, Offset outward,
    double size, Paint linePaint, double strokeWidth) {
  final len = outward.distance;
  if (len == 0) return;
  final dir = outward / len;
  final n = Offset(-dir.dy, dir.dx);
  final fill = Paint()
    ..color = linePaint.color
    ..shader = linePaint.shader
    ..style = PaintingStyle.fill
    ..isAntiAlias = true;
  final back = tip - dir * size;
  switch (kind) {
    case 'oval':
      canvas.drawCircle(tip - dir * (size / 2), size / 2, fill);
    case 'diamond':
      canvas.drawPath(
          Path()
            ..moveTo(tip.dx, tip.dy)
            ..lineTo(tip.dx - dir.dx * size / 2 + n.dx * size / 2,
                tip.dy - dir.dy * size / 2 + n.dy * size / 2)
            ..lineTo(back.dx, back.dy)
            ..lineTo(tip.dx - dir.dx * size / 2 - n.dx * size / 2,
                tip.dy - dir.dy * size / 2 - n.dy * size / 2)
            ..close(),
          fill);
    case 'arrow':
      final p = Path()
        ..moveTo(back.dx + n.dx * size / 2, back.dy + n.dy * size / 2)
        ..lineTo(tip.dx, tip.dy)
        ..lineTo(back.dx - n.dx * size / 2, back.dy - n.dy * size / 2);
      canvas.drawPath(
          p,
          Paint()
            ..color = linePaint.color
            ..shader = linePaint.shader
            ..style = PaintingStyle.stroke
            ..strokeWidth = strokeWidth
            ..strokeJoin = StrokeJoin.miter
            ..isAntiAlias = true);
    case 'stealth':
      final notch = tip - dir * (size * 0.65);
      canvas.drawPath(
          Path()
            ..moveTo(tip.dx, tip.dy)
            ..lineTo(back.dx + n.dx * size / 2, back.dy + n.dy * size / 2)
            ..lineTo(notch.dx, notch.dy)
            ..lineTo(back.dx - n.dx * size / 2, back.dy - n.dy * size / 2)
            ..close(),
          fill);
    default: // triangle
      canvas.drawPath(
          Path()
            ..moveTo(tip.dx, tip.dy)
            ..lineTo(back.dx + n.dx * size / 2, back.dy + n.dy * size / 2)
            ..lineTo(back.dx - n.dx * size / 2, back.dy - n.dy * size / 2)
            ..close(),
          fill);
  }
}

// ---------------------------------------------------------------------------
// Painter

/// Paints one [SlideScene] into the canvas size it is given.
class SlidePainter extends CustomPainter {
  SlidePainter({
    required this.scene,
    required this.deckWidth,
    this.effects = const {},
    this.hidden = const {},
    this.hiddenParas = const {},
  }) : super(repaint: SlideImageCache.instance);

  final SlideScene scene;
  final double deckWidth;
  final Map<int, ShapeEffect> effects;
  final Set<int> hidden;
  final Map<int, Set<int>> hiddenParas;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || deckWidth <= 0) return;
    final s = size.width / deckWidth;
    final bounds = Offset.zero & size;
    canvas.save();
    canvas.clipRect(bounds);
    final bg = scene.background;
    if (bg.isNone) {
      canvas.drawRect(bounds, Paint()..color = const Color(0xFFFFFFFF));
    } else {
      canvas.drawRect(bounds, _paintFor(bg, bounds));
    }
    for (final item in scene.items) {
      if (hidden.contains(item.id)) continue;
      _paintItem(
          canvas, item, s, effects[item.id], hiddenParas[item.id], bounds);
    }
    canvas.restore();
  }

  void _paintItem(Canvas canvas, SceneItem it, double s, ShapeEffect? fx,
      Set<int>? hiddenP, Rect slideBounds) {
    final w = it.w * s, h = it.h * s;
    if (w <= 0 && h <= 0) return;
    final center = Offset((it.x + it.w / 2) * s, (it.y + it.h / 2) * s);
    final useLayer = fx != null && fx.opacity < 1;
    if (useLayer && fx.opacity <= 0) return;
    canvas.save();
    if (useLayer) {
      canvas.saveLayer(
          slideBounds,
          Paint()
            ..color =
                Color.fromRGBO(0, 0, 0, fx.opacity.clamp(0.0, 1.0).toDouble()));
    }
    if (fx != null) {
      canvas.translate(fx.dx * s, fx.dy * s);
      if (fx.scale != 1) {
        canvas.translate(center.dx, center.dy);
        canvas.scale(fx.scale);
        canvas.translate(-center.dx, -center.dy);
      }
    }
    canvas.translate(center.dx, center.dy);
    if (it.rot != 0) canvas.rotate(it.rot * math.pi / 180);
    final local = Rect.fromLTWH(-w / 2, -h / 2, w, h);
    if (fx != null && fx.wipe != null && fx.wipe! < 1) {
      canvas.clipRect(
          _wipeClip(local, fx.wipe!.clamp(0.0, 1.0).toDouble(), fx.wipeDir));
    }

    switch (it) {
      case SceneShape():
        _paintShape(canvas, it, local, s, hiddenP);
      case ScenePicture():
        _paintPicture(canvas, it, local, s);
      case SceneChart():
        paintChart(canvas, local, it.chart, fontPx: 11 * _kEmuPerPt * s);
      case SceneTable():
        _paintTable(canvas, it, local, s);
    }
    if (useLayer) canvas.restore();
    canvas.restore();
  }

  Rect _wipeClip(Rect r, double f, String? dir) {
    const big = 1e5;
    switch (dir) {
      case 'r':
        return Rect.fromLTRB(r.right - r.width * f, -big, big, big);
      case 'u':
        return Rect.fromLTRB(-big, -big, big, r.top + r.height * f);
      case 'd':
        return Rect.fromLTRB(-big, r.bottom - r.height * f, big, big);
      case 'l':
      default:
        return Rect.fromLTRB(-big, -big, r.left + r.width * f, big);
    }
  }

  // -- shared: shadow ---------------------------------------------------------

  void _drawShadow(Canvas canvas, SceneShadow sh, Path path, bool stroke,
      double strokeWidth, SceneItem it, double s) {
    final c = sh.color;
    if (c.a <= 0) return;
    // Offsets are in slide space; undo the shape's rotation and flips.
    final th = it.rot * math.pi / 180;
    var ox = (sh.dx * math.cos(th) + sh.dy * math.sin(th)) * s;
    var oy = (-sh.dx * math.sin(th) + sh.dy * math.cos(th)) * s;
    if (it.flipH) ox = -ox;
    if (it.flipV) oy = -oy;
    final paint = Paint()
      ..color = c
      ..isAntiAlias = true
      ..style = stroke ? PaintingStyle.stroke : PaintingStyle.fill
      ..strokeWidth = strokeWidth;
    final sigma = sh.blur * s * 0.5;
    if (sigma > 0.3) {
      paint.maskFilter = MaskFilter.blur(BlurStyle.normal, sigma);
    }
    canvas.save();
    canvas.translate(ox, oy);
    canvas.drawPath(path, paint);
    canvas.restore();
  }

  // -- shapes -----------------------------------------------------------------

  void _paintShape(
      Canvas canvas, SceneShape it, Rect local, double s, Set<int>? hiddenP) {
    final geom = it.geom;
    final paths = geometryPaths(geom, local);
    final isLine = isLineGeometry(geom);
    canvas.save();
    if (it.flipH || it.flipV) {
      canvas.scale(it.flipH ? -1 : 1, it.flipV ? -1 : 1);
    }

    final stroke = _strokePaint(it.line, local, s);
    final hasFill = !it.fill.isNone && !isLine;

    if (it.shadow != null && (hasFill || stroke != null)) {
      if (hasFill) {
        _drawShadow(canvas, it.shadow!, paths.fill, false, 0, it, s);
      } else {
        _drawShadow(
            canvas, it.shadow!, paths.stroke, true, stroke!.strokeWidth, it, s);
      }
    }
    if (hasFill) canvas.drawPath(paths.fill, _paintFor(it.fill, local));
    if (stroke != null) {
      final p = _dashed(paths.stroke, it.line!, stroke.strokeWidth);
      canvas.drawPath(p, stroke);
      if (it.line!.head != null || it.line!.tail != null) {
        final ends = geometryLineEnds(geom, local);
        if (ends != null) {
          final size = math.max(stroke.strokeWidth * 3, 4.5 * _kEmuPerPt * s);
          if (it.line!.head != null) {
            _drawArrow(canvas, it.line!.head!, ends.start, ends.startDir, size,
                stroke, stroke.strokeWidth);
          }
          if (it.line!.tail != null) {
            _drawArrow(canvas, it.line!.tail!, ends.end, ends.endDir, size,
                stroke, stroke.strokeWidth);
          }
        }
      }
    }
    canvas.restore();

    final text = it.text;
    if (text != null && !text.isEmpty) {
      canvas.save();
      // Text is never mirrored; a vertical flip turns it upside down, as in
      // PowerPoint.
      if (it.flipV) canvas.rotate(math.pi);
      _paintTextBody(
          canvas, text, geometryTextRect(geom, local), local, s, hiddenP);
      canvas.restore();
    }
  }

  // -- pictures ---------------------------------------------------------------

  void _paintPicture(Canvas canvas, ScenePicture it, Rect local, double s) {
    final paths = geometryPaths(it.geom, local);
    canvas.save();
    if (it.flipH || it.flipV) {
      canvas.scale(it.flipH ? -1 : 1, it.flipV ? -1 : 1);
    }
    if (it.shadow != null) {
      _drawShadow(canvas, it.shadow!, paths.fill, false, 0, it, s);
    }
    final cache = SlideImageCache.instance;
    final state = cache.stateOf(it.dataB64);
    final image =
        state == SlideImageState.ready ? cache.peek(it.dataB64) : null;
    canvas.save();
    canvas.clipPath(paths.fill, doAntiAlias: true);
    if (image != null) {
      // Draw the whole image scaled/offset so the crop window lands on the
      // frame, then let the clip cut it: no source-rect edge bleed.
      final sw = (1 - it.crop[0] - it.crop[2]);
      final sh = (1 - it.crop[1] - it.crop[3]);
      if (sw > 0.0001 && sh > 0.0001) {
        final fullW = local.width / sw, fullH = local.height / sh;
        final dst = Rect.fromLTWH(local.left - it.crop[0] * fullW,
            local.top - it.crop[1] * fullH, fullW, fullH);
        canvas.clipRect(local);
        canvas.drawImageRect(
          image,
          Rect.fromLTWH(0, 0, image.width.toDouble(), image.height.toDouble()),
          dst,
          Paint()
            ..filterQuality = FilterQuality.medium
            ..isAntiAlias = true,
        );
      }
    } else if (state == SlideImageState.failed) {
      _paintPlaceholder(canvas, local);
    }
    canvas.restore();
    final stroke = _strokePaint(it.line, local, s);
    if (stroke != null) {
      canvas.drawPath(
          _dashed(paths.stroke, it.line!, stroke.strokeWidth), stroke);
    }
    canvas.restore();
  }

  void _paintPlaceholder(Canvas canvas, Rect r) {
    canvas.drawRect(r, Paint()..color = const Color(0xFFE5E7EB));
    final m = r.shortestSide;
    if (m < 24) return;
    final glyph = Paint()..color = const Color(0xFFCBD5E1);
    final c = r.center;
    final u = m * 0.18;
    canvas.drawPath(
        Path()
          ..moveTo(c.dx - u * 1.6, c.dy + u * 0.9)
          ..lineTo(c.dx - u * 0.4, c.dy - u * 0.6)
          ..lineTo(c.dx + u * 0.3, c.dy + u * 0.2)
          ..lineTo(c.dx + u * 0.8, c.dy - u * 0.3)
          ..lineTo(c.dx + u * 1.6, c.dy + u * 0.9)
          ..close(),
        glyph);
    canvas.drawCircle(Offset(c.dx - u * 0.9, c.dy - u * 0.8), u * 0.28, glyph);
  }

  // -- tables -----------------------------------------------------------------

  void _paintTable(Canvas canvas, SceneTable it, Rect local, double s) {
    if (it.rows.isEmpty || it.cols.isEmpty) return;
    final colX = <double>[local.left];
    for (final c in it.cols) {
      colX.add(colX.last + c * s);
    }
    // Rows grow to fit their text, as PowerPoint does.
    final rowH = <double>[];
    for (final row in it.rows) {
      var hh = row.h * s;
      for (var c = 0; c < row.cells.length && c < it.cols.length; c++) {
        final cell = row.cells[c];
        if (cell.mergedAway || cell.rowSpan > 1) continue;
        final end = math.min(c + cell.gridSpan, it.cols.length);
        final cw = colX[end] - colX[c];
        final ins = cell.text.ins;
        final inner = math.max(cw - (ins[0] + ins[2]) * s, 1.0);
        final need = _bodyHeight(cell.text, s, inner) + (ins[1] + ins[3]) * s;
        hh = math.max(hh, need);
      }
      rowH.add(hh);
    }
    final rowY = <double>[local.top];
    for (final h in rowH) {
      rowY.add(rowY.last + h);
    }
    final line = Paint()..isAntiAlias = true;
    for (var r = 0; r < it.rows.length; r++) {
      final cells = it.rows[r].cells;
      for (var c = 0; c < cells.length && c < it.cols.length; c++) {
        final cell = cells[c];
        if (cell.mergedAway) continue;
        final cEnd = math.min(c + cell.gridSpan, it.cols.length);
        final rEnd = math.min(r + cell.rowSpan, it.rows.length);
        final rect = Rect.fromLTRB(colX[c], rowY[r], colX[cEnd], rowY[rEnd]);
        if (!cell.fill.isNone) {
          canvas.drawRect(rect, _paintFor(cell.fill, rect));
        }
        if (!cell.text.isEmpty) {
          _paintTextBody(canvas, cell.text, rect, rect, s, null);
        }
        final b = cell.borders;
        void edge(SceneLine? l, Offset a, Offset z) {
          final sp = _strokePaint(l, rect, s);
          if (sp == null) return;
          line
            ..color = sp.color
            ..shader = sp.shader
            ..style = PaintingStyle.stroke
            ..strokeWidth = sp.strokeWidth;
          if (l!.dash != 'solid') {
            canvas.drawPath(
                _dashed(
                    Path()
                      ..moveTo(a.dx, a.dy)
                      ..lineTo(z.dx, z.dy),
                    l,
                    sp.strokeWidth),
                line);
          } else {
            canvas.drawLine(a, z, line);
          }
        }

        edge(b[0], rect.topLeft, rect.bottomLeft);
        edge(b[1], rect.topLeft, rect.topRight);
        edge(b[2], rect.topRight, rect.bottomRight);
        edge(b[3], rect.bottomLeft, rect.bottomRight);
      }
    }
  }

  @override
  bool shouldRepaint(SlidePainter old) =>
      !identical(old.scene, scene) ||
      old.deckWidth != deckWidth ||
      !mapEquals(old.effects, effects) ||
      !setEquals(old.hidden, hidden) ||
      !_parasEqual(old.hiddenParas, hiddenParas);

  static bool _parasEqual(Map<int, Set<int>> a, Map<int, Set<int>> b) {
    if (identical(a, b)) return true;
    if (a.length != b.length) return false;
    for (final e in a.entries) {
      final o = b[e.key];
      if (o == null || !setEquals(o, e.value)) return false;
    }
    return true;
  }
}

// ---------------------------------------------------------------------------
// Unsupported-feature summary + semantics label

final Expando<List<String>> _unsupportedCache = Expando('slide unsupported');
final Expando<String> _labelCache = Expando('slide label');

/// Features the slide declares unsupported plus preset shapes this painter
/// had to draw as rectangles. Cached per scene.
List<String> slideUnsupportedSummary(SlideScene scene) {
  final hit = _unsupportedCache[scene];
  if (hit != null) return hit;
  final out = <String>[...scene.unsupported];
  final seen = <String>{};
  for (final it in scene.items) {
    SceneGeom? g;
    if (it is SceneShape) g = it.geom;
    if (it is ScenePicture) g = it.geom;
    if (g is PresetGeom && !isKnownPreset(g.name) && seen.add(g.name)) {
      out.add('Shape "${g.name}" drawn as a rectangle');
    }
  }
  return _unsupportedCache[scene] = out;
}

String _defaultLabel(SlideScene scene) {
  final hit = _labelCache[scene];
  if (hit != null) return hit;
  final text = scene.plainText;
  final label =
      text.isEmpty ? 'Slide ${scene.index}' : 'Slide ${scene.index}: $text';
  return _labelCache[scene] = label;
}

// ---------------------------------------------------------------------------
// Widgets

/// Paints a slide at the width it is given (height follows the deck's
/// aspect ratio). Cheap to rebuild every frame during an animation.
class SlideView extends StatelessWidget {
  const SlideView({
    super.key,
    required this.scene,
    required this.deckWidth,
    required this.deckHeight,
    this.effects = const {},
    this.hidden = const {},
    this.hiddenParas = const {},
    this.showUnsupportedBadge = false,
    this.semanticsLabel,
  });

  final SlideScene scene;
  final double deckWidth, deckHeight;

  /// Per-shape runtime effects by shape id.
  final Map<int, ShapeEffect> effects;

  /// Shape ids not painted at all.
  final Set<int> hidden;

  /// Paragraph indices not painted per shape id (their space is kept).
  final Map<int, Set<int>> hiddenParas;

  /// Show a small badge naming what could not be drawn.
  final bool showUnsupportedBadge;

  /// Accessibility label; defaults to the slide's text.
  final String? semanticsLabel;

  @override
  Widget build(BuildContext context) {
    if (!(deckWidth > 0) || !(deckHeight > 0)) return const SizedBox.shrink();
    final unsupported = showUnsupportedBadge
        ? slideUnsupportedSummary(scene)
        : const <String>[];
    return AspectRatio(
      aspectRatio: deckWidth / deckHeight,
      child: Semantics(
        container: true,
        label: semanticsLabel ?? _defaultLabel(scene),
        child: Stack(
          fit: StackFit.expand,
          children: [
            ExcludeSemantics(
              child: CustomPaint(
                painter: SlidePainter(
                  scene: scene,
                  deckWidth: deckWidth,
                  effects: effects,
                  hidden: hidden,
                  hiddenParas: hiddenParas,
                ),
              ),
            ),
            if (unsupported.isNotEmpty)
              Positioned(
                left: 6,
                right: 6,
                bottom: 6,
                child: Align(
                  alignment: AlignmentDirectional.bottomStart,
                  child: DecoratedBox(
                    decoration: const BoxDecoration(
                      color: Color(0xE61F2937),
                      borderRadius: BorderRadius.all(Radius.circular(4)),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 6, vertical: 2),
                      child: Text(
                        'Not drawn: ${unsupported.join('; ')}',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 11,
                          color: Color(0xFFFFFFFF),
                          decoration: TextDecoration.none,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// A [SlideView] at a fixed small width in its own repaint boundary, with
/// no semantics (the surrounding list item carries the label).
class SlideThumbnail extends StatelessWidget {
  const SlideThumbnail({
    super.key,
    required this.scene,
    required this.deckWidth,
    required this.deckHeight,
    this.width = 160,
  });

  final SlideScene scene;
  final double deckWidth, deckHeight;
  final double width;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: width,
      child: RepaintBoundary(
        child: ExcludeSemantics(
          child: SlideView(
            scene: scene,
            deckWidth: deckWidth,
            deckHeight: deckHeight,
          ),
        ),
      ),
    );
  }
}
