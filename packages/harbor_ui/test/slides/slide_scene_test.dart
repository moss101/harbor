import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/slides.dart';

Map<String, dynamic> _fixture() =>
    jsonDecode(File('test/slides/fixtures/scene_full.json').readAsStringSync())
        as Map<String, dynamic>;

void main() {
  group('DeckScene.fromJson', () {
    test('parses the full-featured fixture', () {
      final deck = DeckScene.fromJson(_fixture());
      expect(deck.width, 12192000);
      expect(deck.height, 6858000);
      expect(deck.slides, hasLength(5));

      final s1 = deck.slides[0];
      expect(s1.index, 1);
      expect(s1.background.kind, PaintKind.linear);
      expect(s1.background.angle, 90);
      expect(s1.background.stops, hasLength(2));
      expect(s1.background.stops.last.color.toARGB32(), 0xFF1E3A8A);
      expect(s1.items, hasLength(4));
      expect(s1.notes, 'Welcome everyone.');
      expect(s1.unsupported, ['SmartArt (Content diagram)']);
      expect(s1.transition!.kind, 'fade');
      expect(s1.transition!.durMs, 700);
      expect(s1.transition!.onClick, isTrue);
      expect(s1.transition!.afterMs, isNull);
      expect(s1.animations, hasLength(2));
      expect(s1.animations[1].para, [0, 0]);
      expect(s1.animations[1].effect, 'fly');
      expect(s1.animations[1].trigger, 'click');
      expect(s1.animations[0].para, isNull);

      final title = s1.items[0] as SceneShape;
      expect(title.id, 1);
      expect(title.text!.anchor, 'ctr');
      expect(title.text!.paras.single.algn, 'ctr');
      expect(title.text!.paras.single.runs.single.b, isTrue);

      final pill = s1.items[2] as SceneShape;
      expect((pill.geom as PresetGeom).name, 'roundRect');
      expect((pill.geom as PresetGeom).adj, [50000]);
      expect(pill.line!.dash, 'dash');
      expect(pill.line!.cap, 'round');
      expect(pill.line!.tail, 'triangle');
      expect(pill.line!.head, isNull);
      expect(pill.shadow!.blur, 50800);
      expect(pill.shadow!.a, 0.4);

      final glow = s1.items[3] as SceneShape;
      expect(glow.fill.kind, PaintKind.radial);
      expect(glow.rot, 15);
      expect(glow.flipH, isTrue);
      expect(glow.flipV, isFalse);
    });

    test('text: rtl, bullets, numbering, soft breaks, autofit', () {
      final s2 = DeckScene.fromJson(_fixture()).slides[1];
      final body = (s2.items[1] as SceneShape).text!;
      expect(body.autofit, 'shrink');
      expect(body.fontScale, 0.925);
      expect(body.lineReduction, 0.1);
      expect(body.paras[0].bullet!.kind, 'char');
      expect(body.paras[0].bullet!.c, '#F59E0B');
      expect(body.paras[0].lnPct, 1.1);
      expect(body.paras[0].indent, -342900);
      expect(body.paras[1].bullet!.isNumbered, isTrue);
      expect(body.paras[1].bullet!.numType, 'arabicPeriod');
      expect(body.paras[1].runs[1].isBreak, isTrue);
      expect(body.paras[1].runs[2].baseline, 30);

      final ar = (s2.items[2] as SceneShape).text!;
      expect(ar.paras[0].rtl, isTrue);
      expect(ar.paras[0].algn, 'r');
      expect(ar.paras[0].runs.single.cs, 'Arial');
      expect(s2.plainText, contains('Key points'));
    });

    test('chart, picture, table, custom path', () {
      final deck = DeckScene.fromJson(_fixture());
      final col = deck.slides[2].items[0] as SceneChart;
      expect(col.chart.kind, 'column');
      expect(col.chart.title, 'Revenue by quarter');
      expect(col.chart.series, hasLength(2));
      expect(col.chart.series[1].values, [15, 25, 35, 45]);
      expect(col.chart.legend, 'b');
      expect(col.chart.dataLabels, isTrue);
      final dn = deck.slides[2].items[1] as SceneChart;
      expect(dn.chart.series.single.pointColors, hasLength(3));
      expect(dn.chart.hole, 0.6);
      expect(deck.slides[2].transition!.afterMs, 3000);
      expect(deck.slides[2].transition!.dir, 'l');

      final pic = deck.slides[3].items[0] as ScenePicture;
      expect(pic.crop, [0.5, 0, 0, 0]);
      expect(pic.dataB64, isNotEmpty);
      expect(pic.alt, 'A 2x2 test image');
      expect((deck.slides[3].items[1] as ScenePicture).dataB64, isEmpty);
      final path = (deck.slides[3].items[2] as SceneShape).geom as PathGeom;
      expect(path.cmds.map((c) => c.op), ['M', 'L', 'C', 'Q', 'Z']);
      expect(path.cmds[2].x2, 50);

      final tbl = deck.slides[4].items[0] as SceneTable;
      expect(tbl.cols, [3000000, 3000000, 3000000]);
      expect(tbl.rows, hasLength(2));
      expect(tbl.rows[0].cells[0].gridSpan, 2);
      expect(tbl.rows[0].cells[1].mergedAway, isTrue);
      expect(tbl.rows[0].cells[0].borders[3]!.w, 12700);
      expect(tbl.rows[0].cells[0].borders[0], isNull);
    });
  });

  group('tolerance', () {
    test('defaults for missing optional fields', () {
      final deck = DeckScene.fromJson({
        'slides': [
          {
            'items': [
              {'t': 'shape', 'id': 7, 'x': 1, 'y': 2, 'w': 3, 'h': 4},
              {'t': 'picture', 'id': 8, 'w': 10, 'h': 10},
              {'t': 'chart', 'id': 9},
              {'t': 'table', 'id': 10},
            ],
          },
        ],
      });
      expect(deck.width, 12192000);
      expect(deck.height, 6858000);
      final s = deck.slides.single;
      expect(s.index, 1);
      expect(s.background.kind, PaintKind.none);
      expect(s.transition, isNull);
      expect(s.animations, isEmpty);
      expect(s.notes, isNull);
      expect(s.unsupported, isEmpty);
      final sh = s.items[0] as SceneShape;
      expect(sh.id, 7);
      expect(sh.rot, 0);
      expect(sh.fill.isNone, isTrue);
      expect(sh.text, isNull);
      expect((sh.geom as PresetGeom).name, 'rect');
      final pic = s.items[1] as ScenePicture;
      expect(pic.crop, [0, 0, 0, 0]);
      expect(pic.dataB64, '');
      expect((s.items[2] as SceneChart).chart.series, isEmpty);
      expect((s.items[3] as SceneTable).rows, isEmpty);
    });

    test('empty text body gets Office default insets and wrap', () {
      final tb = SceneTextBody.fromJson(const {});
      expect(tb.ins, [91440, 45720, 91440, 45720]);
      expect(tb.wrap, isTrue);
      expect(tb.anchor, 't');
      expect(tb.fontScale, 1);
    });

    test('unknown enum strings and unknown item kinds do not throw', () {
      final deck = DeckScene.fromJson({
        'width': 100,
        'height': 50,
        'slides': [
          {
            'index': 3,
            'background': {'k': 'hologram'},
            'items': [
              {'t': 'sparkle', 'id': 1},
              {
                't': 'shape',
                'id': 2,
                'x': 0,
                'y': 0,
                'w': 1,
                'h': 1,
                'geom': {
                  'k': 'preset',
                  'name': 'nonsense',
                  'adj': [1, 'x']
                },
                'fill': {'k': 'plaid'},
                'line': {
                  'w': 1,
                  'paint': {'k': 'solid', 'c': 'zzz', 'a': 2}
                },
                'text': {
                  'anchor': 'weird',
                  'autofit': 'whatever',
                  'paras': [
                    {
                      'algn': 'diagonal',
                      'bullet': {'kind': 'emoji'},
                      'runs': [
                        {'t': 'x', 'baseline': 'oops'}
                      ]
                    }
                  ],
                },
              },
            ],
            'transition': {'kind': 'teleport', 'dir': 'sideways'},
            'animations': [
              {'shape_id': 2, 'effect': 'explode', 'phase': 'bizarre'}
            ],
          },
        ],
      });
      final s = deck.slides.single;
      expect(s.index, 3);
      expect(s.background.kind, PaintKind.none);
      expect(s.items, hasLength(1)); // unknown item kind skipped
      final sh = s.items.single as SceneShape;
      expect((sh.geom as PresetGeom).name, 'nonsense');
      expect(sh.fill.isNone, isTrue);
      expect(sh.line!.paint.kind, PaintKind.solid);
      expect(sh.text!.paras.single.runs.single.baseline, 0);
      expect(s.transition!.kind, 'teleport');
      expect(s.animations.single.effect, 'explode');
    });

    test('parseSceneColor handles garbage and alpha', () {
      expect(parseSceneColor('#FF0000', 1).toARGB32(), 0xFFFF0000);
      expect(parseSceneColor('00FF00', 1).toARGB32(), 0xFF00FF00);
      expect(parseSceneColor('nope', 1).toARGB32(), 0xFF000000);
      expect(parseSceneColor('#0000FF', 0.5).a, closeTo(0.5, 0.01));
      expect(parseSceneColor('#0000FF', double.nan).a, 1);
    });
  });

  test('loads Rust-produced scene_*.json fixtures when present', () {
    final dir = Directory('../../core/harbor_render/tests/fixtures');
    if (!dir.existsSync()) {
      markTestSkipped('no Rust fixtures yet');
      return;
    }
    final files = dir.listSync().whereType<File>().where((f) =>
        f.uri.pathSegments.last.startsWith('scene_') &&
        f.path.endsWith('.json'));
    for (final f in files) {
      final json = jsonDecode(f.readAsStringSync());
      if (json is! Map<String, dynamic>) continue;
      final map = json['scene'] is Map<String, dynamic>
          ? json['scene'] as Map<String, dynamic>
          : json;
      expect(() => DeckScene.fromJson(map), returnsNormally, reason: f.path);
    }
  });
}
