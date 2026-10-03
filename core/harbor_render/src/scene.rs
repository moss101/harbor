//! The slide scene: a resolved, renderer-ready description of a slide.
//!
//! `harbor_render::slides` reads a PPTX package (Harbor-made or not) and
//! resolves everything a painter needs — placeholder inheritance, theme
//! colours and colour transforms, fonts, geometry — into these types. The
//! Flutter `SlideView` paints them; nothing in the app re-implements
//! OOXML. Contract: `docs/operations/slide-scene-contract.md` (the JSON
//! shape of these types IS the contract; keep the two in step).
//!
//! Units: geometry in EMU (`f64`), font sizes and paragraph spacing in
//! points, times in milliseconds, angles in degrees (clockwise), colours
//! as `"#RRGGBB"` with a separate alpha in `0.0..=1.0`.

use serde::{Deserialize, Serialize};

/// A whole deck: one slide size, every slide resolved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeckScene {
    pub width: f64,
    pub height: f64,
    pub slides: Vec<SlideScene>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SlideScene {
    /// 1-based position in presentation order.
    pub index: usize,
    pub background: Paint,
    /// Paint order: first is back-most.
    pub items: Vec<Item>,
    pub transition: Option<Transition>,
    /// Play order (see [`Animation`]).
    pub animations: Vec<Animation>,
    pub notes: Option<String>,
    /// Features present on the slide that this scene could not draw,
    /// stated rather than silently dropped.
    pub unsupported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Item {
    Shape(ShapeItem),
    Picture(PictureItem),
    Chart(ChartItem),
    Table(TableItem),
}

/// Placement shared by every item. `(x, y)` is the top-left of the
/// unrotated box; `rot` rotates about its centre; flips apply before the
/// rotation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// The shape id on the slide (`p:cNvPr/@id`); animations target it.
    pub id: u32,
    pub name: String,
    /// Alt text (`descr`), if any.
    pub alt: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShapeItem {
    #[serde(flatten)]
    pub frame: Frame,
    pub geom: Geom,
    pub fill: Paint,
    pub line: Option<LineStyle>,
    pub shadow: Option<Shadow>,
    pub text: Option<TextBody>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PictureItem {
    #[serde(flatten)]
    pub frame: Frame,
    /// `image/png`, `image/jpeg`, `image/gif`, `image/svg+xml`, ...
    pub mime: String,
    /// Standard base64 of the encoded image. Empty when the image was too
    /// large to carry (`unsupported` on the slide says so).
    pub data_b64: String,
    /// Source-rectangle crop as fractions `[left, top, right, bottom]`.
    pub crop: [f64; 4],
    /// Clip geometry (a rounded picture is a `roundRect` preset).
    pub geom: Geom,
    pub line: Option<LineStyle>,
    pub shadow: Option<Shadow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartItem {
    #[serde(flatten)]
    pub frame: Frame,
    pub chart: ChartData,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableItem {
    #[serde(flatten)]
    pub frame: Frame,
    /// Column widths in EMU.
    pub cols: Vec<f64>,
    pub rows: Vec<TableRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableRow {
    /// Row height in EMU.
    pub h: f64,
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableCell {
    pub text: TextBody,
    pub fill: Paint,
    /// Borders as `[left, top, right, bottom]`.
    pub borders: [Option<LineStyle>; 4],
    pub grid_span: u32,
    pub row_span: u32,
    /// Covered by a neighbouring merged cell (draw nothing).
    pub merged_away: bool,
}

// ---------------------------------------------------------------------------
// Geometry and paint

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Geom {
    /// A preset shape by its DrawingML name (`rect`, `roundRect`,
    /// `ellipse`, `line`, `triangle`, `rtTriangle`, `diamond`, `chevron`,
    /// `homePlate`, `parallelogram`, `trapezoid`, `hexagon`, `pentagon`,
    /// `rightArrow`, `leftArrow`, `upArrow`, `downArrow`, `plus`, `star5`,
    /// `arc`, `pie`, `blockArc`, `donut`, `flowChartProcess`, ...). `adj`
    /// holds the adjust values in OOXML units (`roundRect`: `[16667]`
    /// means 16.667% of the short side). A painter that does not know a
    /// name draws the frame as a rectangle and the slide lists it under
    /// `unsupported`.
    Preset { name: String, adj: Vec<f64> },
    /// A custom path in its own `w`×`h` coordinate space, scaled to the
    /// frame.
    Path {
        w: f64,
        h: f64,
        cmds: Vec<PathCmd>,
    },
}

impl Default for Geom {
    fn default() -> Self {
        Geom::Preset {
            name: "rect".into(),
            adj: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "c")]
pub enum PathCmd {
    M { x: f64, y: f64 },
    L { x: f64, y: f64 },
    /// Cubic Bézier.
    C {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    /// Quadratic Bézier.
    Q { x1: f64, y1: f64, x: f64, y: f64 },
    Z,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Paint {
    #[default]
    None,
    Solid {
        c: String,
        a: f64,
    },
    /// `angle` is the direction of the gradient line, degrees clockwise
    /// from the +x axis (0 = left→right, 90 = top→bottom).
    Linear {
        angle: f64,
        stops: Vec<Stop>,
    },
    /// Centre `(cx, cy)` as fractions of the frame.
    Radial {
        cx: f64,
        cy: f64,
        stops: Vec<Stop>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    /// Position `0.0..=1.0`.
    pub p: f64,
    pub c: String,
    pub a: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineStyle {
    /// Width in EMU.
    pub w: f64,
    pub paint: Paint,
    /// `solid`, `dash`, `dot`, `dashDot`, `lgDash`, `sysDash`, `sysDot`.
    pub dash: String,
    /// `flat`, `round`, `square`.
    pub cap: String,
    /// Arrowhead names (`triangle`, `arrow`, `stealth`, `oval`,
    /// `diamond`) at the start / end of the line.
    pub head: Option<String>,
    pub tail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub dx: f64,
    pub dy: f64,
    pub blur: f64,
    pub c: String,
    pub a: f64,
}

// ---------------------------------------------------------------------------
// Text

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextBody {
    /// Insets `[left, top, right, bottom]` in EMU.
    pub ins: [f64; 4],
    /// `t`, `ctr`, `b`.
    pub anchor: String,
    pub wrap: bool,
    /// `none`, `shrink` (normAutofit), `resize` (spAutoFit).
    pub autofit: String,
    /// `normAutofit/@fontScale` as a fraction (1.0 = unscaled).
    pub font_scale: f64,
    /// `normAutofit/@lnSpcReduction` as a fraction (0.1 = 10% tighter).
    pub line_reduction: f64,
    pub paras: Vec<Para>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Para {
    /// `l`, `ctr`, `r`, `just`. Absolute: a right-to-left paragraph that
    /// is "start aligned" already arrives as `r`.
    pub algn: String,
    pub rtl: bool,
    /// Left margin and first-line indent in EMU (indent is negative for a
    /// hanging bullet). For `rtl` paragraphs they apply from the right.
    pub mar_l: f64,
    pub indent: f64,
    pub bullet: Option<Bullet>,
    /// Space before / after in points.
    pub spc_bef: f64,
    pub spc_aft: f64,
    /// Line spacing as a multiple of the font's line height (1.0 =
    /// single), or exact points when `ln_pts` is set.
    pub ln_pct: Option<f64>,
    pub ln_pts: Option<f64>,
    pub runs: Vec<Run>,
    /// Size of an empty paragraph, in points.
    pub end_sz: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bullet {
    /// `char` or `num`.
    pub kind: String,
    /// The bullet character (`char`).
    pub ch: String,
    /// Numbering scheme (`num`): `arabicPeriod`, `arabicParenR`,
    /// `romanUcPeriod`, `alphaLcPeriod`, ...
    pub num_type: String,
    pub start: u32,
    /// Bullet colour; `None` follows the text.
    pub c: Option<String>,
    /// Size relative to the text (1.0 = same).
    pub sz_pct: f64,
    pub font: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Text. `"\n"` is a soft line break. Fields are resolved (slide
    /// number → its digits).
    pub t: String,
    /// Size in points.
    pub sz: f64,
    pub b: bool,
    pub i: bool,
    pub u: bool,
    pub strike: bool,
    pub c: String,
    pub a: f64,
    /// Latin font family; `cs` is the complex-script (Arabic) family and
    /// applies to right-to-left runs.
    pub font: String,
    pub cs: Option<String>,
    /// Superscript/subscript offset in percent (`30` / `-25`).
    pub baseline: f64,
    pub caps: bool,
    /// Extra character spacing in points.
    pub spc: f64,
}

// ---------------------------------------------------------------------------
// Charts (basic series; the scope of matrix rows 14/19)

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartData {
    /// `bar` (horizontal), `column`, `line`, `area`, `pie`, `doughnut`,
    /// `scatter`.
    pub kind: String,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<ChartSeries>,
    pub stacked: bool,
    pub percent_stacked: bool,
    /// `none`, `t`, `b`, `l`, `r`.
    pub legend: String,
    pub data_labels: bool,
    pub axis_min: Option<f64>,
    pub axis_max: Option<f64>,
    pub show_gridlines: bool,
    /// Text colour for axes, labels and legend.
    pub text_color: String,
    /// Gap between category clusters as a fraction of the bar width.
    pub gap_width: f64,
    pub smooth: bool,
    /// Pie/doughnut hole size as a fraction of the radius.
    pub hole: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
    /// X values (scatter only).
    pub x: Option<Vec<f64>>,
    pub color: String,
    /// Per-point colours (pie slices, vary-colours bars); empty = `color`.
    pub point_colors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Motion

/// A slide transition as authored (`p:transition`, including the `p14`,
/// `p15` and `p159` extension forms). `kind` is a normalised name:
/// `fade`, `fade_through_black`, `push`, `wipe`, `split`, `cover`,
/// `uncover`, `pull`, `zoom`, `morph`, `ripple`, `reveal`, `flip`,
/// `vortex`, `prism`, `glitter`, `honeycomb`, `doors`, `window`, `ferris`,
/// `gallery`, `conveyor`, `pan`, `warp`, `flythrough`, `flash`, `shred`,
/// `random`, or `other` (present but not one Harbor knows — painters fall
/// back to `fade`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: String,
    /// Direction: `l`, `r`, `u`, `d`, `lu`, `ru`, `ld`, `rd`, `in`, `out`,
    /// `horz`, `vert`.
    pub dir: Option<String>,
    pub dur_ms: u32,
    pub on_click: bool,
    /// Auto-advance after this many ms.
    pub after_ms: Option<u32>,
}

/// One authored animation step, in play order. `trigger` follows
/// PowerPoint: `click` waits for the presenter, `with_prev` starts
/// together with the previous step, `after_prev` starts when the
/// previous step has finished. `delay_ms` is added on top.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Animation {
    pub shape_id: u32,
    /// Paragraph range `[first, last]` (0-based, inclusive) when the
    /// animation builds text by paragraph.
    pub para: Option<[u32; 2]>,
    /// `appear`, `fade`, `fly`, `float`, `wipe`, `zoom`, `split`,
    /// `wheel`, `grow`, `spin`, `pulse`, `other`.
    pub effect: String,
    /// `entrance`, `exit`, `emphasis`.
    pub phase: String,
    pub dir: Option<String>,
    pub trigger: String,
    pub delay_ms: u32,
    pub dur_ms: u32,
}
