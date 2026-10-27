import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';
import 'package:harbor_ui/slides.dart';

/// Deck used by the painter tests: 1 px == 12700 EMU == 1 pt at the
/// default 960 x 540 render, so geometry and font sizes can be written in
/// pixels.
const double kDeckW = 12192000;
const double kDeckH = 6858000;
const double kPx = 12700;
const int kViewW = 960;
const int kViewH = 540;

double e(num px) => px * kPx;

class Pixels {
  Pixels(this.width, this.height, this.data);
  final int width, height;
  final ByteData data;

  /// (r, g, b, a) 0..255 at (x, y).
  List<int> at(int x, int y) {
    final o = (y * width + x) * 4;
    return [
      data.getUint8(o),
      data.getUint8(o + 1),
      data.getUint8(o + 2),
      data.getUint8(o + 3),
    ];
  }

  bool near(int x, int y, int r, int g, int b, {int tol = 6}) {
    final p = at(x, y);
    return (p[0] - r).abs() <= tol &&
        (p[1] - g).abs() <= tol &&
        (p[2] - b).abs() <= tol;
  }

  /// All pixels in [rect] satisfying [test].
  List<(int, int)> where(bool Function(List<int> p) test, {Rect? within}) {
    final out = <(int, int)>[];
    final r =
        within ?? Rect.fromLTWH(0, 0, width.toDouble(), height.toDouble());
    for (var y = r.top.floor().clamp(0, height - 1);
        y < r.bottom.ceil().clamp(0, height);
        y++) {
      for (var x = r.left.floor().clamp(0, width - 1);
          x < r.right.ceil().clamp(0, width);
          x++) {
        if (test(at(x, y))) out.add((x, y));
      }
    }
    return out;
  }
}

Future<Pixels> renderSlide(
  SlideScene scene, {
  int width = kViewW,
  int height = kViewH,
  Map<int, ShapeEffect> effects = const {},
  Set<int> hidden = const {},
  Map<int, Set<int>> hiddenParas = const {},
}) async {
  // Pictures decode asynchronously: have them ready before the first paint.
  for (final it in scene.items) {
    if (it is ScenePicture) await SlideImageCache.instance.load(it.dataB64);
  }
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder);
  SlidePainter(
    scene: scene,
    deckWidth: kDeckW,
    effects: effects,
    hidden: hidden,
    hiddenParas: hiddenParas,
  ).paint(canvas, Size(width.toDouble(), height.toDouble()));
  final image = await recorder.endRecording().toImage(width, height);
  final data = (await image.toByteData(format: ui.ImageByteFormat.rawRgba))!;
  return Pixels(width, height, data);
}

SlideScene slideOf(List<SceneItem> items,
        {ScenePaint background =
            const ScenePaint(kind: PaintKind.solid, c: '#FFFFFF')}) =>
    SlideScene(index: 1, background: background, items: items);

/// A rectangle-ish shape positioned in px.
SceneShape box(
  int id,
  double x,
  double y,
  double w,
  double h, {
  SceneGeom geom = const PresetGeom('rect'),
  ScenePaint fill = ScenePaint.none,
  SceneLine? line,
  SceneShadow? shadow,
  SceneTextBody? text,
  double rot = 0,
  bool flipH = false,
  bool flipV = false,
}) =>
    SceneShape(
      id: id,
      x: e(x),
      y: e(y),
      w: e(w),
      h: e(h),
      rot: rot,
      flipH: flipH,
      flipV: flipV,
      geom: geom,
      fill: fill,
      line: line,
      shadow: shadow,
      text: text,
    );

ScenePaint solid(String hex, [double a = 1]) =>
    ScenePaint(kind: PaintKind.solid, c: hex, a: a);

SceneTextBody textBody(List<SceneParagraph> paras,
        {String anchor = 't',
        bool wrap = true,
        String autofit = 'none',
        List<double> ins = const [0, 0, 0, 0]}) =>
    SceneTextBody(
        ins: [for (final v in ins) e(v)],
        anchor: anchor,
        wrap: wrap,
        autofit: autofit,
        paras: paras);

SceneRun run(String t,
        {double sz = 20,
        String c = '#000000',
        bool b = false,
        double baseline = 0}) =>
    SceneRun(t: t, sz: sz, c: c, b: b, baseline: baseline);
