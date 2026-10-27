// Chart painting (owner: SlideView agent).
//
// Paints a [ChartData] into a rectangle with the conventions of a modern,
// restrained chart: thin axes, light gridlines, no chartjunk. Kinds:
// column, bar (horizontal), line, area, pie, doughnut, scatter.
//
// The painter is a pure function of (data, rect, fontPx): text painters are
// cached per chart so repainting at 60 fps while other shapes animate does
// not re-shape labels.
import 'dart:math' as math;

import 'package:flutter/painting.dart';

import 'slide_geometry.dart';
import 'slide_scene.dart';

/// Office default accents, used when a series has no (parsable) colour.
const List<String> kChartPalette = [
  '#4472C4',
  '#ED7D31',
  '#A5A5A5',
  '#FFC000',
  '#5B9BD5',
  '#70AD47',
  '#264478',
  '#9E480E',
];

/// A "nice" value axis: [min]..[max] in steps of [step].
class AxisScale {
  const AxisScale(this.min, this.max, this.step);
  final double min, max, step;

  List<double> get ticks {
    final out = <double>[];
    if (step <= 0 || !step.isFinite) return [min, max];
    final n = ((max - min) / step).round();
    for (var i = 0; i <= n && i < 200; i++) {
      out.add(_clean(min + i * step, step));
    }
    return out;
  }
}

double _clean(double v, double step) {
  final digits = step >= 1 ? 0 : (-(math.log(step) / math.ln10)).ceil() + 1;
  return double.parse(v.toStringAsFixed(digits.clamp(0, 10)));
}

/// Nice-number axis covering [lo]..[hi]. [fixedMin]/[fixedMax] pin an end.
AxisScale niceAxisScale(double lo, double hi,
    {int targetTicks = 5, double? fixedMin, double? fixedMax}) {
  if (!lo.isFinite) lo = 0;
  if (!hi.isFinite) hi = lo + 1;
  if (fixedMin != null) lo = fixedMin;
  if (fixedMax != null) hi = fixedMax;
  if (hi < lo) {
    final t = hi;
    hi = lo;
    lo = t;
  }
  if (hi - lo < 1e-12) {
    if (lo == 0) {
      hi = 1;
    } else {
      hi = lo + lo.abs() * 0.5;
      lo = lo - lo.abs() * 0.5;
      if (fixedMin != null) lo = fixedMin;
    }
  }
  final range = hi - lo;
  final raw = range / math.max(1, targetTicks);
  final mag = math.pow(10, (math.log(raw) / math.ln10).floorToDouble());
  final norm = raw / mag;
  final nice = norm <= 1
      ? 1.0
      : norm <= 2
          ? 2.0
          : norm <= 5
              ? 5.0
              : 10.0;
  final step = nice * mag.toDouble();
  final min = fixedMin ?? (lo / step).floorToDouble() * step;
  final max = fixedMax ?? (hi / step).ceilToDouble() * step;
  return AxisScale(_clean(min, step), _clean(max, step), step);
}

/// Formats an axis/data value: grouping for thousands, decimals only when
/// needed. [step] (axis ticks) fixes the decimals so labels align.
String formatChartNumber(double v, {double? step}) {
  if (!v.isFinite) return '';
  int decimals;
  if (step != null && step > 0) {
    decimals =
        step >= 1 ? 0 : (-(math.log(step) / math.ln10)).ceil().clamp(0, 6);
    if (step >= 1 && (v - v.roundToDouble()).abs() > 1e-9) decimals = 1;
  } else {
    decimals = (v - v.roundToDouble()).abs() < 1e-9 ? 0 : 2;
  }
  var s = v.abs().toStringAsFixed(decimals);
  if (decimals > 0 && step == null) {
    s = s.replaceFirst(RegExp(r'0+$'), '').replaceFirst(RegExp(r'\.$'), '');
  }
  final parts = s.split('.');
  final ip = parts[0];
  final buf = StringBuffer();
  for (var i = 0; i < ip.length; i++) {
    if (i > 0 && (ip.length - i) % 3 == 0 && ip.length > 4) buf.write(',');
    buf.write(ip[i]);
  }
  final out = parts.length > 1 ? '$buf.${parts[1]}' : buf.toString();
  return (v < 0 && out.replaceAll(RegExp(r'[0.,]'), '').isNotEmpty ? '-' : '') +
      out;
}

/// The colour of series [i] (or of its point [point]).
Color chartSeriesColor(ChartData d, int i, [int? point]) {
  final fallback = parseSceneColor(kChartPalette[i % kChartPalette.length], 1);
  if (i >= d.series.length) return fallback;
  final s = d.series[i];
  if (point != null && point < s.pointColors.length) {
    return parseSceneColor(s.pointColors[point], 1, fallback);
  }
  return parseSceneColor(s.color, 1, fallback);
}

/// Paints [data] into [rect] on [canvas]. [fontPx] is the base (axis) font
/// size in pixels; strokes scale with it.
void paintChart(Canvas canvas, Rect rect, ChartData data,
    {required double fontPx}) {
  if (rect.width < 8 || rect.height < 8) return;
  _ChartPainter(canvas, rect, data, math.max(fontPx, 4)).paint();
}

// ---------------------------------------------------------------------------

final Expando<Map<String, TextPainter>> _textCache = Expando('chart text');

class _ChartPainter {
  _ChartPainter(this.canvas, this.rect, this.d, this.fs)
      : textColor = parseSceneColor(d.textColor, 1, const Color(0xFF404040));

  final Canvas canvas;
  final Rect rect;
  final ChartData d;
  final double fs;
  final Color textColor;

  int get _seriesCount => d.series.length;
  int get _n {
    var n = d.categories.length;
    for (final s in d.series) {
      n = math.max(n, s.values.length);
    }
    return n;
  }

  Color get _grid => textColor.withValues(alpha: 0.14);
  Color get _axis => textColor.withValues(alpha: 0.4);
  double get _hair => math.max(0.75, fs / 12);

  TextPainter _tp(String text, double size,
      {Color? color,
      bool bold = false,
      double maxWidth = double.infinity,
      TextAlign align = TextAlign.left}) {
    final cache = _textCache[d] ??= <String, TextPainter>{};
    if (cache.length > 400) {
      for (final p in cache.values) {
        p.dispose();
      }
      cache.clear();
    }
    final c = color ?? textColor;
    final mw = maxWidth.isFinite ? maxWidth.roundToDouble() : -1;
    final key =
        '$text|${size.toStringAsFixed(2)}|${c.toARGB32()}|$bold|$mw|${align.index}';
    final hit = cache[key];
    if (hit != null) return hit;
    final font = resolveSlideFont(null, complexScript: hasComplexScript(text));
    final tp = TextPainter(
      text: TextSpan(
        text: text,
        style: TextStyle(
          fontSize: size,
          color: c,
          fontWeight: bold ? FontWeight.w600 : FontWeight.w400,
          fontFamilyFallback: font.fallback,
          height: 1.15,
        ),
      ),
      textDirection:
          hasComplexScript(text) ? TextDirection.rtl : TextDirection.ltr,
      textAlign: align,
      maxLines: 1,
      ellipsis: '…',
    )..layout(
        maxWidth: maxWidth.isFinite ? math.max(maxWidth, 1) : double.infinity);
    cache[key] = tp;
    return tp;
  }

  void _text(TextPainter tp, Offset topLeft) => tp.paint(canvas, topLeft);

  void paint() {
    canvas.save();
    canvas.clipRect(rect);
    var area = rect.deflate(fs * 0.5);

    if (d.title != null && d.title!.trim().isNotEmpty) {
      final tp = _tp(d.title!, fs * 1.25,
          bold: true, maxWidth: area.width, align: TextAlign.center);
      _text(tp, Offset(area.center.dx - tp.width / 2, area.top));
      area = Rect.fromLTRB(
          area.left, area.top + tp.height + fs * 0.6, area.right, area.bottom);
    }
    area = _layoutLegend(area);
    if (area.width > 4 && area.height > 4 && _seriesCount > 0) {
      switch (d.kind) {
        case 'pie':
        case 'doughnut':
          _paintPie(area);
        case 'scatter':
          _paintScatter(area);
        case 'line':
        case 'area':
        case 'bar':
        case 'column':
        default:
          _paintCartesian(area);
      }
    } else if (_seriesCount == 0) {
      final tp = _tp('No data', fs, color: textColor.withValues(alpha: 0.5));
      _text(
          tp,
          Offset(
              rect.center.dx - tp.width / 2, rect.center.dy - tp.height / 2));
    }
    canvas.restore();
  }

  // -- legend ---------------------------------------------------------------

  bool get _isPie => d.kind == 'pie' || d.kind == 'doughnut';

  List<(String, Color)> get _legendEntries {
    if (_isPie && _seriesCount > 0) {
      final count = math.max(d.categories.length, d.series.first.values.length);
      return [
        for (var i = 0; i < count; i++)
          (
            i < d.categories.length ? d.categories[i] : '${i + 1}',
            _sliceColor(i),
          ),
      ];
    }
    return [
      for (var i = 0; i < _seriesCount; i++)
        (
          d.series[i].name.isEmpty ? 'Series ${i + 1}' : d.series[i].name,
          chartSeriesColor(d, i)
        ),
    ];
  }

  Color _sliceColor(int i) {
    final s = d.series.first;
    if (i < s.pointColors.length) {
      return parseSceneColor(s.pointColors[i], 1,
          parseSceneColor(kChartPalette[i % kChartPalette.length], 1));
    }
    // A single colour for every slice would be unreadable: vary the palette.
    return parseSceneColor(kChartPalette[i % kChartPalette.length], 1);
  }

  Rect _layoutLegend(Rect area) {
    final pos = d.legend;
    if (pos != 't' && pos != 'b' && pos != 'l' && pos != 'r') return area;
    final entries = _legendEntries;
    if (entries.isEmpty) return area;
    if (!_isPie &&
        _seriesCount == 1 &&
        entries.first.$1.startsWith('Series ')) {
      return area;
    }
    final size = fs * 0.95;
    final sw = size * 0.7;
    final gap = size * 0.45;
    final pad = size * 1.1;
    final tps = [
      for (final e in entries)
        _tp(e.$1, size,
            maxWidth: (pos == 'l' || pos == 'r')
                ? area.width * 0.3
                : area.width * 0.5),
    ];
    final lineH = tps.map((t) => t.height).fold<double>(0, math.max);
    if (pos == 't' || pos == 'b') {
      // Wrap entries into rows.
      final rows = <List<int>>[];
      var cur = <int>[];
      var curW = 0.0;
      for (var i = 0; i < tps.length; i++) {
        final w = sw + gap + tps[i].width;
        if (cur.isNotEmpty && curW + pad + w > area.width) {
          rows.add(cur);
          cur = <int>[];
          curW = 0;
        }
        curW += (cur.isEmpty ? 0 : pad) + w;
        cur.add(i);
      }
      if (cur.isNotEmpty) rows.add(cur);
      final h = rows.length * lineH + (rows.length - 1) * gap;
      final top = pos == 't' ? area.top : area.bottom - h;
      var y = top;
      for (final row in rows) {
        var rowW = 0.0;
        for (final i in row) {
          rowW += sw + gap + tps[i].width;
        }
        rowW += (row.length - 1) * pad;
        var x = area.center.dx - rowW / 2;
        for (final i in row) {
          _swatch(
              Rect.fromLTWH(x, y + (lineH - sw) / 2, sw, sw), entries[i].$2);
          _text(tps[i], Offset(x + sw + gap, y));
          x += sw + gap + tps[i].width + pad;
        }
        y += lineH + gap;
      }
      return pos == 't'
          ? Rect.fromLTRB(
              area.left, area.top + h + fs * 0.6, area.right, area.bottom)
          : Rect.fromLTRB(
              area.left, area.top, area.right, area.bottom - h - fs * 0.6);
    }
    final colW = sw + gap + tps.map((t) => t.width).fold<double>(0, math.max);
    final h = tps.length * lineH + (tps.length - 1) * gap;
    final x = pos == 'l' ? area.left : area.right - colW;
    var y = area.center.dy - h / 2;
    for (var i = 0; i < tps.length; i++) {
      _swatch(Rect.fromLTWH(x, y + (lineH - sw) / 2, sw, sw), entries[i].$2);
      _text(tps[i], Offset(x + sw + gap, y));
      y += lineH + gap;
    }
    return pos == 'l'
        ? Rect.fromLTRB(
            area.left + colW + fs * 0.8, area.top, area.right, area.bottom)
        : Rect.fromLTRB(
            area.left, area.top, area.right - colW - fs * 0.8, area.bottom);
  }

  void _swatch(Rect r, Color c) {
    canvas.drawRRect(
        RRect.fromRectAndRadius(r, Radius.circular(r.shortestSide * 0.2)),
        Paint()..color = c);
  }

  // -- cartesian ------------------------------------------------------------

  bool get _horizontal => d.kind == 'bar';
  bool get _isBarLike => d.kind == 'bar' || d.kind == 'column';
  bool get _stack => d.stacked || d.percentStacked;

  double _v(int s, int i) {
    final vals = d.series[s].values;
    if (i >= vals.length) return 0;
    final v = vals[i];
    return v.isFinite ? v : 0;
  }

  void _paintCartesian(Rect area) {
    final n = _n;
    if (n == 0) return;
    final k = _seriesCount;

    // Stack bases per (series, category).
    final bases = List.generate(k, (_) => List<double>.filled(n, 0));
    final tops = List.generate(k, (_) => List<double>.filled(n, 0));
    var dMin = 0.0, dMax = 0.0;
    var first = true;
    void note(double v) {
      if (first) {
        dMin = v;
        dMax = v;
        first = false;
      } else {
        dMin = math.min(dMin, v);
        dMax = math.max(dMax, v);
      }
    }

    final absTotals = List<double>.generate(n, (i) {
      var t = 0.0;
      for (var s = 0; s < k; s++) {
        t += _v(s, i).abs();
      }
      return t;
    });
    for (var i = 0; i < n; i++) {
      var pos = 0.0, neg = 0.0;
      for (var s = 0; s < k; s++) {
        var v = _v(s, i);
        if (d.percentStacked) {
          v = absTotals[i] == 0 ? 0 : v / absTotals[i] * 100;
        }
        if (_stack) {
          if (v >= 0) {
            bases[s][i] = pos;
            pos += v;
            tops[s][i] = pos;
          } else {
            bases[s][i] = neg;
            neg += v;
            tops[s][i] = neg;
          }
          note(pos);
          note(neg);
        } else {
          bases[s][i] = 0;
          tops[s][i] = v;
          note(v);
        }
      }
    }
    if (first) {
      dMin = 0;
      dMax = 1;
    }

    final zeroRule = _isBarLike || d.kind == 'area' || _stack;
    double lo = dMin, hi = dMax;
    if (zeroRule) {
      lo = math.min(lo, 0);
      hi = math.max(hi, 0);
    } else if (lo >= 0 && lo < hi * 5 / 6) {
      // PowerPoint rule: a line chart starts at zero unless the data sit
      // in a narrow band far from it.
      lo = 0;
    }
    final isPct = d.percentStacked;
    final scale = isPct
        ? niceAxisScale(0, 100,
            targetTicks: 5,
            fixedMin: d.axisMin ?? 0,
            fixedMax: d.axisMax ?? 100)
        : niceAxisScale(lo, hi,
            targetTicks: _horizontal
                ? 5
                : (area.height / (fs * 3.4)).floor().clamp(3, 8),
            fixedMin: d.axisMin,
            fixedMax: d.axisMax);

    String fmtTick(double v) =>
        formatChartNumber(v, step: scale.step) + (isPct ? '%' : '');
    final tickTps = [
      for (final t in scale.ticks)
        _tp(fmtTick(t), fs, color: textColor.withValues(alpha: 0.85)),
    ];
    final tickW = tickTps.map((t) => t.width).fold<double>(0, math.max);
    final tickH = tickTps.fold<double>(0, (m, t) => math.max(m, t.height));

    // Category label measurement.
    final cats = [
      for (var i = 0; i < n; i++)
        i < d.categories.length ? d.categories[i] : '${i + 1}',
    ];
    double catLabelW(double size) => cats
        .map(
            (c) => _tp(c, size, color: textColor.withValues(alpha: 0.85)).width)
        .fold<double>(0, math.max);

    Rect plot;
    if (_horizontal) {
      final catW = math.min(catLabelW(fs), area.width * 0.32);
      // Leave room at the bar ends for data labels.
      var labelRoom = 0.0;
      if (d.dataLabels && !_stack) {
        for (final s in d.series) {
          for (final v in s.values) {
            labelRoom =
                math.max(labelRoom, _tp(formatChartNumber(v), fs * 0.88).width);
          }
        }
        labelRoom += 6;
      }
      plot = Rect.fromLTRB(
          area.left + catW + fs * 0.6,
          area.top + tickH * 0.5,
          area.right - math.max(tickW * 0.5, labelRoom),
          area.bottom - tickH - fs * 0.6);
    } else {
      plot = Rect.fromLTRB(area.left + tickW + fs * 0.7, area.top + tickH * 0.5,
          area.right - fs * 0.3, area.bottom - tickH - fs * 0.6);
    }
    if (plot.width < 8 || plot.height < 8) return;

    final span = scale.max - scale.min;
    // Value -> coordinate along the value axis.
    double vPos(double v) {
      final f = ((v - scale.min) / (span == 0 ? 1 : span)).clamp(-0.5, 1.5);
      return _horizontal
          ? plot.left + f * plot.width
          : plot.bottom - f * plot.height;
    }

    // Gridlines + value labels.
    final gridPaint = Paint()
      ..color = _grid
      ..strokeWidth = _hair
      ..style = PaintingStyle.stroke;
    for (var ti = 0; ti < scale.ticks.length; ti++) {
      final t = scale.ticks[ti];
      final p = vPos(t);
      if (d.showGridlines) {
        if (_horizontal) {
          canvas.drawLine(
              Offset(p, plot.top), Offset(p, plot.bottom), gridPaint);
        } else {
          canvas.drawLine(
              Offset(plot.left, p), Offset(plot.right, p), gridPaint);
        }
      }
      final tp = tickTps[ti];
      if (_horizontal) {
        _text(tp, Offset(p - tp.width / 2, plot.bottom + fs * 0.45));
      } else {
        _text(tp, Offset(plot.left - fs * 0.5 - tp.width, p - tp.height / 2));
      }
    }

    // Category geometry. Area charts run edge to edge; everything else sits
    // in bands.
    final edgeToEdge = d.kind == 'area' && n > 1;
    final catLen = _horizontal ? plot.height : plot.width;
    final band = edgeToEdge ? catLen / (n - 1) : catLen / n;
    // Category index -> centre coordinate along the category axis.
    double cPos(int i) {
      if (_horizontal) {
        // First category at the bottom, as PowerPoint draws bar charts.
        return edgeToEdge
            ? plot.bottom - i * band
            : plot.bottom - (i + 0.5) * band;
      }
      return edgeToEdge ? plot.left + i * band : plot.left + (i + 0.5) * band;
    }

    // Category labels.
    _paintCategoryLabels(cats, plot, band, cPos, edgeToEdge);

    final zero = vPos(0.0.clamp(scale.min, scale.max));

    // Series.
    switch (d.kind) {
      case 'line':
        _paintLines(plot, n, bases, tops, cPos, vPos);
      case 'area':
        _paintAreas(plot, n, bases, tops, cPos, vPos);
      default:
        _paintBars(plot, n, bases, tops, band, cPos, vPos, zero);
    }

    // Axis lines on top.
    final axisPaint = Paint()
      ..color = _axis
      ..strokeWidth = _hair
      ..style = PaintingStyle.stroke;
    if (_horizontal) {
      canvas.drawLine(
          Offset(zero, plot.top), Offset(zero, plot.bottom), axisPaint);
    } else {
      canvas.drawLine(
          Offset(plot.left, zero), Offset(plot.right, zero), axisPaint);
    }
  }

  void _paintCategoryLabels(List<String> cats, Rect plot, double band,
      double Function(int) cPos, bool edgeToEdge) {
    final n = cats.length;
    var size = fs;
    final col = textColor.withValues(alpha: 0.85);
    if (_horizontal) {
      final lineH = _tp('Ag', size, color: col).height;
      final skip = math.max(1, (lineH * 1.1 / band).ceil());
      final maxW =
          math.max(plot.left - (rect.left + fs * 0.5) - fs * 0.5, 10.0);
      for (var i = 0; i < n; i += skip) {
        final tp = _tp(cats[i], size,
            color: col, maxWidth: maxW, align: TextAlign.right);
        _text(tp,
            Offset(plot.left - fs * 0.5 - tp.width, cPos(i) - tp.height / 2));
      }
      return;
    }
    double widest(double sz) =>
        cats.map((c) => _tp(c, sz, color: col).width).fold<double>(0, math.max);
    var w = widest(size);
    if (w > band * 0.92) {
      size = fs * 0.85;
      w = widest(size);
    }
    var skip = 1;
    if (w > band * 0.92) skip = math.max(1, (w / (band * 0.92)).ceil());
    final maxW = band * skip * 0.95;
    for (var i = 0; i < n; i += skip) {
      final tp = _tp(cats[i], size,
          color: col, maxWidth: maxW, align: TextAlign.center);
      _text(tp, Offset(cPos(i) - tp.width / 2, plot.bottom + fs * 0.45));
    }
  }

  double get _gap {
    final g = d.gapWidth;
    // The contract states a fraction; tolerate raw OOXML percent (150).
    return (g > 10 ? g / 100 : g).clamp(0.0, 5.0);
  }

  void _paintBars(
      Rect plot,
      int n,
      List<List<double>> bases,
      List<List<double>> tops,
      double band,
      double Function(int) cPos,
      double Function(double) vPos,
      double zero) {
    final k = _seriesCount;
    final groups = _stack ? 1 : k;
    final bw = band / (groups + _gap);
    final cluster = groups * bw;
    final labelSize = fs * 0.88;
    final single = k == 1;
    for (var i = 0; i < n; i++) {
      for (var s = 0; s < k; s++) {
        final from = vPos(bases[s][i]);
        final to = vPos(tops[s][i]);
        if (_v(s, i) == 0 && !_stack) continue;
        // Series 0 sits first: left in a column cluster, bottom in a bar one.
        final g = _stack ? 0 : (_horizontal ? groups - 1 - s : s);
        Rect r;
        if (_horizontal) {
          // Category axis runs bottom to top; the cluster spans upwards.
          final yTop = cPos(i) - cluster / 2 + g * bw;
          r = Rect.fromLTRB(
              math.min(from, to), yTop, math.max(from, to), yTop + bw);
        } else {
          final x0 = (cPos(i) - cluster / 2) + g * bw;
          r = Rect.fromLTRB(
              x0, math.min(from, to), x0 + bw, math.max(from, to));
        }
        final color = single && s == 0
            ? chartSeriesColor(d, s, i)
            : chartSeriesColor(
                d, s, d.series[s].pointColors.length > i ? i : null);
        canvas.drawRect(r, Paint()..color = color);
        if (d.dataLabels) {
          final raw = _v(s, i);
          final text = formatChartNumber(raw);
          if (_stack) {
            final tp = _tp(text, labelSize, color: _contrast(color));
            final fits = _horizontal
                ? r.width > tp.width + 4 && r.height > tp.height * 0.8
                : r.height > tp.height * 1.1 && r.width > tp.width + 2;
            if (fits) {
              _text(
                  tp,
                  Offset(
                      r.center.dx - tp.width / 2, r.center.dy - tp.height / 2));
            }
          } else {
            final tp = _tp(text, labelSize, color: textColor);
            final negative = raw < 0;
            if (_horizontal) {
              final x = negative ? r.left - tp.width - 3 : r.right + 3;
              _text(tp, Offset(x, r.center.dy - tp.height / 2));
            } else {
              final y = negative ? r.bottom + 2 : r.top - tp.height - 1;
              _text(tp, Offset(r.center.dx - tp.width / 2, y));
            }
          }
        }
      }
    }
  }

  Path _linePath(List<Offset> pts, bool smooth) {
    final path = Path()..moveTo(pts.first.dx, pts.first.dy);
    if (!smooth || pts.length < 3) {
      for (var i = 1; i < pts.length; i++) {
        path.lineTo(pts[i].dx, pts[i].dy);
      }
      return path;
    }
    // Monotone cubic (Fritsch-Carlson): no overshoot between points.
    final n = pts.length;
    final dx = <double>[], m = <double>[];
    for (var i = 0; i < n - 1; i++) {
      dx.add(pts[i + 1].dx - pts[i].dx);
      m.add(dx[i] == 0 ? 0 : (pts[i + 1].dy - pts[i].dy) / dx[i]);
    }
    final tan = List<double>.filled(n, 0);
    tan[0] = m.first;
    tan[n - 1] = m.last;
    for (var i = 1; i < n - 1; i++) {
      tan[i] = m[i - 1] * m[i] <= 0 ? 0 : (m[i - 1] + m[i]) / 2;
    }
    for (var i = 0; i < n - 1; i++) {
      if (m[i] == 0) {
        tan[i] = 0;
        tan[i + 1] = 0;
        continue;
      }
      final a = tan[i] / m[i], b = tan[i + 1] / m[i];
      final h = math.sqrt(a * a + b * b);
      if (h > 3) {
        final t = 3 / h;
        tan[i] = t * a * m[i];
        tan[i + 1] = t * b * m[i];
      }
    }
    for (var i = 0; i < n - 1; i++) {
      final h = dx[i] / 3;
      path.cubicTo(pts[i].dx + h, pts[i].dy + tan[i] * h, pts[i + 1].dx - h,
          pts[i + 1].dy - tan[i + 1] * h, pts[i + 1].dx, pts[i + 1].dy);
    }
    return path;
  }

  void _paintLines(
      Rect plot,
      int n,
      List<List<double>> bases,
      List<List<double>> tops,
      double Function(int) cPos,
      double Function(double) vPos) {
    final lw = math.max(1.5, fs * 0.2);
    for (var s = 0; s < _seriesCount; s++) {
      final color = chartSeriesColor(d, s);
      final pts = <Offset>[];
      for (var i = 0; i < n; i++) {
        if (i >= d.series[s].values.length || !d.series[s].values[i].isFinite) {
          continue;
        }
        final y = vPos(_stack ? tops[s][i] : _v(s, i));
        pts.add(Offset(cPos(i), y));
      }
      if (pts.isEmpty) continue;
      final paint = Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = lw
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round;
      if (pts.length > 1) canvas.drawPath(_linePath(pts, d.smooth), paint);
      if (n <= 24) {
        final fill = Paint()..color = color;
        for (final p in pts) {
          canvas.drawCircle(p, lw * 1.35, fill);
        }
      }
      if (d.dataLabels) {
        var idx = 0;
        for (var i = 0; i < n; i++) {
          if (i >= d.series[s].values.length ||
              !d.series[s].values[i].isFinite) {
            continue;
          }
          final tp = _tp(formatChartNumber(_v(s, i)), fs * 0.88);
          final p = pts[idx++];
          _text(tp, Offset(p.dx - tp.width / 2, p.dy - tp.height - lw * 1.6));
        }
      }
    }
  }

  void _paintAreas(
      Rect plot,
      int n,
      List<List<double>> bases,
      List<List<double>> tops,
      double Function(int) cPos,
      double Function(double) vPos) {
    final lw = math.max(1.25, fs * 0.15);
    // Later series paint over earlier ones, as in PowerPoint's 2-D areas.
    for (var s = 0; s < _seriesCount; s++) {
      final color = chartSeriesColor(d, s);
      final top = <Offset>[], base = <Offset>[];
      for (var i = 0; i < n; i++) {
        top.add(Offset(cPos(i), vPos(_stack ? tops[s][i] : _v(s, i))));
        base.add(Offset(cPos(i), vPos(_stack ? bases[s][i] : 0)));
      }
      final path = Path()..moveTo(top.first.dx, top.first.dy);
      final topPath = _linePath(top, d.smooth);
      path
        ..addPath(topPath, Offset.zero)
        ..lineTo(base.last.dx, base.last.dy);
      for (var i = base.length - 1; i >= 0; i--) {
        path.lineTo(base[i].dx, base[i].dy);
      }
      path.close();
      canvas.drawPath(path, Paint()..color = color.withValues(alpha: 0.78));
      canvas.drawPath(
          topPath,
          Paint()
            ..color = color
            ..style = PaintingStyle.stroke
            ..strokeWidth = lw
            ..strokeJoin = StrokeJoin.round);
      if (d.dataLabels) {
        for (var i = 0; i < n; i++) {
          final tp = _tp(formatChartNumber(_v(s, i)), fs * 0.88);
          _text(
              tp, Offset(top[i].dx - tp.width / 2, top[i].dy - tp.height - 2));
        }
      }
    }
  }

  // -- scatter ---------------------------------------------------------------

  void _paintScatter(Rect area) {
    var xLo = double.infinity, xHi = -double.infinity;
    var yLo = double.infinity, yHi = -double.infinity;
    for (final s in d.series) {
      for (var i = 0; i < s.values.length; i++) {
        final x = (s.x != null && i < s.x!.length) ? s.x![i] : i + 1.0;
        final y = s.values[i];
        if (!x.isFinite || !y.isFinite) continue;
        xLo = math.min(xLo, x);
        xHi = math.max(xHi, x);
        yLo = math.min(yLo, y);
        yHi = math.max(yHi, y);
      }
    }
    if (!xLo.isFinite || !yLo.isFinite) return;
    if (yLo >= 0 && yLo < yHi * 5 / 6) yLo = 0;
    final ys = niceAxisScale(yLo, yHi,
        targetTicks: (area.height / (fs * 3.4)).floor().clamp(3, 8),
        fixedMin: d.axisMin,
        fixedMax: d.axisMax);
    final xs = niceAxisScale(xLo, xHi,
        targetTicks: (area.width / (fs * 6)).floor().clamp(3, 8));
    final col = textColor.withValues(alpha: 0.85);
    final yTps = [
      for (final t in ys.ticks)
        _tp(formatChartNumber(t, step: ys.step), fs, color: col)
    ];
    final xTps = [
      for (final t in xs.ticks)
        _tp(formatChartNumber(t, step: xs.step), fs, color: col)
    ];
    final tickW = yTps.map((t) => t.width).fold<double>(0, math.max);
    final tickH = xTps.map((t) => t.height).fold<double>(0, math.max);
    final plot = Rect.fromLTRB(
        area.left + tickW + fs * 0.7,
        area.top + tickH * 0.5,
        area.right - xTps.last.width * 0.5,
        area.bottom - tickH - fs * 0.6);
    if (plot.width < 8 || plot.height < 8) return;
    double px(double v) =>
        plot.left + (v - xs.min) / (xs.max - xs.min) * plot.width;
    double py(double v) =>
        plot.bottom - (v - ys.min) / (ys.max - ys.min) * plot.height;
    final grid = Paint()
      ..color = _grid
      ..strokeWidth = _hair;
    for (var i = 0; i < ys.ticks.length; i++) {
      final y = py(ys.ticks[i]);
      if (d.showGridlines) {
        canvas.drawLine(Offset(plot.left, y), Offset(plot.right, y), grid);
      }
      _text(yTps[i],
          Offset(plot.left - fs * 0.5 - yTps[i].width, y - yTps[i].height / 2));
    }
    for (var i = 0; i < xs.ticks.length; i++) {
      final x = px(xs.ticks[i]);
      if (d.showGridlines) {
        canvas.drawLine(Offset(x, plot.top), Offset(x, plot.bottom), grid);
      }
      _text(xTps[i], Offset(x - xTps[i].width / 2, plot.bottom + fs * 0.45));
    }
    final axis = Paint()
      ..color = _axis
      ..strokeWidth = _hair;
    canvas.drawLine(plot.bottomLeft, plot.bottomRight, axis);
    canvas.drawLine(plot.topLeft, plot.bottomLeft, axis);
    for (var s = 0; s < _seriesCount; s++) {
      final paint = Paint()
        ..color = chartSeriesColor(d, s).withValues(alpha: 0.88);
      final ser = d.series[s];
      for (var i = 0; i < ser.values.length; i++) {
        final x = (ser.x != null && i < ser.x!.length) ? ser.x![i] : i + 1.0;
        if (!x.isFinite || !ser.values[i].isFinite) continue;
        canvas.drawCircle(
            Offset(px(x), py(ser.values[i])), math.max(2.5, fs * 0.3), paint);
      }
    }
  }

  // -- pie / doughnut ----------------------------------------------------------

  void _paintPie(Rect area) {
    final s = d.series.first;
    final values = [for (final v in s.values) v.isFinite && v > 0 ? v : 0.0];
    final total = values.fold<double>(0, (a, b) => a + b);
    if (total <= 0) return;
    final radius = math.min(area.width, area.height) / 2 * 0.94;
    if (radius < 4) return;
    final c = area.center;
    final oval = Rect.fromCircle(center: c, radius: radius);
    final inner = d.kind == 'doughnut' ? radius * d.hole.clamp(0.1, 0.9) : 0.0;
    var angle = -math.pi / 2;
    final sep = Paint()
      ..color = const Color(0xFFFFFFFF).withValues(alpha: 0.9)
      ..style = PaintingStyle.stroke
      ..strokeWidth = math.max(1.0, fs * 0.1)
      ..strokeJoin = StrokeJoin.round;
    final slices = <(double, double, int)>[];
    for (var i = 0; i < values.length; i++) {
      if (values[i] <= 0) continue;
      final sweep = values[i] / total * 2 * math.pi;
      final path = Path();
      if (inner > 0) {
        final io = Rect.fromCircle(center: c, radius: inner);
        path
          ..arcTo(oval, angle, sweep, true)
          ..arcTo(io, angle + sweep, -sweep, false)
          ..close();
      } else {
        path
          ..moveTo(c.dx, c.dy)
          ..arcTo(oval, angle, sweep, false)
          ..close();
      }
      canvas.drawPath(path, Paint()..color = _sliceColor(i));
      if (values.length > 1) canvas.drawPath(path, sep);
      slices.add((angle, sweep, i));
      angle += sweep;
    }
    if (d.dataLabels) {
      for (final (a, sweep, i) in slices) {
        final frac = values[i] / total;
        if (frac < 0.04) continue;
        final mid = a + sweep / 2;
        final r = inner > 0 ? (inner + radius) / 2 : radius * 0.64;
        final text = '${(frac * 100).round()}%';
        final tp =
            _tp(text, fs * 0.92, color: _contrast(_sliceColor(i)), bold: true);
        _text(
            tp,
            Offset(c.dx + r * math.cos(mid) - tp.width / 2,
                c.dy + r * math.sin(mid) - tp.height / 2));
      }
    }
  }

  Color _contrast(Color bg) => bg.computeLuminance() > 0.55
      ? const Color(0xFF1F2937)
      : const Color(0xFFFFFFFF);
}
