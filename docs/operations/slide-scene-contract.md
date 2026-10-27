# Slide scene contract

Real slide previews: the Rust core reads a PPTX package and resolves every
slide into a **scene**; Flutter paints scenes. No OOXML knowledge lives in
the app. The scene is read from the *file*, so a Harbor-made deck, a deck
from PowerPoint and a deck from anywhere else preview through the same path
and what you see is what the file contains.

- Rust types (the JSON shape is normative): `core/harbor_render/src/scene.rs`
- Producer: `harbor_render::slides` (core), surfaced as
  `artifact.preview` → `{ "kind": "deck", "preview": { "kind", "title", "slides": [...text...],
  "scene": DeckScene | null, "scene_error": String | null }, "compatibility": ... }`.
  The scene rides INSIDE `preview` (the map the Work surface already hands to
  `DeckView`). `scene` is `null` (with `scene_error`) when the package could
  not be resolved; the text-only `slides` list always remains.
- Consumers: `packages/harbor_ui/lib/slides.dart` (painter, player,
  presenter), then `apps/harbor_office` and `apps/harbor_app` deck views.

## Units and conventions

| thing | unit |
|---|---|
| geometry (`x y w h`, insets, line width, shadow offsets, indents) | EMU, `f64`. 12700 EMU = 1 pt; 914400 = 1 in. A 16:9 deck is 12192000 × 6858000 |
| font size, paragraph spacing, character spacing | points |
| time | milliseconds |
| angle | degrees, clockwise (`rot`; gradient `angle` 0 = left→right, 90 = top→bottom) |
| colour | `"#RRGGBB"` plus a separate alpha `0.0..=1.0` |

A painter maps the slide to a view of width `W` px with
`scale = W / deck.width`. A font of `sz` points paints at
`sz * 12700 * scale` logical pixels.

Items are in **paint order** (first = back-most). Groups are flattened by
the producer; every item arrives with absolute slide coordinates. Rotation is
about the centre of the unrotated box; flips apply before rotation.

## JSON shape (abridged — `scene.rs` is complete)

```jsonc
{ "width": 12192000, "height": 6858000,
  "slides": [ {
    "index": 1,
    "background": { "k": "solid", "c": "#0B1220", "a": 1 },      // Paint
    "items": [ ...Item ],
    "transition": { "kind": "fade", "dir": null, "dur_ms": 700, "on_click": true, "after_ms": null },
    "animations": [ { "shape_id": 4, "para": null, "effect": "fade", "phase": "entrance",
                      "dir": null, "trigger": "after_prev", "delay_ms": 0, "dur_ms": 500 } ],
    "notes": "speaker notes or null",
    "unsupported": [ "SmartArt (Content diagram)" ]
  } ] }
```

`Paint`: `{k:"none"}` | `{k:"solid",c,a}` | `{k:"linear",angle,stops:[{p,c,a}]}`
| `{k:"radial",cx,cy,stops}`.

`Item` is tagged by `t`: `shape` | `picture` | `chart` | `table`. All carry the
flattened `Frame` fields `id name alt x y w h rot flip_h flip_v`.

- `shape`: `geom` (`{k:"preset",name,adj:[]}` or `{k:"path",w,h,cmds:[{c:"M",x,y}|{c:"L"}|{c:"C"}|{c:"Q"}|{c:"Z"}]}`),
  `fill`, `line?`, `shadow?`, `text?`.
- `picture`: `mime`, `data_b64`, `crop:[l,t,r,b]` (fractions), `geom` (clip), `line?`, `shadow?`.
- `chart`: `chart` = `ChartData` (see below).
- `table`: `cols:[emu]`, `rows:[{h, cells:[{text, fill, borders:[l,t,r,b], grid_span, row_span, merged_away}]}]`.

`TextBody`: `ins:[l,t,r,b]`, `anchor: t|ctr|b`, `wrap`, `autofit: none|shrink|resize`,
`font_scale`, `line_reduction`, `paras:[Para]`. `Para`: `algn l|ctr|r|just`
(absolute, never `start`), `rtl`, `mar_l`, `indent`, `bullet?`, `spc_bef`,
`spc_aft`, `ln_pct?`/`ln_pts?`, `runs:[Run]`, `end_sz`. `Run`: `t sz b i u strike c a
font cs baseline caps spc`. A run whose text is `"\n"` is a soft line break.

**Fonts.** `font` / `cs` are family names as authored (`Calibri`, `Georgia`,
`Arial`, ...). The device may not have them: the painter keeps a fallback
chain per family (sans / serif / mono / Arabic) and must still wrap inside the
frame. Previews approximate glyph widths; the contract is "never overflow the
frame silently" — apply `font_scale`, then clip to the frame.

**Right-to-left.** `Para.rtl` paragraphs lay out right-to-left with `algn`
already absolute. Runs with `cs` use that family. Bullets and numbering sit on
the right.

### `ChartData`

`kind` (`bar` horizontal | `column` | `line` | `area` | `pie` | `doughnut` |
`scatter`), `title?`, `categories[]`, `series[{name, values[], x?[], color,
point_colors[]}]`, `stacked`, `percent_stacked`, `legend` (`none|t|b|l|r`),
`data_labels`, `axis_min?`, `axis_max?`, `show_gridlines`, `text_color`,
`gap_width`, `smooth`, `hole`.

### Motion

`Transition.kind` normalised names: `fade fade_through_black push wipe split
cover uncover pull zoom morph ripple reveal flip vortex prism glitter honeycomb
doors window ferris gallery conveyor pan warp flythrough flash shred random
other`. Anything a player does not implement falls back to `fade` (and never
fails). `dir`: `l r u d lu ru ld rd in out horz vert`.

`Animation` list is in **play order**. `trigger`:
`click` (wait for the presenter) | `with_prev` (start with the previous step) |
`after_prev` (start when the previous step ends); `delay_ms` is extra. Steps
that target the same `shape_id` + `para` range are sequential builds. `phase`
`entrance` means the target is **hidden until its step starts**; `exit` means
visible until it plays; `emphasis` never changes visibility. Effects:
`appear fade fly float wipe zoom split wheel grow spin pulse other` (unknown →
`fade`). Total auto-played motion must respect the platform "reduce motion"
setting: with it on, the player skips animations and transitions entirely.

## Dart API (names are fixed — three agents code against them)

`packages/harbor_ui/lib/src/slides/` (exported by `package:harbor_ui/slides.dart`):

```dart
// slide_scene.dart  — pure data, no Flutter imports beyond dart:ui types
class DeckScene { final double width, height; final List<SlideScene> slides;
  factory DeckScene.fromJson(Map<String, dynamic> json); }
class SlideScene { final int index; final ScenePaint background; final List<SceneItem> items;
  final SceneTransition? transition; final List<SceneAnimation> animations;
  final String? notes; final List<String> unsupported; }
// SceneItem is a sealed class: SceneShape, ScenePicture, SceneChart, SceneTable.
// Every SceneItem has `int id` and the Frame fields.
class SceneTransition { final String kind; final String? dir; final int durMs; final bool onClick; final int? afterMs; }
class SceneAnimation  { final int shapeId; final List<int>? para; final String effect, phase, trigger; final String? dir; final int delayMs, durMs; }

// slide_view.dart
/// Per-shape runtime effect applied on top of the static scene (the
/// presenter animates these; static previews pass none).
class ShapeEffect { final double opacity; final double dx, dy;   // in EMU
  final double scale; final double? wipe;  // 0..1 reveal fraction along [wipeDir]
  final String? wipeDir;
  const ShapeEffect({this.opacity = 1, this.dx = 0, this.dy = 0, this.scale = 1, this.wipe, this.wipeDir}); }

class SlideView extends StatelessWidget {
  const SlideView({super.key, required this.scene, required this.deckWidth, required this.deckHeight,
    this.effects = const {},          // Map<int shapeId, ShapeEffect>
    this.hidden = const {},           // Set<int> shapeIds not painted at all
    this.hiddenParas = const {},      // Map<int shapeId, Set<int paraIndex>> paragraphs not painted
    this.showUnsupportedBadge = false, this.semanticsLabel});
  // Sizes itself to the incoming width, AspectRatio(deckWidth/deckHeight).
}
class SlideThumbnail extends StatelessWidget { /* SlideView in a RepaintBoundary, fixed small size, no semantics noise */ }
```

Presenter (`slide_player.dart`, `slide_transitions.dart`, `present_view.dart`):

```dart
class SlidePlayer { // pure Dart timeline engine, unit-testable without a widget tree
  SlidePlayer(SlideScene scene);
  int get steps;                       // number of click steps (0 = nothing to click)
  /// State at [t] ms into step [step]; step -1 = before anything (entrance targets hidden).
  PlayState stateAt(int step, int tMs);
}
class PlayState { final Map<int, ShapeEffect> effects; final Set<int> hidden;
  final Map<int, Set<int>> hiddenParas; }
class TransitionView extends StatelessWidget {  // paints [from] -> [to] at progress 0..1
  const TransitionView({super.key, required this.from, required this.to, required this.deckWidth,
    required this.deckHeight, required this.transition, required this.progress}); }
class PresentView extends StatefulWidget {      // full-screen presenter
  const PresentView({super.key, required this.deck, this.startIndex = 0, this.reduceMotion}); }
```

## Core integration

`artifact.preview` for a deck adds `preview.scene` (see top). Both apps already
call it (`harbor_service.dart`); the Work surface hands `result['preview']` to
`DeckView(preview: ...)`, so the scene is `preview['scene']`. Until the producer lands, tests use the fixtures
in `core/harbor_render/tests/fixtures/` (hand-written first, then produced by
Rust).
