//! Milestones in three genuinely different compositions.
//!
//! * **Spine**: a horizontal line with a node per step, the date above the
//!   line and the heading and detail below it. The header decides the
//!   dialect: centred decks centre every column, poster decks draw a heavy
//!   rule with square nodes, rail decks a hairline with small dots, the
//!   rest gradient lines with ring nodes.
//! * **Chevrons**: a consulting-style process. A row of arrows, one per
//!   step, carries the date or step number; heading and detail sit below
//!   (sticker decks outline the arrows, bento decks set the text in tiles).
//! * **Vertical**: dates in a left column, a rail with node dots, heading and
//!   detail to the right.
//!
//! A horizontal arrangement never squeezes: when the area cannot give every
//! step a readable column the steps split over two rows.

#![allow(unused_imports)]
use super::compare::{body_area, inked_fill, sheet};
use super::kit::*;
use crate::deck::grid::*;
use crate::deck::look::*;
use crate::deck::model::{split_stat, Block, DesignedSlide, LayoutKind};
use crate::deck::motion::Role;
use crate::deck::parts::LayoutPart;
use crate::deck::theme::{contrast, ThemeSpec};
use crate::deck::xml::*;
use crate::pptx::{ChartSpec, SlideImage};

/// One milestone.
struct Step {
    when: String,
    head: String,
    body: String,
}

fn steps_of(s: &DesignedSlide) -> Vec<Step> {
    if !s.blocks.is_empty() {
        s.blocks
            .iter()
            .take(6)
            .map(|b| Step {
                when: b.label.clone().unwrap_or_default(),
                head: b.heading.clone(),
                body: b.text.join(" "),
            })
            .collect()
    } else {
        s.bullets
            .iter()
            .take(6)
            .map(|x| {
                // "Q2 2026: launch beta" -> when + heading.
                let (w, h) = x.split_once(": ").or_else(|| x.split_once(" \u{2014} ")).unwrap_or(("", x));
                Step {
                    when: w.to_string(),
                    head: h.to_string(),
                    body: String::new(),
                }
            })
            .collect()
    }
}

/// Type sizes of one step cell, in points.
#[derive(Clone, Copy)]
struct Z {
    when: f64,
    head: f64,
    body: f64,
}

const MIN_Z: Z = Z {
    when: 11.0,
    head: 13.0,
    body: 11.0,
};
/// Text is estimated a little wide: viewers wrap with different metrics.
const SAFE: f64 = 1.08;
const HEAD_BEFORE: f64 = 5.0;
const BODY_BEFORE: f64 = 6.0;

fn half_pt(x: f64) -> f64 {
    (x * 2.0).floor() / 2.0
}

/// The largest size (up to `max`) at which every label fits its own box.
fn fit_each(labels: &[String], w: i64, h: i64, max: f64, min: f64, factor: f64) -> f64 {
    labels
        .iter()
        .map(|l| fit_pt(std::slice::from_ref(l), w, h, max, min, factor, 1.0, 0.0))
        .fold(max, f64::min)
}

fn pt_emu(pt: f64) -> i64 {
    (pt * EMU_PER_PT).round() as i64
}

/// Height in EMU of a step cell at sizes `z` in a box `w` wide. The date line
/// (caps, letter-spaced) is estimated wide.
fn cell_height(t: &ThemeSpec, st: &Step, z: Z, w: i64, with_when: bool) -> i64 {
    let mut pt = 0.0;
    let mut any = false;
    if with_when && !st.when.is_empty() {
        pt += line_count(&st.when, z.when, w, factor_minor(t) * 1.25) as f64 * z.when * 1.2;
        any = true;
    }
    if !st.head.is_empty() {
        pt += line_count(&st.head, z.head, w, factor_major(t) * SAFE) as f64 * z.head * 1.2;
        if any {
            pt += HEAD_BEFORE;
        }
        any = true;
    }
    if !st.body.is_empty() {
        pt += line_count(&st.body, z.body, w, factor_minor(t) * SAFE) as f64 * z.body * 1.2;
        if any {
            pt += BODY_BEFORE;
        }
    }
    pt_emu(pt)
}

/// The largest uniform sizes (every step gets the same type) at which every
/// cell fits `h` EMU.
fn fit_cells(t: &ThemeSpec, steps: &[Step], w: i64, h: i64, caps: Z, with_when: bool) -> Z {
    // First choice: every heading on one line (a heading never breaks into
    // one word per line while a smaller size would hold it); if that would
    // need a heading under 16 pt, accept two lines.
    for single in [true, false] {
        let mut k = 1.0f64;
        loop {
            let z = Z {
                when: half_pt((caps.when * k).max(MIN_Z.when)),
                head: half_pt((caps.head * k).max(MIN_Z.head)),
                body: half_pt((caps.body * k).max(MIN_Z.body)),
            };
            let need = steps.iter().map(|s| cell_height(t, s, z, w, with_when)).max().unwrap_or(0);
            let one_line = steps.iter().all(|s| line_count(&s.head, z.head, w, factor_major(t) * SAFE) <= 1);
            let floor = if single { 16.0 } else { MIN_Z.head };
            if need <= h && (!single || one_line) {
                return z;
            }
            if z.head <= floor && (z.body <= MIN_Z.body || single) {
                break;
            }
            k -= 0.04;
        }
    }
    MIN_Z
}

fn step_paras(t: &ThemeSpec, st: &Step, z: Z, accent: u8, centred: bool, with_when: bool) -> Vec<Para> {
    let al = |p: Para| if centred { p.align(Align::Center) } else { p };
    let mut v: Vec<Para> = Vec::new();
    if with_when && !st.when.is_empty() {
        v.push(al(Para::from_runs(vec![Run::new(st.when.clone())
            .size(z.when)
            .bold()
            .caps()
            .spacing(0.6)
            .color(accent_text(t, accent, 4.5))])
        .no_bullet()));
    }
    if !st.head.is_empty() {
        let mut p = Para::from_runs(vec![Run::new(st.head.clone())
            .size(z.head)
            .bold()
            .font(FontRef::Major)
            .color(Clr::tx1())])
        .no_bullet();
        if !v.is_empty() {
            p = p.before(HEAD_BEFORE);
        }
        v.push(al(p));
    }
    if !st.body.is_empty() {
        let mut p = Para::from_runs(vec![Run::new(st.body.clone()).size(z.body).color(muted(75))]).no_bullet();
        if !v.is_empty() {
            p = p.before(BODY_BEFORE);
        }
        v.push(al(p));
    }
    v
}

/// How many steps go on each row so that no column is narrower than
/// `min_pitch`; rows are balanced (3 + 3, not 5 + 1).
fn row_split(n: usize, w: i64, min_pitch: i64, gap: i64) -> Vec<usize> {
    let cap = (((w + gap) / (min_pitch + gap)).max(1)) as usize;
    if n <= cap {
        return vec![n];
    }
    let rows = n.div_ceil(cap);
    let per = n.div_ceil(rows);
    let mut left = n;
    let mut v = Vec::new();
    for _ in 0..rows {
        let k = per.min(left);
        v.push(k);
        left -= k;
    }
    v
}

pub(crate) fn timeline(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let steps = steps_of(s);
    if !steps.is_empty() {
        match cx.t.look.timeline {
            TimelineStyle::Spine => spine(&mut b, &steps),
            TimelineStyle::Chevrons => chevrons(&mut b, &steps),
            TimelineStyle::Vertical => vertical(&mut b, &steps),
        }
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}

// ---------------------------------------------------------------------------
// Spine

fn spine(b: &mut B, steps: &[Step]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let hdr = t.look.header;
    let centred = hdr == Header::Centered;
    let gap = GUTTER / 2;
    let rows = row_split(steps.len(), area.w, 1_750_000, gap);
    let per = rows[0];
    let nrows = rows.len();
    let row_gap = 360_000;
    let row_h = (area.h - row_gap * (nrows as i64 - 1)) / nrows as i64;
    let cols = columns(R::new(area.x, area.y, area.w, row_h), per, gap);
    let col_w = cols[0].w;
    let text_w = col_w - 100_000;
    let (node_d, line_h): (i64, i64) = match hdr {
        Header::Poster => (300_000, 50_800),
        Header::Rail => (190_000, 12_700),
        _ => (340_000, 38_100),
    };
    let n_gap = 140_000;
    let has_when = steps.iter().any(|s| !s.when.is_empty());
    let caps = match per {
        0..=3 => Z { when: 18.0, head: 26.0, body: 18.0 },
        4 => Z { when: 16.0, head: 22.0, body: 16.0 },
        _ => Z { when: 14.0, head: 19.0, body: 14.0 },
    };
    // The date sits on one line above the spine.
    let labels: Vec<String> = steps.iter().map(|s| s.when.clone()).collect();
    let when_pt = if has_when {
        let one_line = pt_emu(caps.when * 1.25);
        fit_each(&labels, text_w, one_line, caps.when, MIN_Z.when, factor_minor(t) * 1.25)
    } else {
        0.0
    };
    let when_h = if has_when { pt_emu(when_pt * 1.2) + n_gap } else { 0 };
    let budget = row_h - when_h - node_d - n_gap - 30_000;
    let mut z = fit_cells(t, steps, text_w, budget, caps, false);
    z.when = when_pt;
    let below_h = steps.iter().map(|s| cell_height(t, s, z, text_w, false)).max().unwrap_or(0);
    let block_h = when_h + node_d + n_gap + below_h;
    let total = block_h * nrows as i64 + row_gap * (nrows as i64 - 1);
    let top = vcentre(area, total);
    let mut base = 0usize;
    for (r, &cnt) in rows.iter().enumerate() {
        let row_top = top + r as i64 * (block_h + row_gap);
        let line_y = row_top + when_h + node_d / 2;
        let shift = if centred { (per - cnt) as i64 * (col_w + gap) / 2 } else { 0 };
        let fill = match hdr {
            Header::Poster => Fill::Solid(Clr::tx1()),
            Header::Rail => Fill::Solid(Clr::tx1().alpha(38)),
            _ => Fill::Linear {
                angle: 0,
                stops: vec![(0, Clr::accent(1)), (100, Clr::accent(2))],
            },
        };
        b.sp(
            Shape::deco("Spine", Geom::Rect, R::new(area.x, line_y - line_h / 2, area.w, line_h), fill),
            Role::Deco,
        );
        for (k, col) in cols.iter().take(cnt).enumerate() {
            let i = base + k;
            let st = &steps[i];
            let accent = 1 + (i as u8 % 4);
            let x = col.x + shift;
            let node_x = if centred { x + (col_w - node_d) / 2 } else { x };
            let mut node = match hdr {
                Header::Poster => Shape::new(
                    format!("Milestone {}", i + 1),
                    Geom::Rect,
                    R::new(node_x, line_y - node_d / 2, node_d, node_d),
                )
                .fill(Fill::Solid(Clr::accent(accent))),
                Header::Rail => Shape::new(
                    format!("Milestone {}", i + 1),
                    Geom::Ellipse,
                    R::new(node_x, line_y - node_d / 2, node_d, node_d),
                )
                .fill(Fill::Solid(Clr::accent(accent))),
                _ => {
                    let mut n = Shape::new(
                        format!("Milestone {}", i + 1),
                        Geom::Ellipse,
                        R::new(node_x, line_y - node_d / 2, node_d, node_d),
                    )
                    .fill(Fill::Solid(Clr::bg1()));
                    n.line = Some(Line::new(Clr::accent(accent), 3.5));
                    n
                }
            };
            node.decorative = true;
            b.sp(node, Role::Item(i));
            if has_when && !st.when.is_empty() {
                let w_rect = R::new(x, row_top, text_w, when_h - n_gap);
                let p = Para::from_runs(vec![Run::new(st.when.clone())
                    .size(when_pt)
                    .bold()
                    .caps()
                    .spacing(0.6)
                    .color(accent_text(t, accent, 4.5))])
                .no_bullet();
                let p = if centred { p.align(Align::Center) } else { p };
                b.sp(
                    Shape::text_box(
                        format!("Milestone date {}", i + 1),
                        w_rect,
                        Text::one(p).anchor(Anchor::Bottom),
                    ),
                    Role::Item(i),
                );
            }
            let paras = step_paras(t, st, z, accent, centred, false);
            if !paras.is_empty() {
                let rect = R::new(x, line_y + node_d / 2 + n_gap, text_w, below_h);
                b.sp(
                    Shape::text_box(format!("Milestone text {}", i + 1), rect, Text::new(paras)),
                    Role::Item(i),
                );
            }
        }
        base += cnt;
    }
}

// ---------------------------------------------------------------------------
// Chevrons

fn chevrons(b: &mut B, steps: &[Step]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let centred = t.look.header == Header::Centered;
    let bento = t.card == CardStyle::Bento;
    let sticker = t.card == CardStyle::Sticker;
    let gap = GUTTER / 2;
    let rows = row_split(steps.len(), area.w, 1_850_000, gap);
    let per = rows[0];
    let nrows = rows.len();
    let row_gap = 240_000;
    let row_h = (area.h - row_gap * (nrows as i64 - 1)) / nrows as i64;
    let ch_h: i64 = if nrows > 1 { 520_000 } else { 640_000 };
    let depth = ch_h / 2; // the preset's default point depth: half the short side
    let g = 45_000;
    let pitch = (area.w - depth + g) / per as i64;
    let sh_w = pitch + depth - g;
    let text_gap = 150_000;
    let pad = if bento { 150_000 } else { 0 };
    let text_w = pitch - 110_000 - 2 * pad;
    let caps = match per {
        0..=3 => Z { when: 0.0, head: 24.0, body: 17.0 },
        4 => Z { when: 0.0, head: 20.0, body: 15.0 },
        _ => Z { when: 0.0, head: 17.0, body: 14.0 },
    };
    let budget = row_h - ch_h - text_gap - 2 * pad - 20_000;
    let z = fit_cells(t, steps, text_w, budget, caps, false);
    let below_h = steps.iter().map(|s| cell_height(t, s, z, text_w, false)).max().unwrap_or(0);
    let block_h = ch_h + text_gap + below_h + 2 * pad;
    let total = block_h * nrows as i64 + row_gap * (nrows as i64 - 1);
    let top = vcentre(area, total);

    // The label inside an arrow: the date, else the step number.
    let labels: Vec<String> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| if s.when.is_empty() { format!("{:02}", i + 1) } else { s.when.clone() })
        .collect();
    let label_w = pitch - depth * 7 / 10 - depth - 80_000;
    let label_pt = labels
        .iter()
        .map(|l| fit_pt(std::slice::from_ref(l), label_w, ch_h * 9 / 10, 20.0, 11.0, factor_major(t), 1.0, 0.0))
        .fold(20.0, f64::min);

    let mut base = 0usize;
    for (r, &cnt) in rows.iter().enumerate() {
        let row_top = top + r as i64 * (block_h + row_gap);
        let shift = if centred { (per - cnt) as i64 * pitch / 2 } else { 0 };
        for k in 0..cnt {
            let i = base + k;
            let st = &steps[i];
            let x = area.x + shift + k as i64 * pitch;
            // Alternating two accents bands the process; the ink is chosen
            // so the label reads on either.
            let (fill, ink) = inked_fill(t, Clr::accent(1 + (i % 2) as u8), 100, 3.0);
            let (geom, ins) = if k == 0 {
                (Geom::HomePlate, (120_000, 0, depth * 7 / 10, 0))
            } else {
                (Geom::Chevron, (depth + 40_000, 0, depth * 7 / 10, 0))
            };
            let mut sh = Shape::new(format!("Chevron {}", i + 1), geom, R::new(x, row_top, sh_w, ch_h)).fill(Fill::Solid(fill));
            if sticker {
                sh.line = Some(Line::new(Clr::tx1(), 2.25));
            }
            sh.text = Some(
                Text::one(
                    Para::from_runs(vec![Run::new(labels[i].clone())
                        .size(label_pt)
                        .bold()
                        .font(FontRef::Major)
                        .color(ink)])
                    .no_bullet()
                    .align(Align::Center),
                )
                .anchor(Anchor::Middle)
                .insets(ins.0, ins.1, ins.2, ins.3),
            );
            b.sp(sh, Role::Item(i));
            // Text column below: from the arrow's visual left edge.
            let col_x = if centred {
                x + sh_w / 2 - (text_w + 2 * pad) / 2
            } else if k == 0 {
                x
            } else {
                x + depth / 2
            };
            let col_y = row_top + ch_h + text_gap;
            if bento {
                b.card(
                    &format!("Step tile {}", i + 1),
                    R::new(col_x, col_y, text_w + 2 * pad, below_h + 2 * pad),
                    Role::Item(i),
                );
            }
            let paras = step_paras(t, st, z, 1 + (i % 2) as u8, centred, false);
            if !paras.is_empty() {
                b.sp(
                    Shape::text_box(
                        format!("Milestone text {}", i + 1),
                        R::new(col_x + pad, col_y + pad, text_w, below_h),
                        Text::new(paras),
                    ),
                    Role::Item(i),
                );
            }
        }
        base += cnt;
    }
}

// ---------------------------------------------------------------------------
// Vertical

fn vertical(b: &mut B, steps: &[Step]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let n = steps.len();
    let hdr = t.look.header;
    let bracket = hdr == Header::Banner;
    let hairlines = t.card == CardStyle::Rule;
    let has_when = steps.iter().any(|s| !s.when.is_empty());
    let caps = match n {
        0..=3 => Z { when: 24.0, head: 24.0, body: 18.0 },
        4 => Z { when: 22.0, head: 21.0, body: 16.0 },
        _ => Z { when: 20.0, head: 18.0, body: 14.0 },
    };
    let node_d = 170_000;
    let when_w = if has_when { (area.w * 24 / 100).min(2_300_000) } else { 0 };
    let row_h = (area.h / n as i64).min(1_050_000);
    let rail_x = area.x + when_w + if has_when { 170_000 } else { 0 } + node_d / 2;
    let text_x = rail_x + node_d / 2 + 280_000;
    let text_w = area.right() - text_x;
    let z = fit_cells(t, steps, text_w, row_h - 120_000, caps, false);
    let first_h = pt_emu(z.head.max(z.body) * 1.2);
    // The date shares the heading's size so the two baselines meet.
    let labels: Vec<String> = steps
        .iter()
        .map(|s| if bracket && !s.when.is_empty() { format!("[{}]", s.when) } else { s.when.clone() })
        .collect();
    let when_pt = if has_when {
        fit_each(&labels, when_w, pt_emu(z.head * 1.3), caps.when.min(z.head + 4.0), MIN_Z.when, factor_major(t))
    } else {
        0.0
    };
    let pad = 40_000;
    let top = vcentre(area, row_h * n as i64);
    let node_y = |i: usize| top + i as i64 * row_h + pad + first_h / 2;
    if let Some(sh) = sheet(
        t,
        "Timeline sheet",
        R::new(area.x - 160_000, top - 140_000, area.w + 320_000, row_h * n as i64 + 280_000),
    ) {
        b.sp(sh, Role::Deco);
    }
    if n > 1 {
        let mut rail = Shape::deco(
            "Rail",
            Geom::Line,
            R::new(rail_x, node_y(0), 0, node_y(n - 1) - node_y(0)),
            Fill::None,
        );
        rail.line = Some(Line::new(Clr::tx1().alpha(28), 1.5));
        b.sp(rail, Role::Deco);
    }
    for (i, st) in steps.iter().enumerate() {
        let y = top + i as i64 * row_h;
        if hairlines && i > 0 {
            let mut rule = Shape::deco(format!("Row rule {}", i + 1), Geom::Line, R::new(text_x, y, text_w, 0), Fill::None);
            rule.line = Some(Line::new(Clr::tx1().alpha(16), 0.75));
            b.sp(rule, Role::Item(i));
        }
        let mut node = Shape::new(
            format!("Milestone {}", i + 1),
            Geom::Ellipse,
            R::new(rail_x - node_d / 2, node_y(i) - node_d / 2, node_d, node_d),
        )
        .fill(Fill::Solid(Clr::accent(1)));
        node.decorative = true;
        b.sp(node, Role::Item(i));
        if has_when && !labels[i].is_empty() {
            let mut run = Run::new(labels[i].clone())
                .size(when_pt)
                .font(FontRef::Major)
                .color(accent_text(t, 1, 4.5));
            if t.bold_titles {
                run = run.bold();
            }
            b.sp(
                Shape::text_box(
                    format!("Milestone date {}", i + 1),
                    R::new(area.x, y + pad, when_w, row_h - pad),
                    Text::one(Para::from_runs(vec![run]).no_bullet()),
                ),
                Role::Item(i),
            );
        }
        let paras = step_paras(t, st, z, 1, false, false);
        if !paras.is_empty() {
            b.sp(
                Shape::text_box(
                    format!("Milestone text {}", i + 1),
                    R::new(text_x, y + pad, text_w, row_h - pad),
                    Text::new(paras),
                ),
                Role::Item(i),
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod support {
    //! Shared by the flow layouts' tests (timeline, compare, agenda).
    use super::super::kit::*;
    use crate::deck::grid::*;
    use crate::deck::look::*;
    use crate::deck::model::*;
    use crate::deck::motion::Role;
    use crate::deck::palette::PALETTES;
    use crate::deck::theme::*;
    use crate::deck::xml::*;

    pub fn headers() -> [Header; 7] {
        [
            Header::TopLeft,
            Header::Banner,
            Header::Sidebar { right: false },
            Header::Sidebar { right: true },
            Header::Rail,
            Header::Centered,
            Header::Poster,
        ]
    }

    pub fn ctx<'a>(t: &'a ThemeSpec) -> Ctx<'a> {
        Ctx {
            t,
            frame: frame(&t.look),
            rtl: false,
            footer: "Harbor".into(),
            numbers: true,
            index: 3,
            deck_title: "Harbor",
            section_no: 1,
        }
    }

    /// Does the text of `sh` fit its box (same estimate as the generic gate)?
    pub fn overflow(t: &ThemeSpec, sh: &Shape) -> Option<(i64, i64)> {
        let text = sh.text.as_ref()?;
        let rect = sh.rect?;
        let (l, tp, r, b) = text.insets;
        let base_w = rect.w - l - r;
        let avail_h = rect.h - tp - b;
        let mut need = 0.0f64;
        for p in &text.paras {
            let s: String = p.runs.iter().map(|r| r.text.as_str()).collect();
            let pt = p
                .runs
                .iter()
                .filter_map(|r| r.size)
                .fold(0.0, f64::max)
                .max(if p.runs.iter().any(|r| r.size.is_some()) { 0.0 } else { 18.0 });
            let major = p.runs.first().is_some_and(|r| r.font == Some(FontRef::Major));
            let factor = glyph_factor(if major { t.fonts.major } else { t.fonts.minor });
            let w = base_w - p.margin.map_or(0, |(m, _)| m.max(0));
            let lines = line_count(&s, pt, w.max(1), factor) as f64;
            let spacing = p.line.map_or(1.0, |l| l as f64 / 100.0);
            need += lines * pt * 1.2 * spacing + p.before.unwrap_or(0.0) + p.after.unwrap_or(0.0);
        }
        let need = (need * EMU_PER_PT) as i64;
        if need as f64 > avail_h as f64 * 1.06 + 25_000.0 {
            Some((need, avail_h))
        } else {
            None
        }
    }

    /// Geometry and fit checks every flow layout must pass.
    pub fn check_built(t: &ThemeSpec, built: &Built, who: &str) {
        for it in &built.items {
            let El::Sp(sh) = &it.el else { continue };
            if let Some(r) = sh.rect {
                assert!(r.w >= 0 && r.h >= 0, "{who} '{}': negative {r:?}", sh.name);
                assert!(
                    r.x >= 0 && r.y >= 0 && r.right() <= SLIDE_W && r.bottom() <= SLIDE_H,
                    "{who} '{}': off page {r:?}",
                    sh.name
                );
            }
            if sh.ph.is_none() {
                if let Some((need, have)) = overflow(t, sh) {
                    panic!("{who} '{}': text needs {need} but box has {have}", sh.name);
                }
            }
        }
    }

    /// Item indices of the non-title content, in emission order.
    pub fn item_order(built: &Built) -> Vec<usize> {
        built
            .items
            .iter()
            .filter_map(|i| match i.role {
                Role::Item(n) => Some(n),
                _ => None,
            })
            .collect()
    }

    pub fn assert_reading_order(built: &Built, count: usize, who: &str) {
        let order = item_order(built);
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "{who}: items out of order {order:?}");
        let mut seen: Vec<usize> = order.clone();
        seen.dedup();
        assert_eq!(seen, (0..count).collect::<Vec<_>>(), "{who}: item tags");
    }

    pub fn all_looks(base: &ThemeSpec) -> Vec<(Header, ThemeSpec)> {
        headers().into_iter().map(|h| (h, ThemeSpec { look: Look { header: h, ..base.look }, ..*base })).collect()
    }

    pub fn palettes() -> &'static [crate::deck::palette::PaletteSpec] {
        PALETTES
    }

    /// Contrast between a fill and a text colour as the deck paints them.
    pub fn contrast_of(t: &ThemeSpec, fill: &Clr, ink: &Clr) -> f64 {
        contrast(&t.resolve(fill).0, &t.resolve(ink).0)
    }
}

#[cfg(test)]
mod tests {
    use super::support::*;
    use super::*;
    use crate::deck::theme::THEMES;

    fn slide(n: usize) -> DesignedSlide {
        let mut s = DesignedSlide::new("From beta to scale in four quarters").kicker("Go to market");
        s.layout = Some(LayoutKind::Timeline);
        s.blocks = (0..n)
            .map(|i| {
                Block::step(
                    format!("Q{} 2026", i + 1),
                    ["Private beta", "Public launch", "Teams and sync", "EMEA expansion", "Marketplace", "Open platform"][i % 6],
                    "Ten design partners, shared workspaces and Arabic first-class",
                )
            })
            .collect();
        s
    }

    #[test]
    fn every_timeline_variant_builds_for_two_to_six_steps_in_every_header() {
        for base in THEMES {
            for style in [TimelineStyle::Spine, TimelineStyle::Chevrons, TimelineStyle::Vertical] {
                for (h, th) in all_looks(base) {
                    let th = ThemeSpec { look: Look { timeline: style, ..th.look }, ..th };
                    for n in 2..=6 {
                        let who = format!("{} {h:?} {style:?} n={n}", base.id);
                        let built = timeline(&slide(n), &ctx(&th));
                        check_built(&th, &built, &who);
                        assert_reading_order(&built, n, &who);
                    }
                }
            }
        }
    }

    #[test]
    fn timelines_without_dates_or_details_still_build() {
        let th = *crate::deck::theme::theme("harbor").unwrap();
        for style in [TimelineStyle::Spine, TimelineStyle::Chevrons, TimelineStyle::Vertical] {
            let th = ThemeSpec { look: Look { timeline: style, ..th.look }, ..th };
            let mut s = DesignedSlide::new("Plan").bullets(["Discover", "Build the beta", "Launch in EMEA"]);
            s.layout = Some(LayoutKind::Timeline);
            check_built(&th, &timeline(&s, &ctx(&th)), "bullets");
            s.bullets.clear();
            check_built(&th, &timeline(&s, &ctx(&th)), "empty");
        }
    }

    #[test]
    fn narrow_areas_split_horizontal_timelines_over_two_rows_instead_of_squeezing() {
        let th = *crate::deck::theme::theme("terracotta").unwrap();
        for style in [TimelineStyle::Spine, TimelineStyle::Chevrons] {
            let th = ThemeSpec { look: Look { timeline: style, ..th.look }, ..th };
            let built = timeline(&slide(6), &ctx(&th));
            let mut ys: Vec<i64> = built
                .items
                .iter()
                .filter_map(|i| match &i.el {
                    El::Sp(s) if s.name.starts_with("Milestone ") && s.name[10..].chars().all(|c| c.is_ascii_digit()) => s.rect.map(|r| r.y),
                    El::Sp(s) if s.name.starts_with("Chevron") => s.rect.map(|r| r.y),
                    _ => None,
                })
                .collect();
            ys.sort();
            ys.dedup();
            assert_eq!(ys.len(), 2, "{style:?}: six steps in a narrow area use two rows, got {ys:?}");
        }
    }

    #[test]
    fn chevron_labels_read_on_their_fill_in_every_theme_and_palette() {
        for base in THEMES {
            for pal in palettes() {
                let th = ThemeSpec { look: Look { timeline: TimelineStyle::Chevrons, ..base.look }, ..base.with_palette(pal) };
                for n in [3usize, 6] {
                    let built = timeline(&slide(n), &ctx(&th));
                    let mut seen = 0;
                    for it in &built.items {
                        let El::Sp(sh) = &it.el else { continue };
                        if !sh.name.starts_with("Chevron") {
                            continue;
                        }
                        let Fill::Solid(fill) = &sh.fill else { panic!("chevron without a solid fill") };
                        for p in &sh.text.as_ref().unwrap().paras {
                            for r in &p.runs {
                                let c = contrast_of(&th, fill, r.color.as_ref().unwrap());
                                assert!(
                                    r.bold == Some(true) && r.size.unwrap() >= 14.0 && c >= 3.0,
                                    "{} in {}: chevron label {c:.2} at {:?} pt",
                                    base.id,
                                    pal.id,
                                    r.size
                                );
                            }
                        }
                        seen += 1;
                    }
                    assert_eq!(seen, n);
                }
            }
        }
    }

    #[test]
    fn date_text_in_every_palette_is_accent_only_when_it_reads() {
        for base in THEMES {
            for pal in palettes() {
                let th = base.with_palette(pal);
                for style in [TimelineStyle::Spine, TimelineStyle::Vertical] {
                    let th = ThemeSpec { look: Look { timeline: style, ..th.look }, ..th };
                    let built = timeline(&slide(4), &ctx(&th));
                    let page = page_hex(&th);
                    for it in &built.items {
                        let El::Sp(sh) = &it.el else { continue };
                        if !sh.name.starts_with("Milestone date") {
                            continue;
                        }
                        let clr = sh.text.as_ref().unwrap().paras[0].runs[0].color.unwrap();
                        let c = contrast(&th.resolve(&clr).0, page);
                        assert!(c >= 4.5, "{} in {} {style:?}: date contrast {c:.2}", base.id, pal.id);
                    }
                }
            }
        }
    }
}
