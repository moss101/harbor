//! Two-sided comparisons in three compositions.
//!
//! * **Panels**: two boxed panels, each with its own heading treatment
//!   (inset chip, accent tick, pill or outlined sticker chip, chosen by the
//!   theme's card chrome and header) and a VS badge on the seam.
//! * **Rows**: an open table. Side headings sit over coloured rules, the
//!   points run in two columns divided by hairlines, aligned row by row when
//!   both sides have as many points.
//! * **Fields**: two full-height colour fields, text on colour: the primary
//!   accent against a deep tone, ink chosen for contrast.

#![allow(unused_imports)]
use super::kit::*;
use crate::deck::grid::*;
use crate::deck::look::*;
use crate::deck::model::{split_stat, Block, DesignedSlide, LayoutKind};
use crate::deck::motion::Role;
use crate::deck::parts::LayoutPart;
use crate::deck::theme::{contrast, ThemeSpec};
use crate::deck::xml::*;
use crate::pptx::{ChartSpec, SlideImage};

pub(crate) fn comparison_titles(title: &str) -> (String, String) {
    let lower = title.to_lowercase();
    for sep in [" vs. ", " vs ", " versus ", " or "] {
        if let Some(i) = lower.find(sep) {
            let l = title[..i].trim().to_string();
            let r = title[i + sep.len()..].trim().trim_end_matches('?').to_string();
            if !l.is_empty() && !r.is_empty() && l.chars().count() <= 24 && r.chars().count() <= 24 {
                return (l, r);
            }
        }
    }
    ("Option A".into(), "Option B".into())
}

/// A solid fill that carries text at `min` contrast: the theme colour `base`
/// (darkened from `start_mod` percent luminance when it cannot carry the
/// text) and the ink (`lt1` or `dk1`) to set on it. Only theme references,
/// so every palette works; the result is verified with the deck's own
/// colour resolution.
pub(crate) fn inked_fill(t: &ThemeSpec, base: Clr, start_mod: u32, min: f64) -> (Clr, Clr) {
    let (lt, dk) = (t.palette.lt1, t.palette.dk1);
    let mut best = (base, Clr::lt1());
    for k in [100u32, 86, 72, 58, 46, 36, 28] {
        let m = start_mod * k / 100;
        let c = if m >= 100 {
            base
        } else {
            Clr {
                lum_mod: Some(m),
                lum_off: None,
                ..base
            }
        };
        let (hex, _) = t.resolve(&c);
        let (cl, cd) = (contrast(&hex, lt), contrast(&hex, dk));
        let (ink, got) = if cl >= cd { (Clr::lt1(), cl) } else { (Clr::dk1(), cd) };
        best = (c, ink);
        if got >= min {
            return best;
        }
    }
    best
}

/// The area the flow layouts build in: the frame's body, lifted clear of the
/// ornament a centred header hangs above it.
pub(crate) fn body_area(cx: &Ctx) -> R {
    let b = cx.frame.body;
    if cx.t.look.header == Header::Centered {
        R::new(b.x, b.y + 150_000, b.w, b.h - 150_000)
    } else {
        b
    }
}

/// On decks with a drafting grid behind the page text must sit on an opaque
/// sheet, never over the grid lines.
pub(crate) fn sheet(t: &ThemeSpec, name: &str, rect: R) -> Option<Shape> {
    if t.deco != Deco::Grid {
        return None;
    }
    let mut s = Shape::deco(name, radius(t), rect, Fill::Solid(Clr::bg1()));
    s.line = Some(Line::new(Clr::tx1().alpha(30), 1.0));
    Some(s)
}

struct Side {
    head: String,
    points: Vec<String>,
}

fn sides_of(s: &DesignedSlide) -> Vec<Side> {
    let mut v: Vec<Side> = if s.blocks.len() >= 2 {
        s.blocks
            .iter()
            .take(2)
            .map(|x| Side {
                head: x.heading.clone(),
                points: x.text.clone(),
            })
            .collect()
    } else {
        let (hl, hr) = comparison_titles(&s.title);
        let half = s.bullets.len().div_ceil(2);
        vec![
            Side {
                head: hl,
                points: s.bullets[..half].to_vec(),
            },
            Side {
                head: hr,
                points: s.bullets[half..].to_vec(),
            },
        ]
    };
    for side in &mut v {
        side.points.truncate(6);
    }
    v
}

pub(crate) fn comparison(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let sides = sides_of(s);
    match cx.t.look.compare {
        CompareStyle::Panels => panels(&mut b, &sides),
        CompareStyle::Rows => table(&mut b, &sides),
        CompareStyle::Fields => fields(&mut b, &sides),
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}

/// Bold display headings are estimated wider than the average glyph.
const HEAD_SAFE: f64 = 1.18;

fn pt_emu(pt: f64) -> i64 {
    (pt * EMU_PER_PT).round() as i64
}

/// Height of a list of bullet paragraphs, each with `before` points above.
fn points_height(t: &ThemeSpec, lines: &[String], pt: f64, w: i64, before: f64) -> i64 {
    if lines.is_empty() {
        return 0;
    }
    stack_height(lines, pt, w, factor_minor(t), 1.04, before) + pt_emu(before)
}

/// Heading size: the largest at which every heading holds one line (down to
/// `one_line_floor`); otherwise the largest at which each fits two lines.
fn fit_heading(t: &ThemeSpec, sides: &[Side], w: i64, max: f64, one_line_floor: f64, min: f64) -> f64 {
    let mut pt = max;
    while pt >= one_line_floor {
        if sides.iter().all(|s| line_count(&s.head, pt, w, factor_major(t) * HEAD_SAFE) <= 1) {
            return pt;
        }
        pt -= 0.5;
    }
    // Two lines at most: start at the one-line floor and shrink.
    let mut pt = one_line_floor;
    while pt > min {
        if sides.iter().all(|s| line_count(&s.head, pt, w, factor_major(t) * HEAD_SAFE) <= 2) {
            return pt;
        }
        pt -= 0.5;
    }
    min
}

/// The largest size at which both sides' points fit `h`.
fn fit_points(t: &ThemeSpec, sides: &[Side], w: i64, h: i64, max: f64, min: f64, before: f64) -> f64 {
    let mut pt = max;
    while pt > min {
        if sides.iter().all(|s| points_height(t, &s.points, pt, w, before) <= h) {
            return pt;
        }
        pt -= 0.5;
    }
    min
}

// ---------------------------------------------------------------------------
// Panels

#[derive(Clone, Copy, PartialEq)]
enum Flavor {
    /// A coloured chip inset in the panel (glass, outlined decks).
    Chip,
    /// The chip with a thick outline and a sticker VS badge.
    Sticker,
    /// A short accent tick over a plain heading (corporate banner decks).
    Edge,
    /// A compact pill label in a soft tile (bento decks).
    Pill,
}

fn panels(b: &mut B, sides: &[Side]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let hdr = t.look.header;
    let narrow = area.w < 8_000_000;
    let flavor = match t.card {
        CardStyle::Sticker => Flavor::Sticker,
        CardStyle::Bento => Flavor::Pill,
        _ if hdr == Header::Banner => Flavor::Edge,
        _ => Flavor::Chip,
    };
    let centred = hdr == Header::Centered;
    let gap = if narrow { 400_000 } else { GUTTER * 2 };
    let pad = 300_000;
    let pad_b = 300_000;
    let col_w = (area.w - gap) / 2;
    let inner_w = col_w - 2 * pad;
    let ind = if centred { 0 } else { 285_750 };
    let chip_h = 560_000;
    let pill_h = 440_000;

    // Heading type: one size for both sides.
    let (head_max, head_min, head_w) = match flavor {
        Flavor::Chip | Flavor::Sticker => (22.0, 14.0, col_w - 280_000 - 400_000),
        Flavor::Edge => (24.0, 16.0, inner_w),
        Flavor::Pill => (16.0, 12.0, inner_w - 440_000),
    };
    let head_box_h = match flavor {
        Flavor::Edge => pt_emu(2.0 * head_max * 1.2),
        Flavor::Pill => pt_emu(head_max * 1.3),
        _ => chip_h - 40_000,
    };
    let head_pt = match flavor {
        Flavor::Edge => fit_heading(t, sides, head_w, head_max, 18.0, head_min),
        _ => sides
            .iter()
            .map(|s| fit_pt(std::slice::from_ref(&s.head), head_w, head_box_h, head_max, head_min, factor_major(t), 1.0, 0.0))
            .fold(head_max, f64::min),
    };
    let edge_head_h = sides
        .iter()
        .map(|s| stack_height(std::slice::from_ref(&s.head), head_pt, head_w, factor_major(t) * HEAD_SAFE, 1.0, 0.0))
        .max()
        .unwrap_or(0);
    let offset = match flavor {
        Flavor::Chip | Flavor::Sticker => 140_000 + chip_h + 200_000,
        Flavor::Edge => pad + 130_000 + edge_head_h + 220_000,
        Flavor::Pill => pad + pill_h + 220_000,
    };
    let max_h = area.h - 100_000;
    let avail = max_h - offset - pad_b;
    let pt = fit_points(t, sides, inner_w - ind, avail, 22.0, 13.0, 10.0);
    let need = sides.iter().map(|s| points_height(t, &s.points, pt, inner_w - ind, 10.0)).max().unwrap_or(0);
    let panel_h = (offset + need + pad_b).clamp(2_900_000, max_h);
    let top = vcentre(R::new(area.x, area.y, area.w, max_h), panel_h);
    let cols = columns(R::new(area.x, top, area.w, panel_h), 2, gap);
    for (i, (col, side)) in cols.iter().zip(sides.iter()).enumerate() {
        let accent = 1 + i as u8;
        let (chip_fill, chip_ink) = inked_fill(t, Clr::accent(accent), 100, 3.0);
        b.card(&format!("Card {}", i + 1), *col, Role::Item(i));
        match flavor {
            Flavor::Chip | Flavor::Sticker => {
                let mut chip = Shape::new(
                    format!("Header {}", i + 1),
                    radius(t),
                    R::new(col.x + 140_000, col.y + 140_000, col.w - 280_000, chip_h),
                )
                .fill(Fill::Solid(chip_fill));
                if flavor == Flavor::Sticker {
                    chip.line = Some(Line::new(Clr::tx1(), 2.25));
                }
                let para = Para::from_runs(vec![Run::new(side.head.clone())
                    .size(head_pt)
                    .bold()
                    .font(FontRef::Major)
                    .color(chip_ink)])
                .no_bullet();
                chip.text = Some(
                    Text::one(if centred { para.align(Align::Center) } else { para })
                        .anchor(Anchor::Middle)
                        .insets(200_000, 0, 200_000, 0),
                );
                b.sp(chip, Role::Item(i));
            }
            Flavor::Edge => {
                b.sp(
                    Shape::deco(
                        format!("Header tick {}", i + 1),
                        Geom::Rect,
                        R::new(col.x + pad, col.y + pad, 520_000, 56_000),
                        Fill::Solid(Clr::accent(accent)),
                    ),
                    Role::Item(i),
                );
                b.sp(
                    Shape::text_box(
                        format!("Header {}", i + 1),
                        R::new(col.x + pad, col.y + pad + 130_000, inner_w, edge_head_h),
                        Text::one(
                            Para::from_runs(vec![Run::new(side.head.clone())
                                .size(head_pt)
                                .bold()
                                .font(FontRef::Major)
                                .color(Clr::tx1())])
                            .no_bullet(),
                        ),
                    ),
                    Role::Item(i),
                );
            }
            Flavor::Pill => {
                let est = (side.head.chars().count() as f64 * head_pt * factor_major(t) * 1.08 * EMU_PER_PT) as i64 + 440_000;
                let w = est.min(inner_w);
                let mut pill = Shape::new(
                    format!("Header {}", i + 1),
                    Geom::Round(50),
                    R::new(col.x + pad, col.y + pad, w, pill_h),
                )
                .fill(Fill::Solid(chip_fill));
                pill.text = Some(
                    Text::one(
                        Para::from_runs(vec![Run::new(side.head.clone())
                            .size(head_pt)
                            .bold()
                            .font(FontRef::Major)
                            .color(chip_ink)])
                        .no_bullet(),
                    )
                    .anchor(Anchor::Middle)
                    .insets(220_000, 0, 220_000, 0),
                );
                b.sp(pill, Role::Item(i));
            }
        }
        let paras: Vec<Para> = side
            .points
            .iter()
            .map(|l| {
                let run = Run::new(l.clone()).size(pt);
                if centred {
                    Para::from_runs(vec![run]).no_bullet().align(Align::Center).before(10.0)
                } else {
                    Para::from_runs(vec![run])
                        .bullet("\u{2022}", Clr::accent(accent))
                        .margin(285_750, -285_750)
                        .before(10.0)
                }
            })
            .collect();
        if !paras.is_empty() {
            b.sp(
                Shape::text_box(
                    format!("Points {}", i + 1),
                    R::new(col.x + pad, col.y + offset, inner_w, panel_h - offset - pad_b),
                    Text::new(paras),
                ),
                Role::Item(i),
            );
        }
    }
    // "VS" on the seam: level with the chips, or mid-panel for the others.
    let d = if narrow { 520_000 } else { 640_000 };
    let seam_x = area.x + col_w + gap / 2;
    let vs_y = match flavor {
        Flavor::Chip | Flavor::Sticker => top + 140_000 + chip_h / 2,
        _ => top + panel_h / 2,
    };
    let vs_rect = R::new(seam_x - d / 2, vs_y - d / 2, d, d);
    if flavor == Flavor::Sticker {
        b.sp(
            Shape::deco(
                "Versus shadow",
                Geom::Ellipse,
                R::new(vs_rect.x + 60_000, vs_rect.y + 60_000, d, d),
                Fill::Solid(Clr::tx1()),
            ),
            Role::Item(1),
        );
    }
    let mut vs = Shape::new("Versus", Geom::Ellipse, vs_rect).fill(Fill::Solid(Clr::bg1()));
    vs.line = Some(if flavor == Flavor::Sticker {
        Line::new(Clr::tx1(), 2.25)
    } else {
        Line::new(Clr::tx1().alpha(30), 1.0)
    });
    vs.decorative = true;
    vs.text = Some(
        Text::one(
            Para::from_runs(vec![Run::new("VS").size(if narrow { 12.0 } else { 14.0 }).bold().color(Clr::tx1())])
                .align(Align::Center)
                .no_bullet(),
        )
        .anchor(Anchor::Middle),
    );
    b.sp(vs, Role::Item(1));
}

// ---------------------------------------------------------------------------
// Rows (an open table)

fn table(b: &mut B, sides: &[Side]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let narrow = area.w < 8_000_000;
    let gap = if narrow { 380_000 } else { 600_000 };
    // Blueprint-style bracketed row labels need the room.
    let idx_w = if t.look.header == Header::Banner && area.w >= 9_000_000 { 560_000 } else { 0 };
    let col_w = (area.w - gap) / 2;
    let cell_w = col_w - idx_w;
    let head_pt = fit_heading(t, sides, col_w, 26.0, 18.0, 16.0);
    let head_h = sides
        .iter()
        .map(|s| stack_height(std::slice::from_ref(&s.head), head_pt, col_w, factor_major(t) * HEAD_SAFE, 1.0, 0.0))
        .max()
        .unwrap_or(0);
    let rule_gap = 110_000;
    let head_block = head_h + rule_gap + 28_575 + 30_000;
    let row_pad = 125_000;
    let aligned = sides.len() == 2 && sides[0].points.len() == sides[1].points.len();
    let heights = |pt: f64| -> Vec<Vec<i64>> {
        let own: Vec<Vec<i64>> = sides
            .iter()
            .map(|s| {
                s.points
                    .iter()
                    .map(|p| stack_height(std::slice::from_ref(p), pt, cell_w, factor_minor(t), 1.04, 0.0) + 2 * row_pad)
                    .collect()
            })
            .collect();
        if aligned {
            let n = own[0].len();
            let shared: Vec<i64> = (0..n).map(|r| own[0][r].max(own[1][r])).collect();
            vec![shared.clone(), shared]
        } else {
            own
        }
    };
    let max_h = area.h - 100_000;
    let mut pt = head_pt.min(22.0);
    while pt > 14.0 {
        let total = head_block + heights(pt).iter().map(|c| c.iter().sum::<i64>()).max().unwrap_or(0);
        if total <= max_h {
            break;
        }
        pt -= 0.5;
    }
    let rows = heights(pt);
    let total = head_block + rows.iter().map(|c| c.iter().sum::<i64>()).max().unwrap_or(0);
    let top = vcentre(R::new(area.x, area.y, area.w, max_h), total.min(max_h));
    let cols = columns(R::new(area.x, top, area.w, total), 2, gap);
    if let Some(sh) = sheet(t, "Table sheet", R::new(area.x - 160_000, top - 140_000, area.w + 320_000, total + 280_000)) {
        b.sp(sh, Role::Deco);
    }
    for (i, (col, side)) in cols.iter().zip(sides.iter()).enumerate() {
        let accent = 1 + i as u8;
        if i == 1 {
            // A quiet "vs" in the gutter, level with the headings.
            b.sp(
                Shape::text_box(
                    "Versus",
                    R::new(area.x + col_w, top, gap, head_h),
                    Text::one(
                        Para::from_runs(vec![Run::new("vs").size(14.0).italic().font(FontRef::Major).color(muted(60))])
                            .align(Align::Center)
                            .no_bullet(),
                    )
                    .anchor(Anchor::Bottom),
                ),
                Role::Item(1),
            );
        }
        b.sp(
            Shape::text_box(
                format!("Header {}", i + 1),
                R::new(col.x, top, col_w, head_h),
                Text::one(
                    Para::from_runs(vec![Run::new(side.head.clone())
                        .size(head_pt)
                        .bold()
                        .font(FontRef::Major)
                        .color(Clr::tx1())])
                    .no_bullet(),
                )
                .anchor(Anchor::Bottom),
            ),
            Role::Item(i),
        );
        let mut rule = Shape::deco(
            format!("Header rule {}", i + 1),
            Geom::Line,
            R::new(col.x, top + head_h + rule_gap, col_w, 0),
            Fill::None,
        );
        rule.line = Some(Line::new(Clr::accent(accent), 2.25));
        b.sp(rule, Role::Item(i));
        let mut y = top + head_block;
        for (r, point) in side.points.iter().enumerate() {
            let h = rows[i][r];
            if idx_w > 0 {
                b.sp(
                    Shape::text_box(
                        format!("Row label {}.{}", i + 1, r + 1),
                        R::new(col.x, y, idx_w, h),
                        Text::one(
                            Para::from_runs(vec![Run::new(format!("[{:02}]", r + 1))
                                .size(12.0)
                                .bold()
                                .spacing(0.6)
                                .color(accent_text(t, accent, 4.5))])
                            .no_bullet(),
                        )
                        .anchor(Anchor::Middle),
                    ),
                    Role::Item(i),
                );
            }
            b.sp(
                Shape::text_box(
                    format!("Point {}.{}", i + 1, r + 1),
                    R::new(col.x + idx_w, y + row_pad, cell_w, h - 2 * row_pad),
                    Text::one(Para::from_runs(vec![Run::new(point.clone()).size(pt)]).no_bullet()).anchor(Anchor::Middle),
                ),
                Role::Item(i),
            );
            let mut hair = Shape::deco(
                format!("Row rule {}.{}", i + 1, r + 1),
                Geom::Line,
                R::new(col.x, y + h, col_w, 0),
                Fill::None,
            );
            hair.line = Some(Line::new(Clr::tx1().alpha(22), 0.75));
            b.sp(hair, Role::Item(i));
            y += h;
        }
    }
}

// ---------------------------------------------------------------------------
// Fields

/// The two field colours with their inks: the primary accent, and a deep
/// tone (the deck's dark colour on light decks, a deepened accent on dark
/// ones) that is clearly not the first.
fn field_colours(t: &ThemeSpec) -> [(Clr, Clr); 2] {
    let a = inked_fill(t, Clr::accent(1), 100, 4.5);
    let a_hex = t.resolve(&a.0).0;
    let mut candidates: Vec<(Clr, u32)> = Vec::new();
    if !t.is_dark() {
        candidates.push((Clr::dk2(), 100));
    }
    candidates.extend([
        (Clr::accent(2), 55),
        (Clr::accent(3), 55),
        (Clr::accent(2), 100),
        (Clr::accent(3), 100),
        (Clr::accent(4), 100),
    ]);
    // The first candidate that stands clear of the first field; if none
    // does, the one furthest from it.
    let mut pick = inked_fill(t, candidates[0].0, candidates[0].1, 4.5);
    let mut best = 0.0;
    for (c, m) in candidates {
        let cand = inked_fill(t, c, m, 4.5);
        let sep = contrast(&t.resolve(&cand.0).0, &a_hex);
        if sep >= 1.6 {
            pick = cand;
            break;
        }
        if sep > best {
            best = sep;
            pick = cand;
        }
    }
    [a, pick]
}

fn fields(b: &mut B, sides: &[Side]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let narrow = area.w < 8_000_000;
    let sticker = t.card == CardStyle::Sticker;
    // Flush hard-edged fields for square decks, soft separate ones for
    // rounded and sticker decks.
    let gap = if sticker {
        300_000
    } else if t.radius >= 8 {
        150_000
    } else {
        0
    };
    let geom = if gap == 0 { Geom::Rect } else { radius(t) };
    let fw = (area.w - gap) / 2;
    let pad_x = if narrow { 300_000 } else { 420_000 };
    let pad_t = if narrow { 340_000 } else { 440_000 };
    let pad_b = 280_000;
    let inner_w = fw - 2 * pad_x;
    let head_pt = fit_heading(t, sides, inner_w, 38.0, 24.0, 20.0);
    let head_h = sides
        .iter()
        .map(|s| stack_height(std::slice::from_ref(&s.head), head_pt, inner_w, factor_major(t) * HEAD_SAFE, 1.0, 0.0))
        .max()
        .unwrap_or(0);
    let rule_y = pad_t + head_h + 150_000;
    let pts_y = rule_y + 38_100 + 240_000;
    let ind = 285_750;
    let h = area.h - 40_000;
    let avail = h - pts_y - pad_b;
    let pt = fit_points(t, sides, inner_w - ind, avail, (if narrow { 20.0 } else { 24.0f64 }).min(head_pt), 13.0, 9.0);
    let pts_h = sides.iter().map(|s| points_height(t, &s.points, pt, inner_w - ind, 9.0)).max().unwrap_or(0);
    let colours = field_colours(t);
    for (i, side) in sides.iter().enumerate() {
        let (fill, ink) = colours[i];
        let x = area.x + i as i64 * (fw + gap);
        let rect = R::new(x, area.y, fw, h);
        if sticker {
            b.sp(
                Shape::deco(
                    format!("Field shadow {}", i + 1),
                    geom.clone(),
                    R::new(rect.x + 80_000, rect.y + 80_000, rect.w, rect.h),
                    Fill::Solid(Clr::tx1()),
                ),
                Role::Item(i),
            );
        }
        let mut field = Shape::deco(format!("Field {}", i + 1), geom.clone(), rect, Fill::Solid(fill));
        if sticker {
            field.line = Some(Line::new(Clr::tx1(), 2.25));
        }
        b.sp(field, Role::Item(i));
        b.sp(
            Shape::text_box(
                format!("Field {} heading", i + 1),
                R::new(x + pad_x, area.y + pad_t, inner_w, head_h),
                Text::one(
                    Para::from_runs(vec![Run::new(side.head.clone())
                        .size(head_pt)
                        .bold()
                        .font(FontRef::Major)
                        .color(ink)])
                    .no_bullet(),
                ),
            ),
            Role::Item(i),
        );
        b.sp(
            Shape::deco(
                format!("Field rule {}", i + 1),
                Geom::Rect,
                R::new(x + pad_x, area.y + rule_y, 520_000, 38_100),
                Fill::Solid(ink.alpha(70)),
            ),
            Role::Item(i),
        );
        let paras: Vec<Para> = side
            .points
            .iter()
            .map(|l| {
                Para::from_runs(vec![Run::new(l.clone()).size(pt).color(ink)])
                    .bullet("\u{2013}", ink)
                    .margin(ind, -ind)
                    .before(9.0)
            })
            .collect();
        if !paras.is_empty() {
            b.sp(
                Shape::text_box(
                    format!("Field {} points", i + 1),
                    R::new(x + pad_x, area.y + h - pad_b - pts_h, inner_w, pts_h),
                    Text::new(paras).anchor(Anchor::Bottom),
                ),
                Role::Item(i),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::timeline::support::*;
    use super::*;
    use crate::deck::theme::THEMES;

    fn slide(left: usize, right: usize) -> DesignedSlide {
        let pts = |n: usize, side: &str| -> Vec<String> {
            [
                "Cloud by default",
                "Per-seat pricing that grows with every hire",
                "Opaque formulas",
                "Slow on large files",
                "No offline mode at all",
            ]
            .iter()
            .cycle()
            .take(n)
            .map(|p| format!("{p} {side}"))
            .collect()
        };
        let mut s = DesignedSlide::new("Build vs buy").kicker("Competition");
        s.layout = Some(LayoutKind::Comparison);
        s.blocks = vec![
            Block {
                heading: "Legacy suites".into(),
                text: pts(left, "(a)"),
                ..Default::default()
            },
            Block {
                heading: "Harbor".into(),
                text: pts(right, "(b)"),
                ..Default::default()
            },
        ];
        s
    }

    #[test]
    fn every_comparison_variant_builds_for_one_to_five_points_per_side_in_every_header() {
        for base in THEMES {
            for style in [CompareStyle::Panels, CompareStyle::Rows, CompareStyle::Fields] {
                for (h, th) in all_looks(base) {
                    let th = ThemeSpec { look: Look { compare: style, ..th.look }, ..th };
                    for (l, r) in [(1, 1), (2, 2), (3, 3), (5, 5), (3, 5), (5, 2), (4, 4)] {
                        let who = format!("{} {h:?} {style:?} {l}/{r}", base.id);
                        let built = comparison(&slide(l, r), &ctx(&th));
                        check_built(&th, &built, &who);
                        assert_reading_order(&built, 2, &who);
                    }
                }
            }
        }
    }

    #[test]
    fn a_comparison_from_bullets_splits_into_two_sides() {
        let th = *crate::deck::theme::theme("slate").unwrap();
        for style in [CompareStyle::Panels, CompareStyle::Rows, CompareStyle::Fields] {
            let th = ThemeSpec { look: Look { compare: style, ..th.look }, ..th };
            let mut s = DesignedSlide::new("Build vs buy").bullets(["Own the roadmap", "Slower start", "Ready tomorrow", "Vendor lock-in"]);
            s.layout = Some(LayoutKind::Comparison);
            let built = comparison(&s, &ctx(&th));
            check_built(&th, &built, "bullets");
            assert_reading_order(&built, 2, "bullets");
            s.bullets.clear();
            check_built(&th, &comparison(&s, &ctx(&th)), "empty");
        }
    }

    #[test]
    fn table_rows_line_up_when_both_sides_have_the_same_number_of_points() {
        let th = *crate::deck::theme::theme("ivory").unwrap();
        let built = comparison(&slide(3, 3), &ctx(&th));
        let y = |name: &str| {
            built
                .items
                .iter()
                .find_map(|i| match &i.el {
                    El::Sp(s) if s.name == name => s.rect.map(|r| (r.y, r.h)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no {name}"))
        };
        for r in 1..=3 {
            assert_eq!(y(&format!("Point 1.{r}")), y(&format!("Point 2.{r}")), "row {r} is shared");
        }
    }

    #[test]
    fn field_text_reads_on_the_field_in_every_theme_and_palette() {
        for base in THEMES {
            for pal in palettes() {
                let th = ThemeSpec { look: Look { compare: CompareStyle::Fields, ..base.look }, ..base.with_palette(pal) };
                let built = comparison(&slide(3, 3), &ctx(&th));
                let fill_of = |n: usize| -> Clr {
                    built
                        .items
                        .iter()
                        .find_map(|i| match &i.el {
                            El::Sp(s) if s.name == format!("Field {n}") => match &s.fill {
                                Fill::Solid(c) => Some(*c),
                                _ => None,
                            },
                            _ => None,
                        })
                        .unwrap()
                };
                let (a, bb) = (fill_of(1), fill_of(2));
                assert!(
                    contrast_of(&th, &a, &bb) >= 1.3,
                    "{} in {}: the two fields are one colour ({:.2})",
                    base.id,
                    pal.id,
                    contrast_of(&th, &a, &bb)
                );
                let mut checked = 0;
                for it in &built.items {
                    let El::Sp(sh) = &it.el else { continue };
                    for n in 1..=2 {
                        let heading = sh.name == format!("Field {n} heading");
                        let points = sh.name == format!("Field {n} points");
                        if !heading && !points {
                            continue;
                        }
                        let fill = if n == 1 { a } else { bb };
                        for p in &sh.text.as_ref().unwrap().paras {
                            for r in &p.runs {
                                let c = contrast_of(&th, &fill, r.color.as_ref().unwrap());
                                if heading {
                                    assert!(r.bold == Some(true) && r.size.unwrap() >= 18.0 && c >= 3.0, "{} in {}: heading {c:.2}", base.id, pal.id);
                                } else {
                                    assert!(c >= 4.5, "{} in {}: points {c:.2}", base.id, pal.id);
                                }
                                checked += 1;
                            }
                        }
                    }
                }
                assert!(checked >= 8, "{} in {}: nothing checked", base.id, pal.id);
            }
        }
    }

    #[test]
    fn chip_text_reads_on_panel_headers_in_every_palette() {
        for base in THEMES {
            for pal in palettes() {
                let th = ThemeSpec { look: Look { compare: CompareStyle::Panels, ..base.look }, ..base.with_palette(pal) };
                let built = comparison(&slide(2, 2), &ctx(&th));
                for it in &built.items {
                    let El::Sp(sh) = &it.el else { continue };
                    if !sh.name.starts_with("Header ") || sh.text.is_none() {
                        continue;
                    }
                    let Fill::Solid(fill) = &sh.fill else { continue };
                    for p in &sh.text.as_ref().unwrap().paras {
                        for r in &p.runs {
                            let c = contrast_of(&th, fill, r.color.as_ref().unwrap());
                            assert!(c >= 3.0 && r.bold == Some(true) && r.size.unwrap() >= 14.0, "{} in {}: chip {c:.2}", base.id, pal.id);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn comparison_titles_still_parse() {
        assert_eq!(comparison_titles("Build vs buy"), ("Build".into(), "buy".into()));
        assert_eq!(comparison_titles("Pricing"), ("Option A".into(), "Option B".into()));
    }
}
