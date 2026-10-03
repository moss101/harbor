//! Ideas and people: boxed cards, stickers, numbered rows, open columns,
//! bento tiles, and the team slide.
//!
//! * **Grid** — boxed cards with a number badge; restacks as horizontal
//!   cards (badge left, text right) or a 2 x 2 grid when the area is narrow.
//! * **Stickers** — the same cells, but each card is tilted a degree or so
//!   with a hard outline and offset shadow (the theme's chrome).
//! * **Rows** — numbered rows: big numeral, heading, text, a hairline between.
//! * **Columns** — unboxed columns, thin rules between, the column head
//!   (bar, ring, numeral, bracket) chosen by the header composition.
//! * **Bento** — one large tile and smaller ones beside it.
//! * Team: **Cards** (boxed, initials avatar) or **Inline** (open, large
//!   initials circle with name, role and bio).

#![allow(unused_imports)]
use crate::deck::grid::*;
use crate::deck::look::*;
use crate::deck::model::{split_stat, Block, DesignedSlide, LayoutKind};
use crate::deck::motion::Role;
use crate::deck::parts::LayoutPart;
use crate::deck::theme::{contrast, ThemeSpec};
use crate::deck::xml::*;
use crate::pptx::{ChartSpec, SlideImage};
use super::kit::*;
use super::stats::{accent_on, card_surface, chrome_card, field_reads, grid_backing, hex_of, hline, soft_ink, vline};

const PAD: i64 = 280_000;
const BADGE: i64 = 440_000;

pub(crate) fn cards(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let blocks: Vec<Block> = blocks_of(s).into_iter().take(6).collect();
    if !blocks.is_empty() {
        match cx.t.look.cards {
            CardsStyle::Grid => grid(&mut b, &blocks, false),
            CardsStyle::Stickers => grid(&mut b, &blocks, true),
            CardsStyle::Rows => numbered_rows(&mut b, &blocks),
            CardsStyle::Columns => open_columns(&mut b, &blocks),
            CardsStyle::Bento => bento(&mut b, &blocks),
        }
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}

// ---------------------------------------------------------------------------
// Text fitting shared by the variants

fn heading_h(t: &ThemeSpec, blk: &Block, hpt: f64, w: i64) -> i64 {
    if blk.heading.is_empty() {
        return 0;
    }
    let lines = line_count(&blk.heading, hpt, w, factor_major(t) * 1.08);
    (lines as f64 * hpt * 1.2 * EMU_PER_PT).round() as i64
}

/// Height of a card's heading + body at the given sizes (conservative: the
/// body is set at 1.1 leading).
fn card_need(t: &ThemeSpec, blk: &Block, w: i64, hpt: f64, bpt: f64) -> i64 {
    let hh = heading_h(t, blk, hpt, w);
    let body = if blk.text.is_empty() {
        0
    } else {
        stack_height(&blk.text, bpt, w, factor_minor(t), 1.1, 4.0)
            + if hh > 0 { (6.0 * EMU_PER_PT) as i64 } else { 0 }
    };
    hh + body
}

/// Common heading/body sizes at which every block fits `w` x `avail`
/// (after `lead` EMU reserved for something above the text): heading and
/// body shrink together. Returns (heading pt, body pt, tallest need).
#[allow(clippy::too_many_arguments)]
fn fit_texts(
    t: &ThemeSpec,
    blocks: &[Block],
    w: i64,
    avail: i64,
    (hmax, hmin): (f64, f64),
    (bmax, bmin): (f64, f64),
    lead: i64,
) -> (f64, f64, i64) {
    let mut step = 0.0;
    loop {
        let hpt = (hmax - step).max(hmin);
        let bpt = (bmax - step * 0.75).max(bmin);
        let need = blocks.iter().map(|b| card_need(t, b, w, hpt, bpt)).max().unwrap_or(0) + lead;
        if need <= avail || (hpt <= hmin && bpt <= bmin) {
            return (hpt, bpt, need);
        }
        step += 0.5;
    }
}

/// The paragraphs of one card: optional lead line, heading, body lines.
fn card_paras(blk: &Block, hpt: f64, bpt: f64, al: Align, hclr: Clr, bclr: Clr, lead: Option<Para>) -> Vec<Para> {
    let mut v: Vec<Para> = Vec::new();
    if let Some(l) = lead {
        v.push(l.align(al));
    }
    if !blk.heading.is_empty() {
        v.push(
            Para::from_runs(vec![Run::new(blk.heading.clone()).size(hpt).bold().font(FontRef::Major).color(hclr)])
                .no_bullet()
                .align(al),
        );
    }
    for (j, line) in blk.text.iter().enumerate() {
        let mut p = Para::from_runs(vec![Run::new(line.clone()).size(bpt).color(bclr)]).no_bullet().align(al);
        p = if j == 0 && !blk.heading.is_empty() { p.before(6.0) } else { p.before(4.0) };
        v.push(p);
    }
    v
}

/// A numeral/label run in the lead line of a card.
fn lead_para(t: &ThemeSpec, text: &str, pt: f64, clr: Clr, after: f64, major: bool, spacing: Option<f64>) -> Para {
    let mut r = Run::new(text).size(pt).color(clr);
    if major {
        r = r.font(FontRef::Major);
    }
    r = if t.bold_titles || !major { r.bold() } else { r.regular() };
    if let Some(s) = spacing {
        r = r.spacing(s);
    }
    Para::from_runs(vec![r]).no_bullet().after(after)
}

fn lead_h(pt: f64, after: f64) -> i64 {
    ((pt * 1.2 + after) * EMU_PER_PT).round() as i64
}

fn is_centred(b: &B) -> bool {
    b.cx.frame.title_align == Align::Center
}

/// Rotate `s` by `deg` about `c` (the card's centre) so a tilted card's
/// contents stay put on it. Right-to-left decks mirror the geometry, which
/// flips the direction of the tilt.
fn rotate_about(mut s: Shape, c: (i64, i64), deg: f64, rtl: bool) -> Shape {
    if let Some(r) = s.rect {
        let (sx, sy) = (r.x + r.w / 2, r.y + r.h / 2);
        let th = deg.to_radians();
        let (dx, dy) = ((sx - c.0) as f64, (sy - c.1) as f64);
        let nx = c.0 as f64 + dx * th.cos() - dy * th.sin();
        let ny = c.1 as f64 + dx * th.sin() + dy * th.cos();
        s.rect = Some(R::new(nx.round() as i64 - r.w / 2, ny.round() as i64 - r.h / 2, r.w, r.h));
        s.rot += if rtl { -deg } else { deg };
    }
    s
}

// ---------------------------------------------------------------------------
// Grid and stickers

struct Plan {
    per: usize,
    horiz: bool,
    cell_w: i64,
    rh: i64,
    hpt: f64,
    bpt: f64,
    inner_w: i64,
}

/// Room under a vertical card's text; tighter when there are many cards.
fn bottom_pad(n: usize) -> i64 {
    if n >= 5 {
        220_000
    } else {
        300_000
    }
}

fn plan_grid(t: &ThemeSpec, blocks: &[Block], area: R, slack: i64) -> Plan {
    // First with comfortable minimum sizes; only if no arrangement fits,
    // let the type go a little smaller.
    let (a, ok) = plan_grid_at(t, blocks, area, slack, (16.0, 12.0));
    if ok {
        return a;
    }
    let (c, ok) = plan_grid_at(t, blocks, area, slack, (14.0, 10.0));
    if ok {
        return c;
    }
    // Nothing fits: the arrangement of the second pass that overflows least.
    c
}

fn plan_grid_at(t: &ThemeSpec, blocks: &[Block], area: R, slack: i64, lo: (f64, f64)) -> (Plan, bool) {
    let n = blocks.len();
    let cands: &[(usize, bool)] = match n {
        1 => &[(1, false)],
        2 => &[(2, false), (1, true)],
        3 => &[(3, false), (1, true)],
        4 => &[(4, false), (2, false), (1, true)],
        _ => &[(3, false), (3, true), (2, true), (1, true)],
    };
    let mut fallback: Option<(i64, Plan)> = None;
    for &(per, horiz) in cands {
        let rows = n.div_ceil(per);
        let full_w = (area.w - GUTTER * (per as i64 - 1)) / per as i64;
        let cell_w = if !horiz && n <= 2 { full_w.min(4_700_000) } else { full_w };
        let avail_h = (area.h - 80_000 - GUTTER * (rows as i64 - 1)) / rows as i64;
        let (eff_w, eff_h) = (cell_w - slack, avail_h - slack);
        let (inner_w, inner_h, min_w, content_extra) = if horiz {
            (eff_w - 2 * PAD - BADGE - 220_000, eff_h - 2 * PAD, 1_900_000, 2 * PAD)
        } else {
            let extra = 280_000 + BADGE + 180_000 + bottom_pad(n);
            (eff_w - 2 * PAD, eff_h - extra, 1_650_000, extra)
        };
        let (hpt, bpt, need) = if horiz {
            fit_texts(t, blocks, inner_w, inner_h, (22.0, lo.0), (17.0, lo.1), 0)
        } else {
            fit_texts(t, blocks, inner_w, inner_h, (24.0, lo.0), (18.0, lo.1), 0)
        };
        let min_h = if horiz { 1_000_000 } else { 2_500_000 };
        let rh = (content_extra + need + slack).max(min_h + slack).min(avail_h);
        let plan = Plan { per, horiz, cell_w, rh, hpt, bpt, inner_w };
        if inner_w >= min_w && need <= inner_h {
            return (plan, true);
        }
        // Nothing fits: keep the candidate that overflows least.
        let deficit = need - inner_h + if inner_w < min_w { 1_000_000 } else { 0 };
        if fallback.as_ref().is_none_or(|(d, _)| deficit < *d) {
            fallback = Some((deficit, plan));
        }
    }
    (fallback.expect("at least one candidate").1, false)
}

fn grid(b: &mut B, blocks: &[Block], sticker: bool) {
    let cx = b.cx;
    let t = cx.t;
    let area = cx.frame.body;
    let n = blocks.len();
    let centred = is_centred(b);
    let surf = card_surface(t);
    // Stickers need room for the hard shadow and the tilt.
    let slack = if !sticker { 0 } else if n >= 5 { 100_000 } else { 150_000 };
    let plan = plan_grid(t, blocks, area, slack);
    let rows = n.div_ceil(plan.per);
    let block_h = plan.rh * rows as i64 + GUTTER * (rows as i64 - 1);
    let top = vcentre(R::new(area.x, area.y, area.w, area.h - 100_000), block_h);
    let group_w = plan.per as i64 * plan.cell_w + GUTTER * (plan.per as i64 - 1);
    let x0 = if centred { area.x + (area.w - group_w) / 2 } else { area.x };
    let tilt = [-1.5, 1.1, -0.8, 1.4, -1.2, 0.9];
    let drop: [i64; 6] = if slack >= 150_000 { [0, 90_000, 30_000, 70_000, 0, 60_000] } else { [0, 40_000, 15_000, 30_000, 0, 25_000] };
    let al = if centred && !plan.horiz { Align::Center } else { Align::Left };

    for (i, blk) in blocks.iter().enumerate() {
        let (r, c) = (i / plan.per, i % plan.per);
        let in_row = if r + 1 == rows { n - r * plan.per } else { plan.per };
        let shift = if centred { (plan.per - in_row) as i64 * (plan.cell_w + GUTTER) / 2 } else { 0 };
        let accent = 1 + (i as u8 % 6);
        let y_off = if sticker { drop[i % 6] } else { 0 };
        let rect = R::new(
            x0 + shift + c as i64 * (plan.cell_w + GUTTER),
            top + r as i64 * (plan.rh + GUTTER) + y_off,
            plan.cell_w - slack,
            plan.rh - slack,
        );
        let mut shapes = chrome(t, &format!("Card {}", i + 1), rect);
        let (badge_rect, text_rect) = if plan.horiz {
            (
                R::new(rect.x + PAD, rect.y + (rect.h - BADGE) / 2, BADGE, BADGE),
                R::new(rect.x + PAD + BADGE + 220_000, rect.y + PAD, plan.inner_w, rect.h - 2 * PAD),
            )
        } else {
            let bx = if centred { rect.x + (rect.w - BADGE) / 2 } else { rect.x + PAD };
            let ty = rect.y + 280_000 + BADGE + 180_000;
            (
                R::new(bx, rect.y + 280_000, BADGE, BADGE),
                R::new(rect.x + PAD, ty, plan.inner_w, rect.h - (ty - rect.y) - bottom_pad(n)),
            )
        };
        let mut bd = badge(t, &format!("Number {}", i + 1), badge_rect, accent, &format!("{}", i + 1), 16.0);
        if sticker {
            bd.line = Some(Line::new(Clr::tx1(), 1.75));
        }
        shapes.push(bd);
        let paras = card_paras(blk, plan.hpt, plan.bpt, al, Clr::tx1(), soft_ink(t, 78, &surf), None);
        let anchor = if plan.horiz { Anchor::Middle } else { Anchor::Top };
        shapes.push(Shape::text_box(format!("Card text {}", i + 1), text_rect, Text::new(paras).anchor(anchor)));
        let centre = (rect.x + rect.w / 2, rect.y + rect.h / 2);
        for sh in shapes {
            let sh = if sticker { rotate_about(sh, centre, tilt[i % 6], cx.rtl) } else { sh };
            b.sp(sh, Role::Item(i));
        }
    }
}

// ---------------------------------------------------------------------------
// Numbered rows

fn numbered_rows(b: &mut B, blocks: &[Block]) {
    let cx = b.cx;
    let t = cx.t;
    let area = cx.frame.body;
    let page = page_hex(t).to_string();
    let n = blocks.len();
    let wide = area.w >= 7_600_000;
    let num_w = if wide { 900_000 } else { 780_000 };
    let cap = match n {
        1..=3 => 1_550_000,
        4 => 1_200_000,
        _ => 950_000,
    };
    let rh = ((area.h - 120_000) / n as i64).min(cap);
    let top = vcentre(area, rh * n as i64);
    let pad_y = 100_000;
    let inner_h = rh - 2 * pad_y;
    let npt = (if wide { 32.0f64 } else { 26.0 }).min(inner_h as f64 / EMU_PER_PT / 1.3);
    let body_clr = soft_ink(t, 78, &page);
    let num_clr = accent_on(t, 1, 3.0, &page);

    // Wide: numeral | heading | text in three columns; narrow: heading
    // over text beside the numeral.
    let hw = ((area.w - num_w) * 36 / 100).clamp(2_200_000, 3_400_000);
    let bw = area.w - num_w - hw - 240_000;
    let tw = area.w - num_w - 120_000;
    let (hpt, bpt) = if wide {
        let mut h = 24.0f64;
        let mut bp = 18.0f64;
        for blk in blocks {
            if !blk.heading.is_empty() {
                h = h.min(fit_pt(std::slice::from_ref(&blk.heading), hw - 120_000, inner_h, 24.0, 16.0, factor_major(t) * 1.08, 1.0, 0.0));
            }
            if !blk.text.is_empty() {
                let w = if blk.heading.is_empty() { hw + bw } else { bw };
                bp = bp.min(fit_pt(&blk.text, w, inner_h, 18.0, 12.0, factor_minor(t), 1.1, 4.0));
            }
        }
        (h, bp)
    } else {
        let (h, bp, _) = fit_texts(t, blocks, tw, inner_h, (22.0, 15.0), (16.0, 12.0), 0);
        (h, bp)
    };

    for (i, blk) in blocks.iter().enumerate() {
        let y = top + i as i64 * rh;
        if i == 0 {
            b.sp(hline("Rule top", area.x, y, area.w, Clr::tx1().alpha(70), 1.5), Role::Item(0));
        }
        b.sp(
            Shape::text_box(
                format!("Number {}", i + 1),
                R::new(area.x, y + pad_y, num_w, inner_h),
                Text::one(
                    Para::from_runs(vec![{
                        let r = Run::new(format!("{:02}", i + 1)).size(npt).font(FontRef::Major).color(num_clr);
                        if t.bold_titles {
                            r.bold()
                        } else {
                            r.regular()
                        }
                    }])
                    .no_bullet(),
                )
                .anchor(Anchor::Middle),
            ),
            Role::Item(i),
        );
        if wide {
            if !blk.heading.is_empty() {
                b.sp(
                    Shape::text_box(
                        format!("Card heading {}", i + 1),
                        R::new(area.x + num_w, y + pad_y, hw - 120_000, inner_h),
                        Text::one(
                            Para::from_runs(vec![Run::new(blk.heading.clone()).size(hpt).bold().font(FontRef::Major).color(Clr::tx1())])
                                .no_bullet(),
                        )
                        .anchor(Anchor::Middle),
                    ),
                    Role::Item(i),
                );
            }
            if !blk.text.is_empty() {
                let (bx, w) = if blk.heading.is_empty() { (area.x + num_w, hw + bw) } else { (area.x + num_w + hw + 120_000, bw) };
                let paras: Vec<Para> = blk
                    .text
                    .iter()
                    .enumerate()
                    .map(|(j, l)| {
                        let p = Para::from_runs(vec![Run::new(l.clone()).size(bpt).color(body_clr)]).no_bullet();
                        if j == 0 {
                            p
                        } else {
                            p.before(4.0)
                        }
                    })
                    .collect();
                b.sp(
                    Shape::text_box(format!("Card text {}", i + 1), R::new(bx, y + pad_y, w, inner_h), Text::new(paras).anchor(Anchor::Middle)),
                    Role::Item(i),
                );
            }
        } else {
            let paras = card_paras(blk, hpt, bpt, Align::Left, Clr::tx1(), body_clr, None);
            b.sp(
                Shape::text_box(
                    format!("Card text {}", i + 1),
                    R::new(area.x + num_w, y + pad_y, tw, inner_h),
                    Text::new(paras).anchor(Anchor::Middle),
                ),
                Role::Item(i),
            );
        }
        let last = i + 1 == n;
        b.sp(
            hline(&format!("Rule {}", i + 1), area.x, y + rh, area.w, Clr::tx1().alpha(if last { 70 } else { 28 }), if last { 1.5 } else { 0.75 }),
            Role::Item(i),
        );
    }
}

// ---------------------------------------------------------------------------
// Open columns

/// What heads each column.
#[derive(Clone, Copy, PartialEq)]
enum Head {
    /// A short accent bar in the item's colour.
    Bar,
    /// A numeral in the lead line.
    Numeral(f64),
    /// An outlined ring holding the number.
    Ring,
    /// Poster-size numeral.
    Giant(f64),
    /// `[01]` (technical drawings).
    Bracket,
}

fn open_columns(b: &mut B, blocks: &[Block]) {
    let cx = b.cx;
    let t = cx.t;
    let area = cx.frame.body;
    let page = page_hex(t).to_string();
    let n = blocks.len();
    let centred = is_centred(b);
    let al = if centred { Align::Center } else { Align::Left };
    // The column head and the separators follow the header composition.
    let (head, rules): (Head, Option<Option<&'static str>>) = match t.look.header {
        Header::TopLeft => (Head::Bar, Some(None)),
        Header::Rail => (Head::Numeral(32.0), Some(None)),
        Header::Centered => (Head::Ring, Some(None)),
        Header::Poster => (Head::Giant(54.0), None),
        Header::Banner => (Head::Bracket, Some(Some("dash"))),
        Header::Sidebar { .. } => (Head::Numeral(26.0), Some(None)),
    };
    let (pad_l, pad_r) = if centred { (150_000, 150_000) } else { (260_000, 180_000) };
    let mut per = if n <= 4 { n } else { 3 };
    if n > 2 && area.w / per as i64 - pad_l - pad_r < 1_550_000 {
        per = n.div_ceil(2);
    }
    let rows = n.div_ceil(per);
    let cw = area.w / per as i64;
    let min_inner = cw - pad_l - pad_r;
    let row_gap = 140_000;
    let row_avail = (area.h - 120_000 - row_gap * (rows as i64 - 1)) / rows as i64;

    // Shape leads sit above the text box; inline leads are its first line.
    let shape_lead = match head {
        Head::Bar => 56_000 + 200_000,
        Head::Ring => BADGE + 180_000,
        _ => 0,
    };
    let inline = match head {
        Head::Numeral(pt) => Some((pt, 4.0)),
        Head::Giant(pt) => Some((pt * 0.9, 2.0)),
        Head::Bracket => Some((12.0, 6.0)),
        _ => None,
    };
    let inline_h = inline.map_or(0, |(pt, a)| lead_h(pt, a));
    let (hpt, bpt, need) = fit_texts(t, blocks, min_inner, row_avail - shape_lead, (22.0, 15.0), (16.0, 12.0), inline_h);
    let rh = (need + shape_lead + 200_000).min(row_avail);
    let block_h = rh * rows as i64 + row_gap * (rows as i64 - 1);
    let top = vcentre(area, block_h);
    let body_clr = soft_ink(t, 78, &page);
    grid_backing(b, R::new(area.x - 220_000, top - 160_000, area.w + 440_000, block_h + 320_000));

    for (i, blk) in blocks.iter().enumerate() {
        let (r, c) = (i / per, i % per);
        let in_row = if r + 1 == rows { n - r * per } else { per };
        let shift = if centred { (per - in_row) as i64 * cw / 2 } else { 0 };
        let x = area.x + shift + c as i64 * cw;
        let y = top + r as i64 * (rh + row_gap);
        let accent = 1 + (i as u8 % 4);
        if c > 0 {
            if let Some(dash) = rules {
                b.sp(vline(&format!("Rule {}", i + 1), x, y, rh, Clr::tx1().alpha(30), 0.75, dash), Role::Item(i));
            }
        }
        if r > 0 && c == 0 && rules.is_some() {
            b.sp(hline(&format!("Rule row {}", r + 1), area.x, y - row_gap / 2, area.w, Clr::tx1().alpha(22), 0.75), Role::Item(i));
        }
        let tx = if c == 0 && !centred { x } else { x + pad_l };
        let tw = cw - pad_r - (tx - x);
        let num = format!("{:02}", i + 1);
        match head {
            Head::Bar => {
                b.sp(
                    Shape::deco(format!("Bar {}", i + 1), Geom::Rect, R::new(tx, y + 20_000, 520_000, 56_000), Fill::Solid(Clr::accent(accent))),
                    Role::Item(i),
                );
            }
            Head::Ring => {
                let rx = if centred { x + (cw - BADGE) / 2 } else { tx };
                let mut ring = Shape::new(format!("Number {}", i + 1), Geom::Ellipse, R::new(rx, y + 20_000, BADGE, BADGE));
                ring.decorative = true;
                ring.line = Some(Line::new(Clr::accent(accent), 1.5));
                ring.text = Some(
                    Text::one(
                        Para::from_runs(vec![Run::new(format!("{}", i + 1))
                            .size(15.0)
                            .bold()
                            .font(FontRef::Major)
                            .color(accent_on(t, accent, 4.5, &page))])
                        .align(Align::Center)
                        .no_bullet(),
                    )
                    .anchor(Anchor::Middle),
                );
                b.sp(ring, Role::Item(i));
            }
            _ => {}
        }
        let lead = match head {
            Head::Numeral(pt) => Some(lead_para(t, &num, pt, accent_on(t, 1, 3.0, &page), 4.0, true, None)),
            Head::Giant(pt) => Some(lead_para(t, &num, pt * 0.9, accent_on(t, 1, 3.0, &page), 2.0, true, None)),
            Head::Bracket => Some(lead_para(t, &format!("[{num}]"), 12.0, accent_on(t, 1, 4.5, &page), 6.0, false, Some(1.2))),
            _ => None,
        };
        let paras = card_paras(blk, hpt, bpt, al, Clr::tx1(), body_clr, lead);
        b.sp(
            Shape::text_box(
                format!("Card text {}", i + 1),
                R::new(tx, y + shape_lead, tw, rh - shape_lead),
                Text::new(paras),
            ),
            Role::Item(i),
        );
    }
}

// ---------------------------------------------------------------------------
// Bento

/// Tile rectangles: the first is the large one.
fn bento_rects(n: usize, area: R, narrow: bool) -> Vec<R> {
    if n == 1 {
        return vec![area];
    }
    let m = n - 1;
    let (big, region, pattern): (R, R, Vec<usize>) = if narrow {
        let bh = area.h * if m <= 2 { 56 } else { 40 } / 100 - GUTTER / 2;
        let pat = match m {
            1 => vec![1],
            2 => vec![2],
            3 => vec![1, 2],
            4 => vec![2, 2],
            _ => vec![2, 2, 1],
        };
        (
            R::new(area.x, area.y, area.w, bh),
            R::new(area.x, area.y + bh + GUTTER, area.w, area.h - bh - GUTTER),
            pat,
        )
    } else {
        let bw = if n == 2 {
            area.w * 54 / 100
        } else if n == 6 {
            area.w * 38 / 100
        } else {
            area.w * 44 / 100
        } - GUTTER / 2;
        let pat = match m {
            1 => vec![1],
            2 => vec![1, 1],
            3 => vec![1, 2],
            4 => vec![2, 2],
            _ => vec![2, 3],
        };
        (
            R::new(area.x, area.y, bw, area.h),
            R::new(area.x + bw + GUTTER, area.y, area.w - bw - GUTTER, area.h),
            pat,
        )
    };
    let mut out = vec![big];
    let rows = pattern.len() as i64;
    let rh = (region.h - GUTTER * (rows - 1)) / rows;
    for (ri, cnt) in pattern.iter().enumerate() {
        let cw = (region.w - GUTTER * (*cnt as i64 - 1)) / *cnt as i64;
        for c in 0..*cnt {
            out.push(R::new(
                region.x + c as i64 * (cw + GUTTER),
                region.y + ri as i64 * (rh + GUTTER),
                cw,
                rh,
            ));
        }
    }
    out
}

/// Heading/body sizes for a bento tile with `lead` EMU of numeral above.
fn tile_fit(t: &ThemeSpec, blk: &Block, inner: R, hs: (f64, f64), bs: (f64, f64), lead: i64) -> (f64, f64, bool) {
    let (hp, bp, need) = fit_texts(t, std::slice::from_ref(blk), inner.w, inner.h, hs, bs, lead);
    (hp, bp, need <= inner.h)
}

fn bento(b: &mut B, blocks: &[Block]) {
    let cx = b.cx;
    let t = cx.t;
    let n = blocks.len();
    let full = cx.frame.body;
    let narrow = full.w < 7_400_000;
    let h = if narrow { full.h - 100_000 } else { (full.h - 100_000).min(4_300_000) };
    let area = R::new(full.x, vcentre(full, h), full.w, h);
    let rects = bento_rects(n, area, narrow);
    let surf = card_surface(t);
    let (padx, pady) = (280_000, 240_000);
    let inner_of = |r: &R| R::new(r.x + padx, r.y + pady, r.w - 2 * padx, r.h - 2 * pady);
    let filled = field_reads(t, 1);

    // Two tiles are both "large" (only the first is filled); otherwise the
    // first is large and the rest small. Small tiles share one pair of
    // sizes (the smallest any needs); when even the smallest sizes do not
    // fit, a bento is the wrong shape for this much text in this little
    // room: fall back to a plain grid.
    let large = |i: usize| i == 0 || n == 2;
    let (mut sh, mut sb) = (22.0f64, 15.0f64);
    let lead_small = lead_h(12.0, 4.0);
    let mut fits = true;
    for (i, (blk, r)) in blocks.iter().zip(rects.iter()).enumerate() {
        if large(i) {
            continue;
        }
        let (hp, bp, ok) = tile_fit(t, blk, inner_of(r), (22.0, 14.0), (15.0, 11.0), lead_small);
        fits &= ok && inner_of(r).w >= 1_150_000;
        sh = sh.min(hp);
        sb = sb.min(bp);
    }
    // The large tiles: the numeral gives way before the text does.
    let mut big_fit = (30.0f64, 19.0f64, 44.0f64);
    for i in (0..n).filter(|i| large(*i)) {
        let inner = inner_of(&rects[i]);
        let mut found = false;
        for npt in [44.0, 36.0, 30.0, 24.0, 18.0] {
            let (hp, bp, ok) = tile_fit(t, &blocks[i], inner, (30.0, 18.0), (19.0, 13.0), lead_h(npt, 6.0));
            if ok {
                big_fit = (big_fit.0.min(hp), big_fit.1.min(bp), big_fit.2.min(npt));
                found = true;
                break;
            }
        }
        fits &= found;
    }
    if !fits {
        return grid(b, blocks, false);
    }

    for (i, (blk, r)) in blocks.iter().zip(rects.iter()).enumerate() {
        let num = format!("{:02}", i + 1);
        let big = large(i);
        let on_field = i == 0 && filled;
        let (hclr, bclr, nclr) = if on_field {
            (on_accent(t, 1), on_accent(t, 1), on_accent(t, 1))
        } else {
            (Clr::tx1(), soft_ink(t, 78, &surf), accent_on(t, 1 + (i as u8 % 4), 4.5, &surf))
        };
        if on_field {
            b.sp(
                Shape::deco(format!("Card {}", i + 1), Geom::Round(t.radius.max(14)), *r, Fill::Solid(Clr::accent(1))),
                Role::Item(i),
            );
        } else {
            chrome_card(b, &format!("Card {}", i + 1), *r, Role::Item(i));
        }
        let inner = inner_of(r);
        let (lead, hpt, bpt, anchor) = if big {
            let (hp, bp, npt) = big_fit;
            let nc = if on_field { nclr } else { accent_on(t, 1, 3.0, &surf) };
            (lead_para(t, &num, npt, nc, 6.0, true, None), hp, bp, if n == 1 { Anchor::Middle } else { Anchor::Bottom })
        } else {
            (lead_para(t, &num, 12.0, nclr, 4.0, false, Some(1.2)), sh, sb, Anchor::Top)
        };
        let paras = card_paras(blk, hpt, bpt, Align::Left, hclr, bclr, Some(lead));
        b.sp(
            Shape::text_box(format!("Card text {}", i + 1), inner, Text::new(paras).anchor(anchor)),
            Role::Item(i),
        );
    }
}

// ---------------------------------------------------------------------------
// Team

struct Person {
    name: String,
    role: String,
    bio: String,
    initials: String,
}

fn people(blocks: &[Block]) -> Vec<Person> {
    blocks
        .iter()
        .map(|blk| {
            let name = person_name(blk);
            let (role, bio) = if blk.heading.is_empty() {
                (String::new(), String::new())
            } else {
                (
                    blk.text.first().cloned().unwrap_or_default(),
                    if blk.text.len() > 1 { blk.text[1..].join(" ") } else { String::new() },
                )
            };
            Person { initials: initials(&name), name, role, bio }
        })
        .collect()
}

/// An initials avatar: a filled disc, or a ring on outlined themes.
fn avatar(t: &ThemeSpec, name: &str, rect: R, accent: u8, initials: &str, ring: bool, surface: &str) -> Shape {
    let pt = (rect.w as f64 / EMU_PER_PT * 0.34).clamp(14.0, 32.0);
    if !ring {
        let mut s = badge(t, name, rect, accent, initials, pt);
        if t.card == CardStyle::Sticker {
            s.line = Some(Line::new(Clr::tx1(), 1.75));
        }
        return s;
    }
    let mut s = Shape::new(name, Geom::Ellipse, rect);
    s.decorative = true;
    s.line = Some(Line::new(Clr::accent(accent), 1.75));
    s.text = Some(
        Text::one(
            Para::from_runs(vec![Run::new(initials)
                .size(pt)
                .bold()
                .font(FontRef::Major)
                .color(accent_on(t, accent, 4.5, surface))])
            .align(Align::Center)
            .no_bullet(),
        )
        .anchor(Anchor::Middle),
    );
    s
}

/// Common name/role/bio sizes for people in `w` and the height they need.
struct PersonFit {
    npt: f64,
    rpt: f64,
    bpt: f64,
    need: i64,
}

fn fit_people(t: &ThemeSpec, ps: &[Person], w: i64, caps: (f64, f64, f64), bio_h: i64) -> PersonFit {
    let (mut npt, mut rpt, mut bpt) = caps;
    for p in ps {
        npt = npt.min(fit_pt(std::slice::from_ref(&p.name), w, 640_000, caps.0, 13.0, factor_major(t) * 1.08, 1.0, 0.0));
        if !p.role.is_empty() {
            rpt = rpt.min(fit_pt(std::slice::from_ref(&p.role), w, 440_000, caps.1, 11.0, factor_minor(t), 1.0, 0.0));
        }
        if !p.bio.is_empty() {
            bpt = bpt.min(fit_pt(std::slice::from_ref(&p.bio), w, bio_h, caps.2, 10.0, factor_minor(t), 1.1, 0.0));
        }
    }
    let mut need = 0;
    for p in ps {
        let mut h = (line_count(&p.name, npt, w, factor_major(t) * 1.08) as f64 * npt * 1.2 * EMU_PER_PT) as i64;
        if !p.role.is_empty() {
            h += (line_count(&p.role, rpt, w, factor_minor(t)) as f64 * rpt * 1.2 * EMU_PER_PT) as i64 + (3.0 * EMU_PER_PT) as i64;
        }
        if !p.bio.is_empty() {
            h += stack_height(std::slice::from_ref(&p.bio), bpt, w, factor_minor(t), 1.1, 0.0) + (6.0 * EMU_PER_PT) as i64;
        }
        need = need.max(h);
    }
    PersonFit { npt, rpt, bpt, need }
}

fn person_paras(p: &Person, f: &PersonFit, al: Align, role_clr: Clr, bio_clr: Clr) -> Vec<Para> {
    let mut v = vec![Para::from_runs(vec![Run::new(p.name.clone()).size(f.npt).bold().font(FontRef::Major)])
        .align(al)
        .no_bullet()];
    if !p.role.is_empty() {
        v.push(
            Para::from_runs(vec![Run::new(p.role.clone()).size(f.rpt).bold().color(role_clr)])
                .align(al)
                .no_bullet()
                .before(3.0),
        );
    }
    if !p.bio.is_empty() {
        v.push(
            Para::from_runs(vec![Run::new(p.bio.clone()).size(f.bpt).color(bio_clr)])
                .align(al)
                .no_bullet()
                .before(6.0),
        );
    }
    v
}

pub(crate) fn team(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let blocks: Vec<Block> = blocks_of(s).into_iter().take(5).collect();
    let ps = people(&blocks);
    if !ps.is_empty() {
        match cx.t.look.team {
            TeamStyle::Cards => team_cards(&mut b, &ps),
            TeamStyle::Inline => {
                let rows_layout = matches!(
                    cx.t.look.header,
                    Header::Banner | Header::Rail | Header::Sidebar { .. }
                ) || cx.frame.body.w / ps.len() as i64 - 300_000 < 1_700_000;
                if rows_layout {
                    team_roster(&mut b, &ps, false)
                } else {
                    team_open_columns(&mut b, &ps)
                }
            }
        }
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}

fn team_cards(b: &mut B, ps: &[Person]) {
    let cx = b.cx;
    let t = cx.t;
    let area = cx.frame.body;
    let n = ps.len();
    let centred = is_centred(b);
    let surf = card_surface(t);
    let ring = false;
    let col_w = (area.w - GUTTER * (n as i64 - 1)) / n as i64;
    let horiz = n > 1 && col_w < 1_850_000;
    // Many people in little room: one boxed row each.
    if horiz && n >= 4 {
        return team_roster(b, ps, true);
    }
    let (per, cell_w) = if horiz {
        (2usize, (area.w - GUTTER) / 2)
    } else {
        (n, if n <= 2 { col_w.min(3_900_000) } else { col_w })
    };
    let rows = n.div_ceil(per);
    let d: i64 = if horiz { 720_000 } else { 860_000.min(cell_w - 400_000) };
    let inner_w = if horiz { cell_w - 2 * 240_000 - d - 220_000 } else { cell_w - 400_000 };
    let fit = fit_people(t, ps, inner_w, (20.0, 15.0, 13.0), if horiz { 700_000 } else { 1_250_000 });
    let avail_h = (area.h - 100_000 - GUTTER * (rows as i64 - 1)) / rows as i64;
    let card_h = if horiz {
        (fit.need.max(d) + 2 * 240_000).max(1_300_000).min(avail_h)
    } else {
        (300_000 + d + 200_000 + fit.need + 280_000).max(2_600_000).min(avail_h)
    };
    let block_h = card_h * rows as i64 + GUTTER * (rows as i64 - 1);
    let top = vcentre(R::new(area.x, area.y, area.w, area.h - 100_000), block_h);
    let group_w = per as i64 * cell_w + GUTTER * (per as i64 - 1);
    let x0 = if centred { area.x + (area.w - group_w) / 2 } else { area.x };
    let al = if centred && !horiz { Align::Center } else { Align::Left };

    for (i, p) in ps.iter().enumerate() {
        let (r, c) = (i / per, i % per);
        let in_row = if r + 1 == rows { n - r * per } else { per };
        let shift = if centred { (per - in_row) as i64 * (cell_w + GUTTER) / 2 } else { 0 };
        let rect = R::new(x0 + shift + c as i64 * (cell_w + GUTTER), top + r as i64 * (card_h + GUTTER), cell_w, card_h);
        let accent = 1 + (i as u8 % 6);
        chrome_card(b, &format!("Person card {}", i + 1), rect, Role::Item(i));
        let (av, text) = if horiz {
            (
                R::new(rect.x + 240_000, rect.y + (card_h - d) / 2, d, d),
                R::new(rect.x + 240_000 + d + 220_000, rect.y + 240_000, inner_w, card_h - 480_000),
            )
        } else {
            let ax = if al == Align::Center { rect.x + (rect.w - d) / 2 } else { rect.x + 200_000 };
            (
                R::new(ax, rect.y + 300_000, d, d),
                R::new(rect.x + 200_000, rect.y + 300_000 + d + 200_000, inner_w, card_h - d - 580_000),
            )
        };
        b.sp(avatar(t, &format!("Avatar {}", i + 1), av, accent, &p.initials, ring, &surf), Role::Item(i));
        let paras = person_paras(p, &fit, al, accent_on(t, accent, 4.5, &surf), soft_ink(t, 74, &surf));
        let anchor = if horiz { Anchor::Middle } else { Anchor::Top };
        b.sp(
            Shape::text_box(format!("Person {}", i + 1), text, Text::new(paras).anchor(anchor)),
            Role::Item(i),
        );
    }
}

/// Open columns: large initials above name, role and bio, thin rules between.
fn team_open_columns(b: &mut B, ps: &[Person]) {
    let cx = b.cx;
    let t = cx.t;
    let area = cx.frame.body;
    let page = page_hex(t).to_string();
    let n = ps.len();
    let centred = is_centred(b);
    let al = if centred { Align::Center } else { Align::Left };
    let ring = matches!(t.card, CardStyle::Outlined | CardStyle::Rule);
    let cw = area.w / n as i64;
    let (pad_l, pad_r) = if centred { (160_000, 160_000) } else { (260_000, 160_000) };
    let min_inner = cw - pad_l - pad_r;
    let d: i64 = 1_000_000.min(min_inner);
    let fit = fit_people(t, ps, min_inner, (22.0, 15.0, 14.0), 1_300_000);
    let h = d + 220_000 + fit.need + 120_000;
    let h = h.min(area.h - 100_000);
    let top = vcentre(area, h);
    grid_backing(b, R::new(area.x - 220_000, top - 160_000, area.w + 440_000, h + 320_000));
    for (i, p) in ps.iter().enumerate() {
        let x = area.x + i as i64 * cw;
        let accent = 1 + (i as u8 % 6);
        if i > 0 {
            b.sp(vline(&format!("Rule {}", i + 1), x, top, h, Clr::tx1().alpha(30), 0.75, None), Role::Item(i));
        }
        let tx = if i == 0 && !centred { x } else { x + pad_l };
        let tw = cw - pad_r - (tx - x);
        let ax = if centred { x + (cw - d) / 2 } else { tx };
        b.sp(
            avatar(t, &format!("Avatar {}", i + 1), R::new(ax, top, d, d), accent, &p.initials, ring, &page),
            Role::Item(i),
        );
        let paras = person_paras(p, &fit, al, accent_on(t, accent, 4.5, &page), soft_ink(t, 74, &page));
        b.sp(
            Shape::text_box(format!("Person {}", i + 1), R::new(tx, top + d + 220_000, tw, h - d - 220_000), Text::new(paras)),
            Role::Item(i),
        );
    }
}

/// A roster: one row per person, avatar then name/role then bio. Open rows
/// are separated by hairlines; boxed rows (the narrow form of team cards)
/// each get their own card.
fn team_roster(b: &mut B, ps: &[Person], boxed: bool) {
    let cx = b.cx;
    let t = cx.t;
    let area = cx.frame.body;
    let n = ps.len();
    let ring = !boxed && matches!(t.card, CardStyle::Outlined | CardStyle::Rule);
    let surf = if boxed { card_surface(t) } else { page_hex(t).to_string() };
    let gap_y: i64 = if boxed { 137_000 } else { 0 };
    let ix: i64 = if boxed { 240_000 } else { 0 };
    let wide = area.w - 2 * ix >= 7_000_000;
    let rh = ((area.h - 120_000 - gap_y * (n as i64 - 1)) / n as i64).min(1_450_000);
    let top = vcentre(area, rh * n as i64 + gap_y * (n as i64 - 1));
    let d = (rh - if boxed { 240_000 } else { 200_000 }).min(980_000);
    let gap = 280_000;
    let cw = area.w - 2 * ix;
    // Wide: name and role in one column, the bio in another; narrow: all stacked.
    let name_w = if wide { (cw - d - gap) * 38 / 100 } else { cw - d - gap };
    let bio_x_off = d + gap + name_w + 240_000;
    let bio_w = cw - bio_x_off;
    let (fit, bio_pt) = if wide {
        let heads: Vec<Person> = ps
            .iter()
            .map(|p| Person { name: p.name.clone(), role: p.role.clone(), bio: String::new(), initials: String::new() })
            .collect();
        let f = fit_people(t, &heads, name_w, (22.0, 15.0, 14.0), 0);
        let mut bp = 16.0f64;
        for p in ps {
            if !p.bio.is_empty() {
                bp = bp.min(fit_pt(std::slice::from_ref(&p.bio), bio_w, rh - 160_000, 16.0, 11.0, factor_minor(t), 1.1, 0.0));
            }
        }
        (f, bp)
    } else {
        (fit_people(t, ps, name_w, (20.0, 14.0, 13.0), (rh - 160_000) / 2), 0.0)
    };

    if !boxed {
        grid_backing(b, R::new(area.x - 220_000, top - 160_000, area.w + 440_000, rh * n as i64 + 320_000));
    }
    for (i, p) in ps.iter().enumerate() {
        let y = top + i as i64 * (rh + gap_y);
        let accent = 1 + (i as u8 % 6);
        if boxed {
            chrome_card(b, &format!("Person card {}", i + 1), R::new(area.x, y, area.w, rh), Role::Item(i));
        } else if i == 0 {
            b.sp(hline("Rule top", area.x, y, area.w, Clr::tx1().alpha(70), 1.5), Role::Item(0));
        }
        b.sp(
            avatar(t, &format!("Avatar {}", i + 1), R::new(area.x + ix, y + (rh - d) / 2, d, d), accent, &p.initials, ring, &surf),
            Role::Item(i),
        );
        let who = Person {
            name: p.name.clone(),
            role: p.role.clone(),
            bio: if wide { String::new() } else { p.bio.clone() },
            initials: String::new(),
        };
        let paras = person_paras(&who, &fit, Align::Left, accent_on(t, accent, 4.5, &surf), soft_ink(t, 74, &surf));
        b.sp(
            Shape::text_box(
                format!("Person {}", i + 1),
                R::new(area.x + ix + d + gap, y + 60_000, name_w, rh - 120_000),
                Text::new(paras).anchor(Anchor::Middle),
            ),
            Role::Item(i),
        );
        if wide && !p.bio.is_empty() {
            b.sp(
                Shape::text_box(
                    format!("Person bio {}", i + 1),
                    R::new(area.x + ix + bio_x_off, y + 60_000, bio_w, rh - 120_000),
                    Text::one(Para::from_runs(vec![Run::new(p.bio.clone()).size(bio_pt).color(soft_ink(t, 76, &surf))]).no_bullet())
                        .anchor(Anchor::Middle),
                ),
                Role::Item(i),
            );
        }
        if !boxed {
            let last = i + 1 == n;
            b.sp(
                hline(&format!("Rule {}", i + 1), area.x, y + rh, area.w, Clr::tx1().alpha(if last { 70 } else { 28 }), if last { 1.5 } else { 0.75 }),
                Role::Item(i),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::super::stats::tests::*;
    use super::*;
    use crate::deck::palette::PALETTES;
    use crate::deck::theme::{ThemeSpec, THEMES};

    const ALL_CARDS: [CardsStyle; 5] =
        [CardsStyle::Grid, CardsStyle::Rows, CardsStyle::Columns, CardsStyle::Stickers, CardsStyle::Bento];

    fn card_slide(n: usize) -> DesignedSlide {
        let heads = ["Local first", "Verified formulas", "One quiet app", "Open formats", "Private AI", "Fast on any Mac"];
        let mut s = DesignedSlide::new("Three ideas, one quiet app").kicker("Solution");
        s.layout = Some(LayoutKind::Cards);
        s.blocks = (0..n)
            .map(|i| Block::card(heads[i], "Documents never leave the device unless you say so, and that stays true."))
            .collect();
        s
    }

    fn team_slide(n: usize) -> DesignedSlide {
        let mut s = DesignedSlide::new("The people building it").kicker("Team");
        s.layout = Some(LayoutKind::Team);
        s.blocks = (0..n)
            .map(|i| Block {
                heading: ["Layla Hassan", "Omar Saleh", "Mina Costa", "Noor Aziz", "Sam Rivera"][i].into(),
                text: vec!["Chief executive".into(), "Ex-Office product lead who shipped three suites".into()],
                ..Default::default()
            })
            .collect();
        s
    }

    fn sps(b: &Built) -> Vec<(&Shape, Role)> {
        b.items
            .iter()
            .filter_map(|i| match &i.el {
                El::Sp(s) => Some((s, i.role)),
                _ => None,
            })
            .collect()
    }

    fn check_items(built: &Built, n: usize, who: &str) {
        let mut last = 0usize;
        let mut seen = vec![false; n];
        for (sh, role) in sps(built) {
            if let Some(r) = sh.rect {
                if !sh.decorative {
                    assert!(
                        r.x >= 0 && r.y >= 0 && r.right() <= SLIDE_W && r.bottom() <= SLIDE_H,
                        "{who} {}: off the page {r:?}",
                        sh.name
                    );
                }
            }
            if let Some(i) = item_of(role) {
                assert!(i >= last, "{who}: item {i} after {last} (reading order)");
                last = i;
                assert!(i < n, "{who}: item index {i} out of range");
                seen[i] = true;
            }
        }
        assert!(seen.iter().all(|x| *x), "{who}: some item has no elements");
    }

    #[test]
    fn every_cards_variant_builds_for_one_to_six_cards_in_order() {
        let base = crate::deck::theme::theme("obsidian").unwrap();
        for header in HEADERS {
            for style in ALL_CARDS {
                for card in [CardStyle::Filled, CardStyle::Rule, CardStyle::Bento, CardStyle::Sticker, CardStyle::Glass] {
                    for n in 1..=6usize {
                        let t = ThemeSpec { look: Look { header, cards: style, ..base.look }, card, ..*base };
                        let built = cards(&card_slide(n), &ctx_for(&t));
                        let who = format!("{header:?} {style:?} {card:?} n={n}");
                        check_items(&built, n, &who);
                        for (sh, role) in sps(&built) {
                            let txt = text_of(sh);
                            for i in 0..n {
                                let head = ["Local first", "Verified formulas", "One quiet app", "Open formats", "Private AI", "Fast on any Mac"][i];
                                if txt.contains(head) {
                                    assert_eq!(role, Role::Item(i), "{who}: '{head}' tagged {role:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_team_variant_builds_for_one_to_five_people_in_order() {
        let base = crate::deck::theme::theme("ivory").unwrap();
        for header in HEADERS {
            for style in [TeamStyle::Cards, TeamStyle::Inline] {
                for card in [CardStyle::Filled, CardStyle::Rule, CardStyle::Sticker] {
                    for n in 1..=5usize {
                        let t = ThemeSpec { look: Look { header, team: style, ..base.look }, card, ..*base };
                        let built = team(&team_slide(n), &ctx_for(&t));
                        let who = format!("{header:?} {style:?} {card:?} n={n}");
                        check_items(&built, n, &who);
                        for (sh, role) in sps(&built) {
                            let txt = text_of(sh);
                            for (i, name) in ["Layla Hassan", "Omar Saleh", "Mina Costa", "Noor Aziz", "Sam Rivera"].iter().take(n).enumerate() {
                                if txt.contains(name) {
                                    assert_eq!(role, Role::Item(i), "{who}: '{name}' tagged {role:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dense_cards_and_people_fit_their_boxes_in_every_style_and_header() {
        let mut bad = Vec::new();
        for base in THEMES {
            for header in HEADERS {
                for style in ALL_CARDS {
                    for n in [4usize, 6] {
                        let t = ThemeSpec { look: Look { header, cards: style, ..base.look }, ..*base };
                        for o in overflows(&t, &cards(&card_slide(n), &ctx_for(&t))) {
                            bad.push(format!("{} {header:?} {style:?} n={n} {o}", base.id));
                        }
                    }
                }
                for style in [TeamStyle::Cards, TeamStyle::Inline] {
                    for n in [4usize, 5] {
                        let t = ThemeSpec { look: Look { header, team: style, ..base.look }, ..*base };
                        for o in overflows(&t, &team(&team_slide(n), &ctx_for(&t))) {
                            bad.push(format!("{} {header:?} team {style:?} n={n} {o}", base.id));
                        }
                    }
                }
            }
        }
        bad.dedup();
        assert!(bad.is_empty(), "{} overflow(s):\n{}", bad.len(), bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }

    #[test]
    fn stickers_tilt_a_little_and_stay_inside_their_cells() {
        let base = crate::deck::theme::theme("sunrise").unwrap();
        for n in 1..=6usize {
            let built = cards(&card_slide(n), &ctx_for(base));
            let area = frame(&base.look).body;
            let mut tilts = 0;
            for (sh, _) in sps(&built) {
                assert!(sh.rot.abs() <= 1.6, "{}: tilt {} too strong", sh.name, sh.rot);
                if sh.rot != 0.0 {
                    tilts += 1;
                }
                if let Some(r) = sh.rect {
                    if sh.name.starts_with("Card") || sh.name.starts_with("Number") {
                        assert!(
                            r.x >= area.x - 60_000 && r.right() <= area.right() + 60_000 && r.y >= area.y - 60_000 && r.bottom() <= area.bottom() + 60_000,
                            "n={n} {}: leaves the body area {r:?} vs {area:?}",
                            sh.name
                        );
                    }
                }
            }
            assert!(tilts >= n, "n={n}: stickers are not tilted");
        }
    }

    #[test]
    fn no_card_or_person_text_is_squeezed_to_a_few_characters_per_line() {
        let base = crate::deck::theme::theme("obsidian").unwrap();
        for header in HEADERS {
            for style in ALL_CARDS {
                for n in 1..=6usize {
                    let t = ThemeSpec { look: Look { header, cards: style, ..base.look }, ..*base };
                    assert_min_chars(&cards(&card_slide(n), &ctx_for(&t)), 8, &format!("{header:?} {style:?} n={n}"));
                }
            }
            for style in [TeamStyle::Cards, TeamStyle::Inline] {
                for n in 1..=5usize {
                    let t = ThemeSpec { look: Look { header, team: style, ..base.look }, ..*base };
                    assert_min_chars(&team(&team_slide(n), &ctx_for(&t)), 8, &format!("{header:?} team {style:?} n={n}"));
                }
            }
        }
    }

    #[test]
    fn card_and_team_text_reads_in_every_theme_and_palette() {
        for base in THEMES {
            for pal in PALETTES {
                for style in ALL_CARDS {
                    for n in [3usize, 6] {
                        let t = ThemeSpec { look: Look { cards: style, ..base.look }, ..base.with_palette(pal) };
                        assert_text_contrast(&t, &cards(&card_slide(n), &ctx_for(&t)), &format!("{} in {} cards {style:?} n={n}", base.id, pal.id));
                    }
                }
                for style in [TeamStyle::Cards, TeamStyle::Inline] {
                    let t = ThemeSpec { look: Look { team: style, ..base.look }, ..base.with_palette(pal) };
                    assert_text_contrast(&t, &team(&team_slide(4), &ctx_for(&t)), &format!("{} in {} team {style:?}", base.id, pal.id));
                }
            }
        }
    }

    #[test]
    fn the_card_designs_are_different_compositions() {
        let base = crate::deck::theme::theme("obsidian").unwrap();
        let mut layouts: Vec<Vec<R>> = Vec::new();
        for style in ALL_CARDS {
            let t = ThemeSpec { look: Look { cards: style, ..base.look }, ..*base };
            let built = cards(&card_slide(4), &ctx_for(&t));
            layouts.push(sps(&built).iter().filter(|(s, _)| s.text.is_some() && s.name.starts_with("Card")).filter_map(|(s, _)| s.rect).collect());
        }
        for i in 0..layouts.len() {
            for j in i + 1..layouts.len() {
                assert_ne!(layouts[i], layouts[j], "card styles {i} and {j} draw the same geometry");
            }
        }
    }

    #[test]
    fn bento_has_one_large_tile_and_smaller_ones() {
        let area = R::new(0, 0, 10_000_000, 4_000_000);
        for n in 2..=6usize {
            for narrow in [false, true] {
                let a = if narrow { R::new(0, 0, 6_800_000, 4_800_000) } else { area };
                let rs = bento_rects(n, a, narrow);
                assert_eq!(rs.len(), n);
                let big = rs[0].w * rs[0].h;
                for r in &rs[1..] {
                    assert!(r.w * r.h < big, "n={n} narrow={narrow}: a small tile is as big as the large one");
                    assert!(r.x >= a.x && r.right() <= a.right() && r.y >= a.y && r.bottom() <= a.bottom());
                }
                // No tile overlaps another.
                for i in 0..rs.len() {
                    for j in i + 1..rs.len() {
                        let (p, q) = (rs[i], rs[j]);
                        assert!(p.right() <= q.x || q.right() <= p.x || p.bottom() <= q.y || q.bottom() <= p.y, "tiles {i}/{j} overlap");
                    }
                }
            }
        }
    }
}
