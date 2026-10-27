// Slide scene model + fromJson (owner: SlideView agent).
//
// Pure data. The JSON shape is `core/harbor_render/src/scene.rs`; the
// contract is `docs/operations/slide-scene-contract.md`. Parsing is
// deliberately tolerant: a missing optional field takes a sensible default
// and an unknown enum string is kept as authored (painters fall back), so a
// scene produced by a newer core never throws here.
import 'dart:ui' show Color;

// ---------------------------------------------------------------------------
// Tolerant JSON readers

double _d(Object? v, [double fallback = 0]) {
  if (v is num) return v.toDouble();
  if (v is String) return double.tryParse(v) ?? fallback;
  return fallback;
}

double? _dn(Object? v) {
  if (v is num) return v.toDouble();
  if (v is String) return double.tryParse(v);
  return null;
}

int _i(Object? v, [int fallback = 0]) {
  if (v is num) return v.toInt();
  if (v is String) return int.tryParse(v) ?? fallback;
  return fallback;
}

int? _in(Object? v) {
  if (v is num) return v.toInt();
  if (v is String) return int.tryParse(v);
  return null;
}

bool _b(Object? v, [bool fallback = false]) => v is bool ? v : fallback;

String _s(Object? v, [String fallback = '']) => v is String ? v : fallback;

String? _sn(Object? v) => v is String ? v : null;

Map<String, dynamic>? _m(Object? v) {
  if (v is Map<String, dynamic>) return v;
  if (v is Map) return v.map((k, val) => MapEntry(k.toString(), val));
  return null;
}

List<Map<String, dynamic>> _ml(Object? v) {
  if (v is! List) return const [];
  final out = <Map<String, dynamic>>[];
  for (final e in v) {
    final m = _m(e);
    if (m != null) out.add(m);
  }
  return out;
}

List<double> _dl(Object? v, [int? length, double fill = 0]) {
  final out = <double>[];
  if (v is List) {
    for (final e in v) {
      out.add(_d(e));
    }
  }
  if (length != null) {
    while (out.length < length) {
      out.add(fill);
    }
  }
  return out;
}

/// Parses `#RRGGBB` / `RRGGBB` / `#AARRGGBB` into a [Color] with [alpha]
/// (0..1) multiplied in. Garbage becomes [fallback].
Color parseSceneColor(String? hex, double alpha,
    [Color fallback = const Color(0xFF000000)]) {
  var h = (hex ?? '').trim();
  if (h.startsWith('#')) h = h.substring(1);
  int? rgb;
  if (h.length == 6) {
    rgb = int.tryParse(h, radix: 16);
  } else if (h.length == 8) {
    final v = int.tryParse(h, radix: 16);
    rgb = v == null ? null : (v & 0xFFFFFF);
  } else if (h.length == 3) {
    final v = int.tryParse(h, radix: 16);
    if (v != null) {
      final r = (v >> 8) & 0xF, g = (v >> 4) & 0xF, b = v & 0xF;
      rgb = (r * 17 << 16) | (g * 17 << 8) | (b * 17);
    }
  }
  if (rgb == null) return fallback;
  final a = (alpha.isNaN ? 1.0 : alpha.clamp(0.0, 1.0)).toDouble();
  return Color(0xFF000000 | rgb).withValues(alpha: a);
}

// ---------------------------------------------------------------------------
// Deck / slide

class DeckScene {
  const DeckScene({
    required this.width,
    required this.height,
    required this.slides,
  });

  /// Slide size in EMU.
  final double width, height;
  final List<SlideScene> slides;

  factory DeckScene.fromJson(Map<String, dynamic> json) {
    final w = _d(json['width'], 12192000);
    final h = _d(json['height'], 6858000);
    return DeckScene(
      width: w > 0 ? w : 12192000,
      height: h > 0 ? h : 6858000,
      slides: _ml(json['slides']).map(SlideScene.fromJson).toList(),
    );
  }
}

class SlideScene {
  const SlideScene({
    required this.index,
    this.background = ScenePaint.none,
    this.items = const [],
    this.transition,
    this.animations = const [],
    this.notes,
    this.unsupported = const [],
  });

  /// 1-based position in presentation order.
  final int index;
  final ScenePaint background;
  final List<SceneItem> items;
  final SceneTransition? transition;
  final List<SceneAnimation> animations;
  final String? notes;
  final List<String> unsupported;

  factory SlideScene.fromJson(Map<String, dynamic> json) {
    final t = _m(json['transition']);
    return SlideScene(
      index: _i(json['index'], 1),
      background: ScenePaint.fromJson(_m(json['background'])),
      items: _ml(json['items'])
          .map(SceneItem.fromJson)
          .whereType<SceneItem>()
          .toList(),
      transition: t == null ? null : SceneTransition.fromJson(t),
      animations: _ml(json['animations']).map(SceneAnimation.fromJson).toList(),
      notes: _sn(json['notes']),
      unsupported: [
        for (final u in (json['unsupported'] is List
            ? json['unsupported'] as List
            : const []))
          if (u is String) u,
      ],
    );
  }

  /// Items by shape id (first wins; ids are unique per slide in practice).
  SceneItem? itemById(int id) {
    for (final it in items) {
      if (it.id == id) return it;
    }
    return null;
  }

  /// Plain text of the slide in paint order (used for the default
  /// accessibility label).
  String get plainText {
    final b = StringBuffer();
    for (final it in items) {
      if (it is SceneShape && it.text != null) {
        final t = it.text!.plainText;
        if (t.isNotEmpty) {
          if (b.isNotEmpty) b.write('. ');
          b.write(t);
        }
      } else if (it is SceneChart && it.chart.title != null) {
        if (b.isNotEmpty) b.write('. ');
        b.write(it.chart.title);
      } else if (it.alt != null && it.alt!.isNotEmpty) {
        if (b.isNotEmpty) b.write('. ');
        b.write(it.alt);
      }
    }
    return b.toString();
  }
}

// ---------------------------------------------------------------------------
// Items

/// Items share the flattened `Frame`: placement is the top-left of the
/// unrotated box; `rot` (degrees clockwise) turns about its centre; flips
/// apply before rotation. Geometry in EMU.
sealed class SceneItem {
  const SceneItem({
    required this.id,
    this.name = '',
    this.alt,
    required this.x,
    required this.y,
    required this.w,
    required this.h,
    this.rot = 0,
    this.flipH = false,
    this.flipV = false,
  });

  final int id;
  final String name;
  final String? alt;
  final double x, y, w, h, rot;
  final bool flipH, flipV;

  /// Null for an unknown `t` (the item is skipped, never thrown on).
  static SceneItem? fromJson(Map<String, dynamic> json) {
    switch (_s(json['t'])) {
      case 'shape':
        return SceneShape.fromJson(json);
      case 'picture':
        return ScenePicture.fromJson(json);
      case 'chart':
        return SceneChart.fromJson(json);
      case 'table':
        return SceneTable.fromJson(json);
    }
    return null;
  }
}

class SceneShape extends SceneItem {
  const SceneShape({
    required super.id,
    super.name,
    super.alt,
    required super.x,
    required super.y,
    required super.w,
    required super.h,
    super.rot,
    super.flipH,
    super.flipV,
    this.geom = const PresetGeom('rect'),
    this.fill = ScenePaint.none,
    this.line,
    this.shadow,
    this.text,
  });

  final SceneGeom geom;
  final ScenePaint fill;
  final SceneLine? line;
  final SceneShadow? shadow;
  final SceneTextBody? text;

  factory SceneShape.fromJson(Map<String, dynamic> j) {
    final line = _m(j['line']);
    final shadow = _m(j['shadow']);
    final text = _m(j['text']);
    return SceneShape(
      id: _i(j['id']),
      name: _s(j['name']),
      alt: _sn(j['alt']),
      x: _d(j['x']),
      y: _d(j['y']),
      w: _d(j['w']),
      h: _d(j['h']),
      rot: _d(j['rot']),
      flipH: _b(j['flip_h']),
      flipV: _b(j['flip_v']),
      geom: SceneGeom.fromJson(_m(j['geom'])),
      fill: ScenePaint.fromJson(_m(j['fill'])),
      line: line == null ? null : SceneLine.fromJson(line),
      shadow: shadow == null ? null : SceneShadow.fromJson(shadow),
      text: text == null ? null : SceneTextBody.fromJson(text),
    );
  }
}

class ScenePicture extends SceneItem {
  const ScenePicture({
    required super.id,
    super.name,
    super.alt,
    required super.x,
    required super.y,
    required super.w,
    required super.h,
    super.rot,
    super.flipH,
    super.flipV,
    this.mime = '',
    this.dataB64 = '',
    this.crop = const [0, 0, 0, 0],
    this.geom = const PresetGeom('rect'),
    this.line,
    this.shadow,
  });

  final String mime;

  /// Standard base64 of the encoded image; empty when it was too large to
  /// carry.
  final String dataB64;

  /// Source crop fractions `[left, top, right, bottom]`.
  final List<double> crop;
  final SceneGeom geom;
  final SceneLine? line;
  final SceneShadow? shadow;

  factory ScenePicture.fromJson(Map<String, dynamic> j) {
    final line = _m(j['line']);
    final shadow = _m(j['shadow']);
    return ScenePicture(
      id: _i(j['id']),
      name: _s(j['name']),
      alt: _sn(j['alt']),
      x: _d(j['x']),
      y: _d(j['y']),
      w: _d(j['w']),
      h: _d(j['h']),
      rot: _d(j['rot']),
      flipH: _b(j['flip_h']),
      flipV: _b(j['flip_v']),
      mime: _s(j['mime']),
      dataB64: _s(j['data_b64']),
      crop: _dl(j['crop'], 4),
      geom: SceneGeom.fromJson(_m(j['geom'])),
      line: line == null ? null : SceneLine.fromJson(line),
      shadow: shadow == null ? null : SceneShadow.fromJson(shadow),
    );
  }
}

class SceneChart extends SceneItem {
  const SceneChart({
    required super.id,
    super.name,
    super.alt,
    required super.x,
    required super.y,
    required super.w,
    required super.h,
    super.rot,
    super.flipH,
    super.flipV,
    required this.chart,
  });

  final ChartData chart;

  factory SceneChart.fromJson(Map<String, dynamic> j) => SceneChart(
        id: _i(j['id']),
        name: _s(j['name']),
        alt: _sn(j['alt']),
        x: _d(j['x']),
        y: _d(j['y']),
        w: _d(j['w']),
        h: _d(j['h']),
        rot: _d(j['rot']),
        flipH: _b(j['flip_h']),
        flipV: _b(j['flip_v']),
        chart: ChartData.fromJson(_m(j['chart']) ?? const {}),
      );
}

class SceneTable extends SceneItem {
  const SceneTable({
    required super.id,
    super.name,
    super.alt,
    required super.x,
    required super.y,
    required super.w,
    required super.h,
    super.rot,
    super.flipH,
    super.flipV,
    this.cols = const [],
    this.rows = const [],
  });

  /// Column widths in EMU.
  final List<double> cols;
  final List<SceneTableRow> rows;

  factory SceneTable.fromJson(Map<String, dynamic> j) => SceneTable(
        id: _i(j['id']),
        name: _s(j['name']),
        alt: _sn(j['alt']),
        x: _d(j['x']),
        y: _d(j['y']),
        w: _d(j['w']),
        h: _d(j['h']),
        rot: _d(j['rot']),
        flipH: _b(j['flip_h']),
        flipV: _b(j['flip_v']),
        cols: _dl(j['cols']),
        rows: _ml(j['rows']).map(SceneTableRow.fromJson).toList(),
      );
}

class SceneTableRow {
  const SceneTableRow({required this.h, this.cells = const []});

  /// Row height in EMU.
  final double h;
  final List<SceneTableCell> cells;

  factory SceneTableRow.fromJson(Map<String, dynamic> j) => SceneTableRow(
        h: _d(j['h']),
        cells: _ml(j['cells']).map(SceneTableCell.fromJson).toList(),
      );
}

class SceneTableCell {
  const SceneTableCell({
    required this.text,
    this.fill = ScenePaint.none,
    this.borders = const [null, null, null, null],
    this.gridSpan = 1,
    this.rowSpan = 1,
    this.mergedAway = false,
  });

  final SceneTextBody text;
  final ScenePaint fill;

  /// `[left, top, right, bottom]`.
  final List<SceneLine?> borders;
  final int gridSpan, rowSpan;
  final bool mergedAway;

  factory SceneTableCell.fromJson(Map<String, dynamic> j) {
    final b = j['borders'];
    final borders = <SceneLine?>[null, null, null, null];
    if (b is List) {
      for (var i = 0; i < 4 && i < b.length; i++) {
        final m = _m(b[i]);
        borders[i] = m == null ? null : SceneLine.fromJson(m);
      }
    }
    return SceneTableCell(
      text: SceneTextBody.fromJson(_m(j['text']) ?? const {}),
      fill: ScenePaint.fromJson(_m(j['fill'])),
      borders: borders,
      gridSpan: _i(j['grid_span'], 1).clamp(1, 1000),
      rowSpan: _i(j['row_span'], 1).clamp(1, 1000),
      mergedAway: _b(j['merged_away']),
    );
  }
}

// ---------------------------------------------------------------------------
// Geometry and paint

sealed class SceneGeom {
  const SceneGeom();

  factory SceneGeom.fromJson(Map<String, dynamic>? j) {
    if (j == null) return const PresetGeom('rect');
    if (_s(j['k']) == 'path') {
      return PathGeom(
        w: _d(j['w']),
        h: _d(j['h']),
        cmds:
            _ml(j['cmds']).map(PathCmd.fromJson).whereType<PathCmd>().toList(),
      );
    }
    return PresetGeom(_s(j['name'], 'rect'), _dl(j['adj']));
  }
}

class PresetGeom extends SceneGeom {
  const PresetGeom(this.name, [this.adj = const []]);
  final String name;

  /// Adjust values in OOXML units (roundRect default `[16667]`).
  final List<double> adj;
}

class PathGeom extends SceneGeom {
  const PathGeom({required this.w, required this.h, required this.cmds});

  /// The path's own coordinate space.
  final double w, h;
  final List<PathCmd> cmds;
}

/// One custom-path command. [op] is `M`, `L`, `C`, `Q` or `Z`.
class PathCmd {
  const PathCmd(this.op,
      {this.x = 0,
      this.y = 0,
      this.x1 = 0,
      this.y1 = 0,
      this.x2 = 0,
      this.y2 = 0});
  final String op;
  final double x, y, x1, y1, x2, y2;

  static PathCmd? fromJson(Map<String, dynamic> j) {
    final op = _s(j['c']);
    if (op != 'M' && op != 'L' && op != 'C' && op != 'Q' && op != 'Z') {
      return null;
    }
    return PathCmd(
      op,
      x: _d(j['x']),
      y: _d(j['y']),
      x1: _d(j['x1']),
      y1: _d(j['y1']),
      x2: _d(j['x2']),
      y2: _d(j['y2']),
    );
  }
}

enum PaintKind { none, solid, linear, radial }

class SceneStop {
  const SceneStop(this.p, this.c, this.a);

  /// Position 0..1.
  final double p;
  final String c;
  final double a;

  Color get color => parseSceneColor(c, a);

  factory SceneStop.fromJson(Map<String, dynamic> j) =>
      SceneStop(_d(j['p']), _s(j['c'], '#000000'), _d(j['a'], 1));
}

class ScenePaint {
  const ScenePaint({
    this.kind = PaintKind.none,
    this.c = '#000000',
    this.a = 1,
    this.angle = 0,
    this.cx = 0.5,
    this.cy = 0.5,
    this.stops = const [],
  });

  static const ScenePaint none = ScenePaint();

  final PaintKind kind;

  /// Solid colour and alpha.
  final String c;
  final double a;

  /// Linear: degrees clockwise from +x (0 = left→right, 90 = top→bottom).
  final double angle;

  /// Radial centre as fractions of the frame.
  final double cx, cy;
  final List<SceneStop> stops;

  bool get isNone => kind == PaintKind.none;

  /// The solid colour with alpha applied.
  Color get color => parseSceneColor(c, a);

  factory ScenePaint.fromJson(Map<String, dynamic>? j) {
    if (j == null) return none;
    List<SceneStop> stops() => _ml(j['stops']).map(SceneStop.fromJson).toList();
    switch (_s(j['k'])) {
      case 'solid':
        return ScenePaint(
            kind: PaintKind.solid, c: _s(j['c'], '#000000'), a: _d(j['a'], 1));
      case 'linear':
        final s = stops();
        if (s.isEmpty) return none;
        return ScenePaint(
            kind: PaintKind.linear, angle: _d(j['angle']), stops: s);
      case 'radial':
        final s = stops();
        if (s.isEmpty) return none;
        return ScenePaint(
            kind: PaintKind.radial,
            cx: _d(j['cx'], 0.5),
            cy: _d(j['cy'], 0.5),
            stops: s);
    }
    return none;
  }
}

class SceneLine {
  const SceneLine({
    required this.w,
    this.paint = ScenePaint.none,
    this.dash = 'solid',
    this.cap = 'flat',
    this.head,
    this.tail,
  });

  /// Width in EMU.
  final double w;
  final ScenePaint paint;

  /// `solid dash dot dashDot lgDash sysDash sysDot`.
  final String dash;

  /// `flat round square`.
  final String cap;

  /// Arrowhead names at the start / end of the line.
  final String? head, tail;

  factory SceneLine.fromJson(Map<String, dynamic> j) => SceneLine(
        w: _d(j['w'], 12700),
        paint: ScenePaint.fromJson(_m(j['paint'])),
        dash: _s(j['dash'], 'solid'),
        cap: _s(j['cap'], 'flat'),
        head: _sn(j['head']),
        tail: _sn(j['tail']),
      );
}

class SceneShadow {
  const SceneShadow({
    this.dx = 0,
    this.dy = 0,
    this.blur = 0,
    this.c = '#000000',
    this.a = 0.4,
  });
  final double dx, dy, blur;
  final String c;
  final double a;

  Color get color => parseSceneColor(c, a);

  factory SceneShadow.fromJson(Map<String, dynamic> j) => SceneShadow(
        dx: _d(j['dx']),
        dy: _d(j['dy']),
        blur: _d(j['blur']),
        c: _s(j['c'], '#000000'),
        a: _d(j['a'], 0.4),
      );
}

// ---------------------------------------------------------------------------
// Text

class SceneTextBody {
  const SceneTextBody({
    this.ins = const [91440, 45720, 91440, 45720],
    this.anchor = 't',
    this.wrap = true,
    this.autofit = 'none',
    this.fontScale = 1,
    this.lineReduction = 0,
    this.paras = const [],
  });

  /// Insets `[left, top, right, bottom]` in EMU.
  final List<double> ins;

  /// `t ctr b`.
  final String anchor;
  final bool wrap;

  /// `none shrink resize`.
  final String autofit;
  final double fontScale, lineReduction;
  final List<SceneParagraph> paras;

  factory SceneTextBody.fromJson(Map<String, dynamic> j) {
    final ins = _dl(j['ins']);
    return SceneTextBody(
      ins: ins.length >= 4
          ? ins.sublist(0, 4)
          : const [91440, 45720, 91440, 45720],
      anchor: _s(j['anchor'], 't'),
      wrap: _b(j['wrap'], true),
      autofit: _s(j['autofit'], 'none'),
      fontScale: () {
        final f = _d(j['font_scale'], 1);
        return f > 0 ? f : 1.0;
      }(),
      lineReduction: _d(j['line_reduction']).clamp(0.0, 0.9).toDouble(),
      paras: _ml(j['paras']).map(SceneParagraph.fromJson).toList(),
    );
  }

  /// Text with paragraphs joined by newlines (soft breaks as spaces).
  String get plainText => paras
      .map((p) => p.plainText)
      .where((t) => t.trim().isNotEmpty)
      .join('. ');

  bool get isEmpty => paras.every((p) => p.runs.every((r) => r.t.isEmpty));
}

class SceneParagraph {
  const SceneParagraph({
    this.algn = 'l',
    this.rtl = false,
    this.marL = 0,
    this.indent = 0,
    this.bullet,
    this.spcBef = 0,
    this.spcAft = 0,
    this.lnPct,
    this.lnPts,
    this.runs = const [],
    this.endSz = 18,
  });

  /// Absolute: `l ctr r just`.
  final String algn;
  final bool rtl;

  /// Left margin and first-line indent in EMU (from the right for [rtl]).
  final double marL, indent;
  final SceneBullet? bullet;

  /// Spacing before / after, points.
  final double spcBef, spcAft;

  /// Line spacing multiple, or exact points when [lnPts] is set.
  final double? lnPct, lnPts;
  final List<SceneRun> runs;
  final double endSz;

  factory SceneParagraph.fromJson(Map<String, dynamic> j) {
    final bullet = _m(j['bullet']);
    return SceneParagraph(
      algn: _s(j['algn'], 'l'),
      rtl: _b(j['rtl']),
      marL: _d(j['mar_l']),
      indent: _d(j['indent']),
      bullet: bullet == null ? null : SceneBullet.fromJson(bullet),
      spcBef: _d(j['spc_bef']),
      spcAft: _d(j['spc_aft']),
      lnPct: _dn(j['ln_pct']),
      lnPts: _dn(j['ln_pts']),
      runs: _ml(j['runs']).map(SceneRun.fromJson).toList(),
      endSz: _d(j['end_sz'], 18),
    );
  }

  String get plainText => runs.map((r) => r.t == '\n' ? ' ' : r.t).join();
}

class SceneBullet {
  const SceneBullet({
    this.kind = 'char',
    this.ch = '•',
    this.numType = '',
    this.start = 1,
    this.c,
    this.szPct = 1,
    this.font,
  });

  /// `char` or `num`.
  final String kind;
  final String ch;

  /// `arabicPeriod arabicParenR romanUcPeriod alphaLcPeriod ...`
  final String numType;
  final int start;
  final String? c;
  final double szPct;
  final String? font;

  bool get isNumbered => kind == 'num';

  factory SceneBullet.fromJson(Map<String, dynamic> j) => SceneBullet(
        kind: _s(j['kind'], 'char'),
        ch: _s(j['ch'], '•'),
        numType: _s(j['num_type']),
        start: _i(j['start'], 1),
        c: _sn(j['c']),
        szPct: () {
          final v = _d(j['sz_pct'], 1);
          return v > 0 ? v : 1.0;
        }(),
        font: _sn(j['font']),
      );
}

class SceneRun {
  const SceneRun({
    required this.t,
    this.sz = 18,
    this.b = false,
    this.i = false,
    this.u = false,
    this.strike = false,
    this.c = '#000000',
    this.a = 1,
    this.font = '',
    this.cs,
    this.baseline = 0,
    this.caps = false,
    this.spc = 0,
  });

  final String t;

  /// Points.
  final double sz;
  final bool b, i, u, strike;
  final String c;
  final double a;
  final String font;
  final String? cs;

  /// Percent: positive = superscript, negative = subscript.
  final double baseline;
  final bool caps;

  /// Extra character spacing in points.
  final double spc;

  bool get isBreak => t == '\n';

  Color get color => parseSceneColor(c, a);

  factory SceneRun.fromJson(Map<String, dynamic> j) => SceneRun(
        t: _s(j['t']),
        sz: () {
          final v = _d(j['sz'], 18);
          return v > 0 ? v : 18.0;
        }(),
        b: _b(j['b']),
        i: _b(j['i']),
        u: _b(j['u']),
        strike: _b(j['strike']),
        c: _s(j['c'], '#000000'),
        a: _d(j['a'], 1),
        font: _s(j['font']),
        cs: _sn(j['cs']),
        baseline: _d(j['baseline']),
        caps: _b(j['caps']),
        spc: _d(j['spc']),
      );
}

// ---------------------------------------------------------------------------
// Charts

class ChartData {
  const ChartData({
    this.kind = 'column',
    this.title,
    this.categories = const [],
    this.series = const [],
    this.stacked = false,
    this.percentStacked = false,
    this.legend = 'none',
    this.dataLabels = false,
    this.axisMin,
    this.axisMax,
    this.showGridlines = true,
    this.textColor = '#404040',
    this.gapWidth = 0.5,
    this.smooth = false,
    this.hole = 0.5,
  });

  /// `bar` (horizontal) `column line area pie doughnut scatter`.
  final String kind;
  final String? title;
  final List<String> categories;
  final List<ChartSeries> series;
  final bool stacked, percentStacked;

  /// `none t b l r`.
  final String legend;
  final bool dataLabels;
  final double? axisMin, axisMax;
  final bool showGridlines;
  final String textColor;

  /// Gap between category clusters as a fraction of the bar width.
  final double gapWidth;
  final bool smooth;

  /// Pie/doughnut hole size as a fraction of the radius.
  final double hole;

  factory ChartData.fromJson(Map<String, dynamic> j) => ChartData(
        kind: _s(j['kind'], 'column'),
        title: _sn(j['title']),
        categories: [
          for (final c
              in (j['categories'] is List ? j['categories'] as List : const []))
            c?.toString() ?? '',
        ],
        series: _ml(j['series']).map(ChartSeries.fromJson).toList(),
        stacked: _b(j['stacked']),
        percentStacked: _b(j['percent_stacked']),
        legend: _s(j['legend'], 'none'),
        dataLabels: _b(j['data_labels']),
        axisMin: _dn(j['axis_min']),
        axisMax: _dn(j['axis_max']),
        showGridlines: _b(j['show_gridlines'], true),
        textColor: _s(j['text_color'], '#404040'),
        gapWidth: _d(j['gap_width'], 0.5),
        smooth: _b(j['smooth']),
        hole: _d(j['hole'], 0.5).clamp(0.0, 0.95).toDouble(),
      );
}

class ChartSeries {
  const ChartSeries({
    this.name = '',
    this.values = const [],
    this.x,
    this.color = '#4472C4',
    this.pointColors = const [],
  });
  final String name;
  final List<double> values;

  /// X values (scatter only).
  final List<double>? x;
  final String color;
  final List<String> pointColors;

  factory ChartSeries.fromJson(Map<String, dynamic> j) => ChartSeries(
        name: _s(j['name']),
        values: _dl(j['values']),
        x: j['x'] is List ? _dl(j['x']) : null,
        color: _s(j['color'], '#4472C4'),
        pointColors: [
          for (final c in (j['point_colors'] is List
              ? j['point_colors'] as List
              : const []))
            if (c is String) c,
        ],
      );
}

// ---------------------------------------------------------------------------
// Motion

class SceneTransition {
  const SceneTransition({
    required this.kind,
    this.dir,
    this.durMs = 500,
    this.onClick = true,
    this.afterMs,
  });

  /// Normalised name (`fade push wipe ...`); unknown names fall back to
  /// `fade` in the player.
  final String kind;

  /// `l r u d lu ru ld rd in out horz vert`.
  final String? dir;
  final int durMs;
  final bool onClick;
  final int? afterMs;

  factory SceneTransition.fromJson(Map<String, dynamic> j) => SceneTransition(
        kind: _s(j['kind'], 'fade'),
        dir: _sn(j['dir']),
        durMs: _i(j['dur_ms'], 500),
        onClick: _b(j['on_click'], true),
        afterMs: _in(j['after_ms']),
      );
}

class SceneAnimation {
  const SceneAnimation({
    required this.shapeId,
    this.para,
    this.effect = 'fade',
    this.phase = 'entrance',
    this.dir,
    this.trigger = 'click',
    this.delayMs = 0,
    this.durMs = 500,
  });

  final int shapeId;

  /// Paragraph range `[first, last]` (0-based, inclusive), or null.
  final List<int>? para;

  /// `appear fade fly float wipe zoom split wheel grow spin pulse other`.
  final String effect;

  /// `entrance exit emphasis`.
  final String phase;
  final String? dir;

  /// `click with_prev after_prev`.
  final String trigger;
  final int delayMs, durMs;

  factory SceneAnimation.fromJson(Map<String, dynamic> j) {
    List<int>? para;
    final p = j['para'];
    if (p is List && p.length >= 2) {
      para = [_i(p[0]), _i(p[1])];
    }
    return SceneAnimation(
      shapeId: _i(j['shape_id']),
      para: para,
      effect: _s(j['effect'], 'fade'),
      phase: _s(j['phase'], 'entrance'),
      dir: _sn(j['dir']),
      trigger: _s(j['trigger'], 'click'),
      delayMs: _i(j['delay_ms']),
      durMs: _i(j['dur_ms'], 500),
    );
  }
}
