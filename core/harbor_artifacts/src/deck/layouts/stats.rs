//! Figures: stat tiles, ruled rows, one hero figure, open columns.
//!
//! Four genuinely different compositions share one content model:
//!
//! * **Tiles** — a row of boxed tiles (chrome comes from the theme).
//! * **Rows** — an editorial table: ruled rows, a huge figure on the left,
//!   its label right.
//! * **Hero** — the first figure giant (in an accent field, a bento tile or
//!   as poster type) with the others stacked as a list beside it.
//! * **Columns** — unboxed columns of figure over label, separated by thin
//!   vertical rules.
//!
//! Every variant adapts to the area it is given (a sidebar leaves only
//! ~6.8 M EMU) and picks colours that read on the surface they sit on in
//! every palette.

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

/// One figure with its label and optional detail line.
struct Stat {
    value: String,
    label: String,
    detail: String,
}

fn stat_items(s: &DesignedSlide) -> Vec<Stat> {
    let blocks: Vec<Block> = if !s.blocks.is_empty() {
        s.blocks.iter().filter(|x| x.value.is_some()).take(4).cloned().collect()
    } else {
        s.bullets
            .iter()
            .filter_map(|x| split_stat(x))
            .take(4)
            .map(|(v, l)| Block::stat(v, l))
            .collect()
    };
    blocks
        .into_iter()
        .map(|b| Stat {
            value: b.value.unwrap_or_default(),
            label: b.label.unwrap_or_default(),
            detail: b.text.first().cloned().unwrap_or_default(),
        })
        .collect()
}

pub(crate) fn stats(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let items = stat_items(s);
    if !items.is_empty() {
        match cx.t.look.stats {
            StatsStyle::Tiles => tiles(&mut b, &items),
            StatsStyle::Rows => rows(&mut b, &items),
            StatsStyle::Hero => match cx.t.card {
                CardStyle::Rule => hero_poster(&mut b, &items),
                CardStyle::Bento => hero_bento(&mut b, &items),
                _ => hero_field(&mut b, &items),
            },
            StatsStyle::Columns => open_columns(&mut b, &items),
        }
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}

// ---------------------------------------------------------------------------
// Shared helpers (cards.rs uses these too)

/// The hex of a theme colour reference (alpha not applied).
pub(crate) fn hex_of(t: &ThemeSpec, c: &Clr) -> String {
    t.resolve(c).0
}

/// The colour a card's inside shows, as a hex (what text on a card sits on).
pub(crate) fn card_surface(t: &ThemeSpec) -> String {
    let page = page_hex(t).to_string();
    match t.card {
        CardStyle::Filled => hex_of(t, &Clr::bg2()),
        CardStyle::Outlined | CardStyle::Rule => page,
        CardStyle::Raised | CardStyle::Sticker => hex_of(t, &Clr::bg1()),
        CardStyle::Glass => t.resolve_over(&Clr::tx1().alpha(7), &page),
        CardStyle::Bento => {
            if t.is_dark() {
                t.resolve_over(&Clr::tx1().alpha(8), &page)
            } else {
                hex_of(t, &Clr::bg2())
            }
        }
    }
}

/// Accent `n` as text on `surface` when it reaches `min` contrast; else
/// the same hue deepened (on a light surface) or lightened (on a dark one)
/// until it does; else the ink.
pub(crate) fn accent_on(t: &ThemeSpec, n: u8, min: f64, surface: &str) -> Clr {
    let hex = t.palette.accents[(n as usize - 1) % 6];
    if contrast(hex, surface) >= min {
        return Clr::accent(n);
    }
    let dark_surface = contrast(surface, "FFFFFF") > contrast(surface, "000000");
    let steps: [(u32, u32); 4] = if dark_surface { [(80, 20), (65, 35), (50, 50), (38, 62)] } else { [(80, 0), (65, 0), (50, 0), (38, 0)] };
    for (m, o) in steps {
        let c = Clr::accent(n).lum(m, o);
        if contrast(&t.resolve(&c).0, surface) >= min {
            return c;
        }
    }
    Clr::tx1()
}

/// An opaque plate behind content that sits on a drafting-grid page, so
/// the grid does not run through the text. The plate matches the page.
pub(crate) fn grid_backing(b: &mut B, rect: R) {
    let t = b.cx.t;
    if t.deco == Deco::Grid {
        // Never reach into the header above or the footer below.
        let body = b.cx.frame.body;
        let top = rect.y.max(body.y - 60_000);
        let bottom = rect.bottom().min(body.bottom() + 40_000);
        let rect = R::new(rect.x, top, rect.w, bottom - top);
        let fill = if t.tinted_page { Clr::bg2() } else { Clr::bg1() };
        let mut s = Shape::deco("Backing", Geom::Rect, rect, Fill::Solid(fill));
        s.line = Some(Line::new(Clr::tx1().alpha(30), 0.75));
        b.sp(s, Role::Deco);
    }
}

/// A card's chrome (the theme's `b.card`; on a drafting-grid page the
/// kit makes outlined cards opaque so the grid does not run through them).
pub(crate) fn chrome_card(b: &mut B, name: &str, rect: R, role: Role) {
    b.card(name, rect, role);
}

/// Whether text on a solid accent `n` field reaches body-text contrast, so
/// the field may carry labels and details (not just a big figure).
pub(crate) fn field_reads(t: &ThemeSpec, n: u8) -> bool {
    let hex = t.palette.accents[(n as usize - 1) % 6];
    let fg = if on_accent(t, n) == Clr::lt1() { t.palette.lt1 } else { t.palette.dk1 };
    contrast(hex, fg) >= 4.5
}

/// Ink at opacity `a` percent, raised until it reads (4.5:1) on `surface`.
pub(crate) fn soft_ink(t: &ThemeSpec, a: u32, surface: &str) -> Clr {
    let mut alpha = a;
    while alpha < 100 {
        let seen = t.resolve_over(&Clr::tx1().alpha(alpha), surface);
        if contrast(&seen, surface) >= 4.5 {
            return Clr::tx1().alpha(alpha);
        }
        alpha += 6;
    }
    Clr::tx1()
}

/// A horizontal hairline.
pub(crate) fn hline(name: &str, x: i64, y: i64, w: i64, clr: Clr, pt: f64) -> Shape {
    let mut s = Shape::deco(name, Geom::Line, R::new(x, y, w, 0), Fill::None);
    s.line = Some(Line::new(clr, pt));
    s
}

/// A vertical hairline (optionally dashed).
pub(crate) fn vline(name: &str, x: i64, y: i64, h: i64, clr: Clr, pt: f64, dash: Option<&'static str>) -> Shape {
    let mut s = Shape::deco(name, Geom::Line, R::new(x, y, 0, h), Fill::None);
    let mut l = Line::new(clr, pt);
    l.dash = dash;
    s.line = Some(l);
    s
}

/// A big figure run: heavy faces bold, light serif faces regular.
fn fig_run(t: &ThemeSpec, text: &str, pt: f64, clr: Clr) -> Run {
    let r = Run::new(text).size(pt).font(FontRef::Major).color(clr);
    if t.bold_titles {
        r.bold()
    } else {
        r.regular()
    }
}

/// Glyph-width factor for big figures (digits and symbols run wide).
fn fig_factor(t: &ThemeSpec) -> f64 {
    factor_major(t) * if t.bold_titles { 1.14 } else { 1.06 }
}

/// The largest size (0.5 pt steps) at which every text fits on ONE line
/// of `w` EMU, between `min` and `max`.
fn one_line_pt(texts: &[&str], w: i64, max: f64, min: f64, factor: f64) -> f64 {
    let w_pt = w.max(1) as f64 / EMU_PER_PT;
    let mut pt = max;
    for s in texts {
        let n = s.chars().count().max(1) as f64;
        pt = pt.min(w_pt / (n * factor));
    }
    let pt = (pt * 2.0).floor() / 2.0;
    pt.max(min.min(max)).min(max)
}

/// Height (EMU) of `n` lines of `pt` type.
fn lines_h(n: usize, pt: f64, leading: f64) -> i64 {
    (n as f64 * pt * 1.2 * leading * EMU_PER_PT).round() as i64
}

fn area_of(b: &B) -> R {
    b.cx.frame.body
}

fn is_centred(b: &B) -> bool {
    b.cx.frame.title_align == Align::Center
}

fn align_of(b: &B) -> Align {
    if is_centred(b) {
        Align::Center
    } else {
        Align::Left
    }
}

/// Label then optional detail, as paragraphs.
fn label_paras(label: &str, detail: &str, lpt: f64, dpt: f64, lclr: Clr, dclr: Clr, al: Align) -> Vec<Para> {
    let mut v = vec![Para::from_runs(vec![Run::new(label).size(lpt).bold().color(lclr)])
        .no_bullet()
        .align(al)];
    if !detail.is_empty() {
        v.push(
            Para::from_runs(vec![Run::new(detail).size(dpt).color(dclr)])
                .no_bullet()
                .align(al)
                .before(5.0),
        );
    }
    v
}

/// Height the label (+ detail) paragraphs need at the given sizes.
fn label_h(t: &ThemeSpec, it: &Stat, w: i64, lpt: f64, dpt: f64) -> i64 {
    let mut h = stack_height(std::slice::from_ref(&it.label), lpt, w, factor_minor(t), 1.1, 0.0);
    if !it.detail.is_empty() {
        h += stack_height(std::slice::from_ref(&it.detail), dpt, w, factor_minor(t), 1.1, 0.0) + (5.0 * EMU_PER_PT) as i64;
    }
    h
}

/// Common label/detail sizes across all items for a box of `w` x `h`.
fn label_sizes(t: &ThemeSpec, items: &[Stat], w: i64, h: i64, lmax: f64, dmax: f64) -> (f64, f64) {
    let mut lpt = lmax;
    let mut dpt = dmax;
    for it in items {
        lpt = lpt.min(fit_pt(std::slice::from_ref(&it.label), w, (h as f64 * 0.55) as i64, lmax, 13.0, factor_minor(t), 1.1, 0.0));
        if !it.detail.is_empty() {
            dpt = dpt.min(fit_pt(std::slice::from_ref(&it.detail), w, (h as f64 * 0.4) as i64, dmax, 11.0, factor_minor(t), 1.1, 0.0));
        }
    }
    (lpt, dpt)
}

// ---------------------------------------------------------------------------
// Tiles

fn tiles(b: &mut B, items: &[Stat]) {
    let cx = b.cx;
    let t = cx.t;
    let area = area_of(b);
    let n = items.len();
    let centred = is_centred(b);
    let al = align_of(b);
    let pad = 300_000;
    let surf = card_surface(t);
    let label_first = matches!(t.card, CardStyle::Glass | CardStyle::Bento);
    let col_w = |per: usize| (area.w - GUTTER * (per as i64 - 1)) / per as i64;
    // Four tiles in a narrow area restack as 2 x 2 rather than squeezing.
    let mut per = n;
    if n > 2 && col_w(n) - 2 * pad < 1_350_000 {
        per = n.div_ceil(2);
    }
    let rows = n.div_ceil(per);
    let cell_w = if n <= 2 { col_w(per).min(4_300_000) } else { col_w(per) };
    let inner_w = cell_w - 2 * pad;

    let values: Vec<&str> = items.iter().map(|i| i.value.as_str()).collect();
    let row_max = (area.h - GUTTER * (rows as i64 - 1) - 60_000) / rows as i64;
    // Start generous and shrink together until the tallest tile fits its row.
    let mut scale = 1.0f64;
    let (vpt, lpt, dpt, need) = loop {
        let vpt = one_line_pt(&values, inner_w, (if rows > 1 { 48.0 } else { 66.0 }) * scale, 20.0, fig_factor(t));
        let (lpt, dpt) = label_sizes(t, items, inner_w, 1_500_000, 20.0 * scale.max(0.7), 15.0 * scale.max(0.7));
        let need = items
            .iter()
            .map(|it| lines_h(1, vpt, 0.95) + (8.0 * EMU_PER_PT) as i64 + label_h(t, it, inner_w, lpt, dpt))
            .max()
            .unwrap_or(0);
        if 520_000 + need + 340_000 <= row_max || scale < 0.45 {
            break (vpt, lpt, dpt, need);
        }
        scale -= 0.08;
    };
    let tile_h = (520_000 + need + 340_000).max(2_300_000.min(row_max)).min(row_max);
    let block_h = tile_h * rows as i64 + GUTTER * (rows as i64 - 1);
    let top = vcentre(area, block_h);
    let group_w = per as i64 * cell_w + GUTTER * (per as i64 - 1);
    let x0 = if centred { area.x + (area.w - group_w) / 2 } else { area.x };

    for (i, it) in items.iter().enumerate() {
        let (r, c) = (i / per, i % per);
        let in_row = if r + 1 == rows { n - r * per } else { per };
        let shift = if centred { (per - in_row) as i64 * (cell_w + GUTTER) / 2 } else { 0 };
        let tile = R::new(
            x0 + shift + c as i64 * (cell_w + GUTTER),
            top + r as i64 * (tile_h + GUTTER),
            cell_w,
            tile_h,
        );
        let accent = 1 + (i as u8 % 4);
        chrome_card(b, &format!("Stat card {}", i + 1), tile, Role::Item(i));
        let tick_w = 520_000;
        let tick_x = if centred { tile.x + (tile.w - tick_w) / 2 } else { tile.x + pad };
        let (tick_geom, tick_h) = if t.card == CardStyle::Sticker { (Geom::Round(50), 90_000) } else { (Geom::Rect, 56_000) };
        b.sp(
            Shape::deco(
                format!("Stat tick {}", i + 1),
                tick_geom,
                R::new(tick_x, tile.y + 340_000, tick_w, tick_h),
                Fill::Solid(Clr::accent(accent)),
            ),
            Role::Item(i),
        );
        let fig = Para::from_runs(vec![fig_run(t, &it.value, vpt, accent_on(t, accent, 3.0, &surf))])
            .no_bullet()
            .align(al)
            .line(95);
        let mut rest = label_paras(&it.label, &it.detail, lpt, dpt, Clr::tx1(), soft_ink(t, 74, &surf), al);
        let paras = if label_first {
            // A KPI card: the label on top, the figure standing on the base.
            let mut v = vec![rest.remove(0)];
            v.push(fig.before(6.0));
            if let Some(d) = rest.pop() {
                v.push(d.before(6.0));
            }
            v
        } else {
            rest[0] = rest[0].clone().before(8.0);
            let mut v = vec![fig];
            v.extend(rest);
            v
        };
        b.sp(
            Shape::text_box(
                format!("Stat {}", i + 1),
                R::new(tile.x + pad, tile.y + 520_000, inner_w, tile.h - 760_000),
                Text::new(paras).anchor(if label_first { Anchor::Bottom } else { Anchor::Top }),
            ),
            Role::Item(i),
        );
    }
}

// ---------------------------------------------------------------------------
// Rows: an editorial table

fn rows(b: &mut B, items: &[Stat]) {
    let cx = b.cx;
    let t = cx.t;
    let area = area_of(b);
    let n = items.len();
    let page = page_hex(t).to_string();
    let row_cap = match n {
        1 => 2_000_000,
        2 => 1_650_000,
        3 => 1_350_000,
        _ => 1_120_000,
    };
    let row_h = ((area.h - 150_000) / n as i64).min(row_cap);
    let top = vcentre(area, row_h * n as i64);
    let max_pt = (row_h as f64 * 0.74 / EMU_PER_PT / 1.2).min(if n == 1 { 96.0 } else { 72.0 });
    let values: Vec<&str> = items.iter().map(|i| i.value.as_str()).collect();
    let fpt = one_line_pt(&values, area.w / 2 - 120_000, max_pt, 24.0, fig_factor(t));
    let widest = values.iter().map(|v| v.chars().count()).max().unwrap_or(1) as f64;
    let est = (widest * fpt * fig_factor(t) * EMU_PER_PT) as i64;
    let fw = (est + 420_000).max(area.w * 30 / 100).min(area.w * 50 / 100);
    let lw = area.w - fw;
    let (lpt, dpt) = label_sizes(t, items, lw, row_h - 160_000, 24.0, 16.0);
    let fig_clr = accent_on(t, 1, 3.0, &page);

    for (i, it) in items.iter().enumerate() {
        let y = top + i as i64 * row_h;
        if i == 0 {
            b.sp(hline("Rule top", area.x, y, area.w, Clr::tx1().alpha(70), 1.5), Role::Item(0));
        }
        b.sp(
            Shape::text_box(
                format!("Stat {}", i + 1),
                R::new(area.x, y + 40_000, fw - 100_000, row_h - 80_000),
                Text::one(Para::from_runs(vec![fig_run(t, &it.value, fpt, fig_clr)]).no_bullet().line(95))
                    .anchor(Anchor::Middle),
            ),
            Role::Item(i),
        );
        b.sp(
            Shape::text_box(
                format!("Stat label {}", i + 1),
                R::new(area.x + fw, y + 60_000, lw, row_h - 120_000),
                Text::new(label_paras(&it.label, &it.detail, lpt, dpt, Clr::tx1(), soft_ink(t, 74, &page), Align::Left))
                    .anchor(Anchor::Middle),
            ),
            Role::Item(i),
        );
        b.sp(
            hline(&format!("Rule {}", i + 1), area.x, y + row_h, area.w, Clr::tx1().alpha(if i + 1 == n { 70 } else { 28 }), if i + 1 == n { 1.5 } else { 0.75 }),
            Role::Item(i),
        );
    }
}

// ---------------------------------------------------------------------------
// Hero: one giant figure, the rest as a list

/// Where the hero and its list sit: (hero rect, list rect), list empty for
/// a single figure.
fn hero_split(area: R, rest: usize, hero_pct: i64, max_h: i64) -> (R, R) {
    let h = (area.h - 100_000).min(max_h);
    let y = vcentre(area, h);
    if rest == 0 {
        return (R::new(area.x, y, area.w, h), R::new(area.right(), y, 0, h));
    }
    let hw = area.w * hero_pct / 100;
    (
        R::new(area.x, y, hw, h),
        R::new(area.x + hw + GUTTER, y, area.w - hw - GUTTER, h),
    )
}

fn hero_field(b: &mut B, items: &[Stat]) {
    let cx = b.cx;
    let t = cx.t;
    let area = area_of(b);
    let page = page_hex(t).to_string();
    let (hero, list) = hero_split(area, items.len() - 1, 46, 4_000_000);
    let filled = field_reads(t, 1);
    let ink = if filled { on_accent(t, 1) } else { Clr::tx1() };
    let surf = if filled { hex_of(t, &Clr::accent(1)) } else { card_surface(t) };
    let sub = if filled { ink } else { soft_ink(t, 76, &surf) };
    if filled {
        b.sp(
            Shape::deco("Stat hero field", radius(t), hero, Fill::Solid(Clr::accent(1))),
            Role::Item(0),
        );
    } else {
        chrome_card(b, "Stat hero field", hero, Role::Item(0));
    }
    hero_text(b, &items[0], hero, (ink, sub, ink_for_figure(t, filled, &surf)), 112.0, false);

    // The rest: a quiet list with a status stripe in each figure's colour.
    let m = items.len() - 1;
    if m == 0 {
        return;
    }
    let eh = (list.h / m as i64).min(1_500_000);
    let y0 = list.y + (list.h - eh * m as i64) / 2;
    let values: Vec<&str> = items[1..].iter().map(|i| i.value.as_str()).collect();
    let tw = list.w - 220_000;
    let fpt = one_line_pt(&values, tw, (eh as f64 * 0.42 / EMU_PER_PT / 1.2).min(54.0), 22.0, fig_factor(t));
    let (lpt, dpt) = label_sizes(t, &items[1..], tw, eh - 260_000 - lines_h(1, fpt, 1.0), 20.0, 14.0);
    for (j, it) in items[1..].iter().enumerate() {
        let i = j + 1;
        let y = y0 + j as i64 * eh;
        let accent = 1 + ((i as u8) % 4);
        b.sp(
            Shape::deco(
                format!("Stat stripe {}", i + 1),
                Geom::Rect,
                R::new(list.x, y + 110_000, 56_000, eh - 220_000),
                Fill::Solid(Clr::accent(accent)),
            ),
            Role::Item(i),
        );
        list_entry(b, it, i, R::new(list.x + 220_000, y, tw, eh), fpt, lpt, dpt, accent_on(t, accent, 3.0, &page), soft_ink(t, 74, &page));
    }
}

/// The colour the hero figure takes: on the accent field the field's ink,
/// otherwise the accent when it reads.
fn ink_for_figure(t: &ThemeSpec, filled: bool, surf: &str) -> Clr {
    if filled {
        on_accent(t, 1)
    } else {
        accent_on(t, 1, 3.0, surf)
    }
}

/// The hero's text: label above the giant figure, detail below.
fn hero_text(b: &mut B, hero_it: &Stat, rect: R, (lclr, dclr, fclr): (Clr, Clr, Clr), max_pt: f64, figure_first: bool) {
    let t = b.cx.t;
    let pad = 380_000;
    let w = rect.w - 2 * pad;
    let fpt = one_line_pt(&[hero_it.value.as_str()], w, max_pt, 36.0, fig_factor(t))
        .min(rect.h as f64 * 0.42 / EMU_PER_PT / 1.2);
    let rest_h = rect.h - 2 * 300_000 - lines_h(1, fpt, 0.95);
    let (lpt, dpt) = label_sizes(t, std::slice::from_ref(hero_it), w, rest_h, 24.0, 17.0);
    let fig = Para::from_runs(vec![fig_run(t, &hero_it.value, fpt, fclr)]).no_bullet().line(95);
    let mut lab = label_paras(&hero_it.label, "", lpt, dpt, lclr, dclr, Align::Left);
    let mut paras = Vec::new();
    if figure_first {
        paras.push(fig);
        lab[0] = lab[0].clone().before(6.0);
        paras.extend(lab);
    } else {
        paras.extend(lab);
        paras.push(fig.before(6.0));
    }
    if !hero_it.detail.is_empty() {
        paras.push(
            Para::from_runs(vec![Run::new(hero_it.detail.as_str()).size(dpt).color(dclr)])
                .no_bullet()
                .before(8.0),
        );
    }
    b.sp(
        Shape::text_box(
            "Stat 1",
            R::new(rect.x + pad, rect.y + 300_000, w, rect.h - 600_000),
            Text::new(paras).anchor(Anchor::Middle),
        ),
        Role::Item(0),
    );
}

/// One list entry: figure over label (and detail).
#[allow(clippy::too_many_arguments)]
fn list_entry(b: &mut B, it: &Stat, i: usize, rect: R, fpt: f64, lpt: f64, dpt: f64, fclr: Clr, dclr: Clr) {
    let t = b.cx.t;
    let mut paras = vec![Para::from_runs(vec![fig_run(t, &it.value, fpt, fclr)]).no_bullet().line(95)];
    let mut rest = label_paras(&it.label, &it.detail, lpt, dpt, Clr::tx1(), dclr, Align::Left);
    rest[0] = rest[0].clone().before(3.0);
    paras.extend(rest);
    b.sp(
        Shape::text_box(
            format!("Stat {}", i + 1),
            R::new(rect.x, rect.y + 60_000, rect.w, rect.h - 120_000),
            Text::new(paras).anchor(Anchor::Middle),
        ),
        Role::Item(i),
    );
}

fn hero_bento(b: &mut B, items: &[Stat]) {
    let cx = b.cx;
    let t = cx.t;
    let area = area_of(b);
    let (hero, list) = hero_split(area, items.len() - 1, 54, 4_000_000);
    let filled = field_reads(t, 1);
    let tile_geom = Geom::Round(t.radius.max(14));
    let (ink, sub, fig_clr) = if filled {
        b.sp(
            Shape::deco("Stat hero tile", tile_geom.clone(), hero, Fill::Solid(Clr::accent(1))),
            Role::Item(0),
        );
        (on_accent(t, 1), on_accent(t, 1), on_accent(t, 1))
    } else {
        chrome_card(b, "Stat hero tile", hero, Role::Item(0));
        let surf = card_surface(t);
        (Clr::tx1(), soft_ink(t, 76, &surf), accent_on(t, 1, 3.0, &surf))
    };
    hero_text(b, &items[0], hero, (ink, sub, fig_clr), 120.0, false);

    let m = items.len() - 1;
    if m == 0 {
        return;
    }
    let surf = card_surface(t);
    let th = (list.h - GUTTER * (m as i64 - 1)) / m as i64;
    let values: Vec<&str> = items[1..].iter().map(|i| i.value.as_str()).collect();
    let tw = list.w - 2 * 300_000;
    let fpt = one_line_pt(&values, tw, (th as f64 * 0.4 / EMU_PER_PT / 1.2).min(60.0), 22.0, fig_factor(t));
    let (lpt, dpt) = label_sizes(t, &items[1..], tw, th - 280_000 - lines_h(1, fpt, 1.0), 20.0, 14.0);
    for (j, it) in items[1..].iter().enumerate() {
        let i = j + 1;
        let tile = R::new(list.x, list.y + j as i64 * (th + GUTTER), list.w, th);
        chrome_card(b, &format!("Stat tile {}", i + 1), tile, Role::Item(i));
        let accent = 1 + ((i as u8) % 4);
        list_entry(
            b,
            it,
            i,
            R::new(tile.x + 300_000, tile.y, tw, tile.h),
            fpt,
            lpt,
            dpt,
            accent_on(t, accent, 3.0, &surf),
            soft_ink(t, 74, &surf),
        );
    }
}

fn hero_poster(b: &mut B, items: &[Stat]) {
    let cx = b.cx;
    let t = cx.t;
    let area = area_of(b);
    let page = page_hex(t).to_string();
    let (hero, list) = hero_split(area, items.len() - 1, 54, 3_800_000);
    let fig_clr = accent_on(t, 1, 3.0, &page);
    // Poster type: no field, the figure is the picture.
    let w = hero.w - 260_000;
    let fpt = one_line_pt(&[items[0].value.as_str()], w, 150.0, 40.0, fig_factor(t))
        .min(hero.h as f64 * 0.5 / EMU_PER_PT / 1.2);
    let (lpt, dpt) = label_sizes(t, &items[..1], w, hero.h - lines_h(1, fpt, 0.9) - 200_000, 26.0, 17.0);
    let mut paras = vec![Para::from_runs(vec![fig_run(t, &items[0].value, fpt, fig_clr)]).no_bullet().line(90)];
    let mut rest = label_paras(&items[0].label, &items[0].detail, lpt, dpt, Clr::tx1(), soft_ink(t, 76, &page), Align::Left);
    rest[0] = rest[0].clone().before(4.0);
    paras.extend(rest);
    b.sp(
        Shape::text_box("Stat 1", R::new(hero.x, hero.y, w, hero.h), Text::new(paras).anchor(Anchor::Middle)),
        Role::Item(0),
    );

    let m = items.len() - 1;
    if m == 0 {
        return;
    }
    b.sp(
        vline("Stat divider", list.x - GUTTER / 2, list.y + 100_000, list.h - 200_000, Clr::tx1().alpha(40), 0.75, None),
        Role::Item(1),
    );
    let eh = (list.h / m as i64).min(1_500_000);
    let y0 = list.y + (list.h - eh * m as i64) / 2;
    let values: Vec<&str> = items[1..].iter().map(|i| i.value.as_str()).collect();
    let tw = list.w - 160_000;
    let fpt = one_line_pt(&values, tw, (eh as f64 * 0.42 / EMU_PER_PT / 1.2).min(54.0), 22.0, fig_factor(t));
    let (lpt, dpt) = label_sizes(t, &items[1..], tw, eh - 260_000 - lines_h(1, fpt, 1.0), 20.0, 14.0);
    for (j, it) in items[1..].iter().enumerate() {
        let i = j + 1;
        let y = y0 + j as i64 * eh;
        b.sp(hline(&format!("Rule {}", i + 1), list.x, y, list.w, Clr::tx1().alpha(if j == 0 { 70 } else { 30 }), if j == 0 { 1.5 } else { 0.75 }), Role::Item(i));
        list_entry(b, it, i, R::new(list.x + 160_000, y, tw, eh), fpt, lpt, dpt, Clr::tx1(), soft_ink(t, 74, &page));
    }
}

// ---------------------------------------------------------------------------
// Columns: figure over label, thin rules between

/// What sits above each figure.
#[derive(Clone, Copy, PartialEq)]
enum Lead {
    /// A short accent tick.
    Tick,
    /// A bracketed index, `[01]` (technical drawings).
    Bracket,
    /// Nothing.
    None,
}

fn open_columns(b: &mut B, items: &[Stat]) {
    let cx = b.cx;
    let t = cx.t;
    let area = area_of(b);
    let page = page_hex(t).to_string();
    let n = items.len();
    let al = align_of(b);
    let centred = al == Align::Center;
    let (lead, dash) = match t.look.header {
        Header::Banner => (Lead::Bracket, Some("dash")),
        Header::Poster => (Lead::None, None),
        _ => (Lead::Tick, None),
    };
    // Four columns in a narrow area restack as 2 x 2.
    let cw_for = |per: usize| area.w / per as i64;
    let mut per = n;
    if n > 2 && cw_for(n) - 460_000 < 1_450_000 {
        per = n.div_ceil(2);
    }
    let rows = n.div_ceil(per);
    let cw = cw_for(per);
    let pad_l = if centred { 160_000 } else { 280_000 };
    let pad_r = if centred { 160_000 } else { 200_000 };
    let min_inner = cw - pad_r - pad_l;

    let values: Vec<&str> = items.iter().map(|i| i.value.as_str()).collect();
    let rows_avail = area.h - 120_000;
    let row_cap = rows_avail / rows as i64;
    let lead_h = match lead {
        Lead::Tick => 56_000 + 190_000,
        Lead::Bracket => lines_h(1, 12.0, 1.0) + (6.0 * EMU_PER_PT) as i64,
        Lead::None => 0,
    };
    let max_fig = (if rows > 1 { 48.0f64 } else { 64.0 }).min(row_cap as f64 * 0.3 / EMU_PER_PT / 1.2);
    let fpt = one_line_pt(&values, min_inner, max_fig, 24.0, fig_factor(t));
    let (lpt, dpt) = label_sizes(t, items, min_inner, row_cap - lead_h - lines_h(1, fpt, 1.0) - 200_000, 20.0, 15.0);
    let need = items
        .iter()
        .map(|it| lead_h + lines_h(1, fpt, 0.95) + (6.0 * EMU_PER_PT) as i64 + label_h(t, it, min_inner, lpt, dpt))
        .max()
        .unwrap_or(0);
    let rh = (need + 160_000).min(row_cap);
    let block_h = rh * rows as i64 + 120_000 * (rows as i64 - 1);
    let top = vcentre(area, block_h);
    grid_backing(b, R::new(area.x - 220_000, top - 160_000, area.w + 440_000, block_h + 320_000));

    for (i, it) in items.iter().enumerate() {
        let (r, c) = (i / per, i % per);
        let in_row = if r + 1 == rows { n - r * per } else { per };
        let shift = if centred { (per - in_row) as i64 * cw / 2 } else { 0 };
        let x = area.x + shift + c as i64 * cw;
        let y = top + r as i64 * (rh + 120_000);
        if c > 0 {
            b.sp(
                vline(&format!("Rule {}", i + 1), x, y, rh, Clr::tx1().alpha(30), 0.75, dash),
                Role::Item(i),
            );
        }
        if r > 0 && c == 0 {
            b.sp(hline(&format!("Rule row {}", r + 1), area.x, y - 60_000, area.w, Clr::tx1().alpha(22), 0.75), Role::Item(i));
        }
        let tx = if c == 0 && !centred { x } else { x + pad_l };
        let tw = cw - pad_r - (tx - x);
        let mut paras: Vec<Para> = Vec::new();
        match lead {
            Lead::Tick => {
                let tick_w = 440_000;
                let tick_x = if centred { x + (cw - tick_w) / 2 } else { tx };
                b.sp(
                    Shape::deco(
                        format!("Stat tick {}", i + 1),
                        Geom::Rect,
                        R::new(tick_x, y + 40_000, tick_w, 56_000),
                        Fill::Solid(Clr::accent(1)),
                    ),
                    Role::Item(i),
                );
            }
            Lead::Bracket => paras.push(
                Para::from_runs(vec![Run::new(format!("[{:02}]", i + 1))
                    .size(12.0)
                    .bold()
                    .spacing(1.2)
                    .color(accent_on(t, 1, 4.5, &page))])
                .no_bullet()
                .align(al)
                .after(6.0),
            ),
            Lead::None => {}
        }
        paras.push(
            Para::from_runs(vec![fig_run(t, &it.value, fpt, accent_on(t, 1, 3.0, &page))])
                .no_bullet()
                .align(al)
                .line(95),
        );
        let mut rest = label_paras(&it.label, &it.detail, lpt, dpt, Clr::tx1(), soft_ink(t, 74, &page), al);
        rest[0] = rest[0].clone().before(6.0);
        paras.extend(rest);
        let text_y = y + if lead == Lead::Tick { 56_000 + 190_000 + 40_000 } else { 0 };
        let text_h = rh - (text_y - y);
        b.sp(
            Shape::text_box(format!("Stat {}", i + 1), R::new(tx, text_y, tw, text_h), Text::new(paras)),
            Role::Item(i),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::deck::palette::PALETTES;
    use crate::deck::theme::{ThemeSpec, THEMES};

    pub(crate) const HEADERS: [Header; 7] = [
        Header::TopLeft,
        Header::Banner,
        Header::Sidebar { right: false },
        Header::Sidebar { right: true },
        Header::Rail,
        Header::Centered,
        Header::Poster,
    ];

    pub(crate) fn ctx_for<'a>(t: &'a ThemeSpec) -> Ctx<'a> {
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

    pub(crate) fn stat_slide(n: usize) -> DesignedSlide {
        let all = [
            ("$4.2B", "addressable market", "Across 40 countries"),
            ("38%", "yearly growth", ""),
            ("12M", "teams on legacy suites", "Still emailing files"),
            ("2.1x", "pipeline coverage", ""),
        ];
        let mut s = DesignedSlide::new("A market that is ready").kicker("Market");
        s.layout = Some(LayoutKind::Stats);
        s.blocks = all
            .iter()
            .take(n)
            .map(|(v, l, d)| {
                let mut b = Block::stat(*v, *l);
                if !d.is_empty() {
                    b.text = vec![d.to_string()];
                }
                b
            })
            .collect();
        s
    }

    pub(crate) fn item_of(name_role: Role) -> Option<usize> {
        match name_role {
            Role::Item(i) => Some(i),
            _ => None,
        }
    }

    fn shapes(b: &Built) -> Vec<(&Shape, Role)> {
        b.items
            .iter()
            .filter_map(|i| match &i.el {
                El::Sp(s) => Some((s, i.role)),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn text_of(s: &Shape) -> String {
        s.text
            .as_ref()
            .map(|t| t.paras.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    }

    #[test]
    fn every_stats_variant_builds_for_one_to_four_figures_in_order() {
        let base = crate::deck::theme::theme("harbor").unwrap();
        for header in HEADERS {
            for style in [StatsStyle::Tiles, StatsStyle::Rows, StatsStyle::Hero, StatsStyle::Columns] {
                for card in [CardStyle::Filled, CardStyle::Rule, CardStyle::Bento, CardStyle::Sticker] {
                    for n in 1..=4usize {
                        let t = ThemeSpec { look: Look { header, stats: style, ..base.look }, card, ..*base };
                        let cx = ctx_for(&t);
                        let built = stats(&stat_slide(n), &cx);
                        let who = format!("{header:?} {style:?} {card:?} n={n}");
                        let mut last = 0usize;
                        let mut seen = vec![false; n];
                        for (sh, role) in shapes(&built) {
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
                        assert!(seen.iter().all(|x| *x), "{who}: some figure has no elements");
                        // The n-th figure's text lives in Role::Item(n).
                        for (sh, role) in shapes(&built) {
                            let txt = text_of(sh);
                            for (i, v) in ["$4.2B", "38%", "12M", "2.1x"].iter().take(n).enumerate() {
                                if txt.contains(v) {
                                    assert_eq!(role, Role::Item(i), "{who}: '{v}' is tagged {role:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn stats_derive_from_bullets_when_there_are_no_blocks() {
        let t = crate::deck::theme::theme("ivory").unwrap();
        let s = DesignedSlide::new("Growth").bullets(["38% faster", "2.1x pipeline", "no figure here"]);
        let built = stats(&s, &ctx_for(t));
        let figs: Vec<String> = shapes(&built).iter().map(|(s, _)| text_of(s)).filter(|t| t.contains('%') || t.contains("2.1x")).collect();
        assert_eq!(figs.len(), 2);
    }

    #[test]
    fn the_four_stat_designs_are_different_compositions() {
        // The same four figures laid out by each style must not share
        // geometry: compare the rectangles of the figures' text boxes.
        let base = crate::deck::theme::theme("harbor").unwrap();
        let mut layouts: Vec<Vec<R>> = Vec::new();
        for style in [StatsStyle::Tiles, StatsStyle::Rows, StatsStyle::Hero, StatsStyle::Columns] {
            let t = ThemeSpec { look: Look { stats: style, ..base.look }, ..*base };
            let built = stats(&stat_slide(4), &ctx_for(&t));
            layouts.push(
                shapes(&built)
                    .iter()
                    .filter(|(s, r)| matches!(r, Role::Item(_)) && s.text.is_some())
                    .filter_map(|(s, _)| s.rect)
                    .collect(),
            );
        }
        for i in 0..layouts.len() {
            for j in i + 1..layouts.len() {
                assert_ne!(layouts[i], layouts[j], "styles {i} and {j} draw the same geometry");
            }
        }
    }

    /// The colour behind a text shape: the last filled shape emitted
    /// before it that contains its centre, composited over the page.
    pub(crate) fn surface_behind(t: &ThemeSpec, built: &Built, idx: usize) -> String {
        let page = page_hex(t).to_string();
        let mut surface = page.clone();
        let Some(r) = (match &built.items[idx].el {
            El::Sp(s) => s.rect,
            _ => None,
        }) else {
            return page;
        };
        let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
        for it in &built.items[..idx] {
            let El::Sp(s) = &it.el else { continue };
            let Some(sr) = s.rect else { continue };
            if sr.w == 0 || sr.h == 0 || s.ph.is_some() {
                continue;
            }
            // Rotated shapes: test against a slightly grown box.
            if cx >= sr.x - 40_000 && cx <= sr.right() + 40_000 && cy >= sr.y - 40_000 && cy <= sr.bottom() + 40_000 {
                if let Fill::Solid(c) = &s.fill {
                    // Text boxes the same size as a card are not surfaces.
                    if !s.text_box {
                        surface = t.resolve_over(c, &surface);
                    }
                }
            }
        }
        surface
    }

    /// Every run of text in `built` reads on what it sits on: 3:1 for large
    /// type (WCAG: >= 18 pt, or bold >= 14 pt), 4.5:1 otherwise.
    pub(crate) fn assert_text_contrast(t: &ThemeSpec, built: &Built, who: &str) {
        for (idx, it) in built.items.iter().enumerate() {
            let El::Sp(s) = &it.el else { continue };
            // Header furniture is the integrator's; check this layout's items.
            if s.ph.is_some() || !matches!(it.role, Role::Item(_)) {
                continue;
            }
            let Some(text) = &s.text else { continue };
            let surface = surface_behind(t, built, idx);
            // A badge/avatar carries its own fill.
            let own = match &s.fill {
                Fill::Solid(c) => t.resolve_over(c, &surface),
                _ => surface.clone(),
            };
            for p in &text.paras {
                for r in &p.runs {
                    if r.text.trim().is_empty() {
                        continue;
                    }
                    let clr = r.color.unwrap_or(Clr::tx1());
                    let fg = t.resolve_over(&clr, &own);
                    let pt = r.size.unwrap_or(18.0);
                    let bold = r.bold.unwrap_or(false);
                    let need = if pt >= 18.0 || (pt >= 14.0 && bold) { 3.0 } else { 4.5 };
                    let c = contrast(&fg, &own);
                    assert!(
                        c >= need,
                        "{who} '{}' ({pt}pt) text '{}' {fg} on {own}: {c:.2} < {need}",
                        s.name,
                        r.text
                    );
                }
            }
        }
    }

    #[test]
    fn stat_figures_and_labels_read_in_every_theme_and_palette() {
        for base in THEMES {
            for pal in PALETTES {
                for style in [StatsStyle::Tiles, StatsStyle::Rows, StatsStyle::Hero, StatsStyle::Columns] {
                    let t = ThemeSpec { look: Look { stats: style, ..base.look }, ..base.with_palette(pal) };
                    let built = stats(&stat_slide(4), &ctx_for(&t));
                    assert_text_contrast(&t, &built, &format!("{} in {} {style:?}", base.id, pal.id));
                }
            }
        }
    }

    #[test]
    fn no_stat_label_is_squeezed_to_a_few_characters_per_line() {
        let base = crate::deck::theme::theme("harbor").unwrap();
        for header in HEADERS {
            for style in [StatsStyle::Tiles, StatsStyle::Rows, StatsStyle::Hero, StatsStyle::Columns] {
                for n in 1..=4usize {
                    let t = ThemeSpec { look: Look { header, stats: style, ..base.look }, ..*base };
                    let built = stats(&stat_slide(n), &ctx_for(&t));
                    assert_min_chars(&built, 8, &format!("{header:?} {style:?} n={n}"));
                }
            }
        }
    }

    /// Text boxes whose estimated text height exceeds their box (same
    /// estimate as the generic gate in `layouts/mod.rs`).
    pub(crate) fn overflows(t: &ThemeSpec, built: &Built) -> Vec<String> {
        let mut out = Vec::new();
        for it in &built.items {
            let El::Sp(sh) = &it.el else { continue };
            if sh.ph.is_some() {
                continue;
            }
            let (Some(text), Some(rect)) = (&sh.text, sh.rect) else { continue };
            let (l, tp, r, bt) = text.insets;
            let base_w = rect.w - l - r;
            let mut need = 0.0f64;
            for p in &text.paras {
                let s: String = p.runs.iter().map(|r| r.text.as_str()).collect();
                let pt = p.runs.iter().filter_map(|r| r.size).fold(0.0, f64::max).max(if p.runs.iter().any(|r| r.size.is_some()) { 0.0 } else { 18.0 });
                let major = p.runs.first().is_some_and(|r| r.font == Some(FontRef::Major));
                let factor = glyph_factor(if major { t.fonts.major } else { t.fonts.minor });
                let w = base_w - p.margin.map_or(0, |(m, _)| m.max(0));
                let lines = line_count(&s, pt, w.max(1), factor) as f64;
                let spacing = p.line.map_or(1.0, |l| l as f64 / 100.0);
                need += lines * pt * 1.2 * spacing + p.before.unwrap_or(0.0) + p.after.unwrap_or(0.0);
            }
            let need = (need * EMU_PER_PT) as i64;
            let have = rect.h - tp - bt;
            if need as f64 > have as f64 * 1.06 + 25_000.0 {
                out.push(format!("'{}': needs {need}, has {have}", sh.name));
            }
        }
        out
    }

    #[test]
    fn dense_figures_fit_their_boxes_in_every_style_and_header() {
        let mut bad = Vec::new();
        for base in THEMES {
            for header in HEADERS {
                for style in [StatsStyle::Tiles, StatsStyle::Rows, StatsStyle::Hero, StatsStyle::Columns] {
                    let t = ThemeSpec { look: Look { header, stats: style, ..base.look }, ..*base };
                    for o in overflows(&t, &stats(&stat_slide(4), &ctx_for(&t))) {
                        bad.push(format!("{} {header:?} {style:?} {o}", base.id));
                    }
                }
            }
        }
        bad.dedup();
        assert!(bad.is_empty(), "{} overflow(s):\n{}", bad.len(), bad.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
    }

    /// Every paragraph longer than a few words has room for >= `min`
    /// characters per line at its size (Arial-wide glyphs).
    pub(crate) fn assert_min_chars(built: &Built, min: usize, who: &str) {
        for it in &built.items {
            let El::Sp(s) = &it.el else { continue };
            if s.ph.is_some() {
                continue;
            }
            let (Some(text), Some(r)) = (&s.text, s.rect) else { continue };
            let (l, _, rr, _) = text.insets;
            for p in &text.paras {
                let txt: String = p.runs.iter().map(|r| r.text.as_str()).collect();
                let pt = p.runs.iter().filter_map(|r| r.size).fold(0.0, f64::max);
                if txt.chars().count() <= 8 || pt == 0.0 {
                    continue;
                }
                let w = (r.w - l - rr - p.margin.map_or(0, |(m, _)| m)) as f64 / EMU_PER_PT;
                let chars = w / (pt * 0.55);
                assert!(chars >= min as f64, "{who} '{}': only {chars:.1} chars/line at {pt}pt for '{txt}'", s.name);
            }
        }
    }
}
