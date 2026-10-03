//! Agendas and contents lists in two compositions.
//!
//! * **Rows**: numbered rows with hairlines. The numeral's treatment follows
//!   the header: accent numerals (top-left), square chips (banner), round
//!   badges (sidebar), light serif numerals (rail), heavy numerals under a
//!   heavy rule (poster). Long agendas split into two columns, or compress
//!   in a narrow area.
//! * **Grid**: large numerals with the item label beneath, two to four per
//!   row. Sticker decks set the numeral in an outlined disc, tile decks in a
//!   soft card, the rest leave it open over a hairline (blueprint brackets
//!   the numeral).

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

pub(crate) fn agenda(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let items: Vec<String> = s.bullets.iter().take(8).cloned().collect();
    if !items.is_empty() {
        match cx.t.look.agenda {
            AgendaStyle::Rows => rows(&mut b, &items),
            AgendaStyle::Grid => grid(&mut b, &items),
        }
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}

/// Accent `n` as numeral text on either the page or a card (`bg2`/`bg1`):
/// the accent when it reads on both, else the ink.
fn accent_on_cards(t: &ThemeSpec, n: u8, min: f64) -> Clr {
    let hex = t.palette.accents[(n as usize - 1) % 6];
    let ok = [page_hex(t).to_string(), t.resolve(&Clr::bg2()).0, t.resolve(&Clr::bg1()).0]
        .iter()
        .all(|bg| contrast(hex, bg) >= min);
    if ok {
        Clr::accent(n)
    } else {
        Clr::tx1()
    }
}

fn pt_emu(pt: f64) -> i64 {
    (pt * EMU_PER_PT).round() as i64
}

/// The largest size (up to `max`) at which every item fits its own box.
fn fit_each(t: &ThemeSpec, items: &[String], w: i64, h: i64, max: f64, min: f64) -> f64 {
    items
        .iter()
        .map(|s| fit_pt(std::slice::from_ref(s), w, h, max, min, factor_minor(t), 1.05, 0.0))
        .fold(max, f64::min)
}

// ---------------------------------------------------------------------------
// Rows

#[derive(Clone, Copy, PartialEq)]
enum Numeral {
    /// Bold accent numerals (top-left, centred).
    Plain,
    /// A filled square chip (banner).
    Square,
    /// A filled round badge (sidebar).
    Circle,
    /// Light serif numerals (rail).
    Light,
    /// Heavy numerals under a heavy rule (poster).
    Heavy,
}

fn rows(b: &mut B, items: &[String]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let n = items.len();
    let kind = match t.look.header {
        Header::Banner => Numeral::Square,
        Header::Sidebar { .. } => Numeral::Circle,
        Header::Rail => Numeral::Light,
        Header::Poster => Numeral::Heavy,
        _ => Numeral::Plain,
    };
    let two = n > 4 && area.w >= 8_500_000;
    let ncols = if two { 2 } else { 1 };
    let per_col = n.div_ceil(ncols);
    let cols = columns(area, ncols, GUTTER * 2);
    let row_h = (area.h / per_col as i64).min(if two { 1_000_000 } else { 900_000 });
    let chip_d = 520_000.min(row_h - 160_000);
    let (num_w, num_pt) = match kind {
        Numeral::Plain => (900_000, 30.0),
        Numeral::Light => (1_100_000, 36.0),
        Numeral::Heavy => (1_250_000, 40.0),
        Numeral::Square | Numeral::Circle => (chip_d, 16.0),
    };
    let gap = if matches!(kind, Numeral::Square | Numeral::Circle) { 260_000 } else { 120_000 };
    let text_w = cols[0].w - num_w - gap;
    let cap = match kind {
        Numeral::Light => 26.0,
        Numeral::Heavy => 28.0,
        _ => 24.0,
    };
    let pt = fit_each(t, items, text_w, row_h - 100_000, cap, 14.0);
    let top = vcentre(area, row_h * per_col as i64);
    if kind == Numeral::Heavy {
        for (c, col) in cols.iter().enumerate() {
            let mut rule = Shape::deco(format!("Agenda top rule {}", c + 1), Geom::Line, R::new(col.x, top, col.w, 0), Fill::None);
            rule.line = Some(Line::new(Clr::tx1(), 3.0));
            b.sp(rule, Role::Deco);
        }
    }
    for (i, text) in items.iter().enumerate() {
        let (c, r) = if two { (i / per_col, i % per_col) } else { (0, i) };
        let col = cols[c.min(cols.len() - 1)];
        let y = top + r as i64 * row_h;
        let accent = match kind {
            Numeral::Plain => 1 + (i as u8 % 4),
            _ => 1,
        };
        let label = format!("{:02}", i + 1);
        match kind {
            Numeral::Square | Numeral::Circle => {
                let (fill, ink) = inked_fill(t, Clr::accent(accent), 100, 3.0);
                let geom = if kind == Numeral::Circle { Geom::Ellipse } else { radius(t) };
                let mut chip = Shape::new(
                    format!("Agenda number {}", i + 1),
                    geom,
                    R::new(col.x, y + (row_h - chip_d) / 2, chip_d, chip_d),
                )
                .fill(Fill::Solid(fill));
                chip.decorative = true;
                chip.text = Some(
                    Text::one(
                        Para::from_runs(vec![Run::new(label).size(num_pt).bold().font(FontRef::Major).color(ink)])
                            .align(Align::Center)
                            .no_bullet(),
                    )
                    .anchor(Anchor::Middle),
                );
                b.sp(chip, Role::Item(i));
            }
            _ => {
                let mut run = Run::new(label)
                    .size(num_pt)
                    .font(FontRef::Major)
                    .color(accent_text(t, accent, 3.0));
                if kind != Numeral::Light || t.bold_titles {
                    run = run.bold();
                }
                b.sp(
                    Shape::text_box(
                        format!("Agenda number {}", i + 1),
                        R::new(col.x, y, num_w, row_h - 20_000),
                        Text::one(Para::from_runs(vec![run]).no_bullet()).anchor(Anchor::Middle),
                    ),
                    Role::Item(i),
                );
            }
        }
        b.sp(
            Shape::text_box(
                format!("Agenda item {}", i + 1),
                R::new(col.x + num_w + gap, y, text_w, row_h - 20_000),
                Text::one(Para::from_runs(vec![Run::new(text.clone()).size(pt)]).no_bullet()).anchor(Anchor::Middle),
            ),
            Role::Item(i),
        );
        let mut rule = Shape::deco(format!("Agenda rule {}", i + 1), Geom::Line, R::new(col.x, y + row_h, col.w, 0), Fill::None);
        rule.line = Some(Line::new(Clr::tx1().alpha(if kind == Numeral::Heavy { 30 } else { 16 }), 0.75));
        b.sp(rule, Role::Item(i));
    }
}

// ---------------------------------------------------------------------------
// Grid

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    /// Numeral over a hairline, no box.
    Open,
    /// Numeral and label in a soft card.
    Tile,
    /// Numeral in an outlined disc with a hard shadow.
    Sticker,
}

/// Columns for `n` items in `w`: 2-4 per row, balanced across rows, never
/// narrower than a readable label column.
fn grid_columns(n: usize, w: i64, gap: i64) -> usize {
    let max_cols = (((w + gap) / (2_300_000 + gap)).clamp(2, 4)) as usize;
    let want = match n {
        0..=4 => n.max(1),
        5 | 6 => 3,
        _ => 4,
    };
    let c = want.min(max_cols).max(1);
    let rows = n.div_ceil(c);
    n.div_ceil(rows)
}

fn grid(b: &mut B, items: &[String]) {
    let cx = b.cx;
    let t = cx.t;
    let area = body_area(cx);
    let hdr = t.look.header;
    let n = items.len();
    let centred = hdr == Header::Centered;
    let cell = match t.card {
        CardStyle::Sticker => Cell::Sticker,
        CardStyle::Raised | CardStyle::Filled | CardStyle::Bento => Cell::Tile,
        _ => Cell::Open,
    };
    let gap = GUTTER;
    // A drafting-grid deck sets each open cell on an opaque sheet.
    let open_sheet = cell == Cell::Open && t.deco == Deco::Grid;
    let cell = if open_sheet { Cell::Tile } else { cell };
    let ncols = grid_columns(n, area.w, gap);
    let nrows = n.div_ceil(ncols);
    let cols = columns(area, ncols, gap);
    let cell_w = cols[0].w;
    let row_gap = 280_000;
    let budget = (area.h - row_gap * (nrows as i64 - 1)) / nrows as i64;
    let bump = if hdr == Header::Poster { 6.0 } else { 0.0 };
    let (num_pt, disc_d): (f64, i64) = match (cell, nrows) {
        (Cell::Open, 1) => (60.0, 0),
        (Cell::Open, 2) => (44.0, 0),
        (Cell::Open, 3) => (32.0, 0),
        (Cell::Open, _) => (26.0, 0),
        (Cell::Tile, 1) => (52.0, 0),
        (Cell::Tile, 2) => (40.0, 0),
        (Cell::Tile, 3) => (30.0, 0),
        (Cell::Tile, _) => (24.0, 0),
        (Cell::Sticker, 1) => (0.0, 960_000),
        (Cell::Sticker, 2) => (0.0, 780_000),
        (Cell::Sticker, 3) => (0.0, 620_000),
        (Cell::Sticker, _) => (0.0, 520_000),
    };
    let num_pt = if num_pt > 0.0 { num_pt + bump } else { 0.0 };
    let bracket = hdr == Header::Banner && (cell == Cell::Open || open_sheet);
    let num_pt = if bracket { num_pt * 0.75 } else { num_pt };
    let pad = if cell == Cell::Tile { 200_000 } else { 0 };
    let num_h = if cell == Cell::Sticker { disc_d } else { pt_emu(num_pt * 1.2) };
    let fixed = match cell {
        Cell::Open => 150_000 + num_h + 90_000,
        Cell::Tile => 2 * pad + num_h + 80_000,
        Cell::Sticker => disc_d + 70_000 + 170_000,
    };
    let label_w = cell_w - 2 * pad - if cell == Cell::Tile { 0 } else { 60_000 };
    let label_budget = (budget - fixed).max(400_000);
    let cap = match nrows {
        1 => 24.0,
        2 => 21.0,
        _ => 18.0,
    };
    // Estimated a little wide: viewers wrap labels differently.
    let wide = factor_minor(t) * 1.1;
    let pt = items
        .iter()
        .map(|s| fit_pt(std::slice::from_ref(s), label_w, label_budget, cap, 13.0, wide, 1.05, 0.0))
        .fold(cap, f64::min);
    let label_need = items
        .iter()
        .map(|s| stack_height(std::slice::from_ref(s), pt, label_w, wide, 1.05, 0.0))
        .max()
        .unwrap_or(0);
    let cell_h = (fixed + label_need).min(budget);
    let total = cell_h * nrows as i64 + row_gap * (nrows as i64 - 1);
    let top = vcentre(area, total);
    let al = |p: Para| if centred { p.align(Align::Center) } else { p };
    for (i, text) in items.iter().enumerate() {
        let (r, c) = (i / ncols, i % ncols);
        let col = cols[c];
        let y = top + r as i64 * (cell_h + row_gap);
        let accent = if cell == Cell::Open { 1 } else { 1 + (i as u8 % 4) };
        let numeral = if bracket { format!("[{:02}]", i + 1) } else { format!("{:02}", i + 1) };
        let (label_x, label_y) = match cell {
            Cell::Open => {
                let mut hair = Shape::deco(
                    format!("Agenda rule {}", i + 1),
                    Geom::Line,
                    R::new(col.x, y, cell_w, 0),
                    Fill::None,
                );
                hair.line = Some(if hdr == Header::Rail {
                    Line::new(Clr::accent(1), 1.0)
                } else {
                    Line::new(Clr::tx1().alpha(32), 1.0)
                });
                b.sp(hair, Role::Item(i));
                b.sp(
                    Shape::text_box(
                        format!("Agenda number {}", i + 1),
                        R::new(col.x, y + 150_000, cell_w, num_h),
                        Text::one(al(Para::from_runs(vec![Run::new(numeral)
                            .size(num_pt)
                            .bold()
                            .font(FontRef::Major)
                            .color(accent_text(t, accent, 3.0))])
                        .no_bullet())),
                    ),
                    Role::Item(i),
                );
                (col.x, y + 150_000 + num_h + 90_000)
            }
            Cell::Tile => {
                let tile = R::new(col.x, y, cell_w, cell_h);
                match sheet(t, &format!("Agenda sheet {}", i + 1), tile) {
                    Some(sh) => b.sp(sh, Role::Item(i)),
                    None => b.card(&format!("Agenda tile {}", i + 1), tile, Role::Item(i)),
                }
                b.sp(
                    Shape::text_box(
                        format!("Agenda number {}", i + 1),
                        R::new(col.x + pad, y + pad, cell_w - 2 * pad, num_h),
                        Text::one(al(Para::from_runs(vec![Run::new(numeral)
                            .size(num_pt)
                            .bold()
                            .font(FontRef::Major)
                            .color(accent_on_cards(t, accent, 3.0))])
                        .no_bullet())),
                    ),
                    Role::Item(i),
                );
                (col.x + pad, y + pad + num_h + 80_000)
            }
            Cell::Sticker => {
                let dx = if centred { (cell_w - disc_d) / 2 } else { 0 };
                let d_rect = R::new(col.x + dx, y, disc_d, disc_d);
                b.sp(
                    Shape::deco(
                        format!("Agenda disc shadow {}", i + 1),
                        Geom::Ellipse,
                        R::new(d_rect.x + 60_000, d_rect.y + 60_000, disc_d, disc_d),
                        Fill::Solid(Clr::tx1()),
                    ),
                    Role::Item(i),
                );
                let (fill, ink) = inked_fill(t, Clr::accent(accent), 100, 3.0);
                let mut disc = Shape::new(format!("Agenda number {}", i + 1), Geom::Ellipse, d_rect).fill(Fill::Solid(fill));
                disc.line = Some(Line::new(Clr::tx1(), 2.25));
                disc.decorative = true;
                disc.text = Some(
                    Text::one(
                        Para::from_runs(vec![Run::new(numeral)
                            .size((disc_d as f64 / EMU_PER_PT * 0.36).floor())
                            .bold()
                            .font(FontRef::Major)
                            .color(ink)])
                        .align(Align::Center)
                        .no_bullet(),
                    )
                    .anchor(Anchor::Middle),
                );
                b.sp(disc, Role::Item(i));
                (col.x, y + disc_d + 70_000 + 170_000)
            }
        };
        b.sp(
            Shape::text_box(
                format!("Agenda item {}", i + 1),
                R::new(label_x, label_y, label_w, (y + cell_h - label_y - pad).max(label_need)),
                Text::one(al(Para::from_runs(vec![Run::new(text.clone()).size(pt)]).no_bullet())),
            ),
            Role::Item(i),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::timeline::support::*;
    use super::*;
    use crate::deck::theme::THEMES;

    fn slide(n: usize) -> DesignedSlide {
        let all = [
            "Ship teams and sync",
            "Qualify more formulas against Excel",
            "Open the EMEA market",
            "Hire eight engineers",
            "Launch the marketplace",
            "Certify the on-device AI",
            "Raise the next round",
            "Review in the fall",
        ];
        let mut s = DesignedSlide::new("What we will do with the money").kicker("Plan");
        s.layout = Some(LayoutKind::Agenda);
        s.bullets = all.iter().take(n).map(|x| x.to_string()).collect();
        s
    }

    #[test]
    fn every_agenda_variant_builds_for_two_to_eight_items_in_every_header() {
        for base in THEMES {
            for style in [AgendaStyle::Rows, AgendaStyle::Grid] {
                for card in [CardStyle::Sticker, CardStyle::Bento, CardStyle::Outlined, CardStyle::Raised] {
                    for (h, th) in all_looks(base) {
                        let th = ThemeSpec {
                            look: Look { agenda: style, ..th.look },
                            card,
                            ..th
                        };
                        for n in 2..=8 {
                            let who = format!("{} {h:?} {style:?} {card:?} n={n}", base.id);
                            let built = agenda(&slide(n), &ctx(&th));
                            check_built(&th, &built, &who);
                            assert_reading_order(&built, n, &who);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn an_empty_agenda_still_builds() {
        let th = *crate::deck::theme::theme("aurora").unwrap();
        let mut s = slide(3);
        s.bullets.clear();
        check_built(&th, &agenda(&s, &ctx(&th)), "empty");
    }

    #[test]
    fn grids_balance_their_rows_and_never_squeeze_columns() {
        assert_eq!(grid_columns(4, 10_800_000, GUTTER), 4);
        assert_eq!(grid_columns(4, 6_800_000, GUTTER), 2, "a narrow area makes a 2 x 2");
        assert_eq!(grid_columns(6, 10_800_000, GUTTER), 3);
        assert_eq!(grid_columns(5, 10_800_000, GUTTER), 3);
        assert_eq!(grid_columns(8, 10_800_000, GUTTER), 4);
        assert_eq!(grid_columns(3, 10_800_000, GUTTER), 3);
        for n in 1..=8 {
            for w in [6_800_000, 8_000_000, 10_800_000] {
                let c = grid_columns(n, w, GUTTER);
                assert!((1..=4).contains(&c));
                assert!((w - GUTTER * (c as i64 - 1)) / c as i64 >= 1_500_000 || c == 1, "n={n} w={w} c={c}");
            }
        }
    }

    #[test]
    fn long_agendas_use_two_columns_only_when_there_is_room() {
        let wide = *crate::deck::theme::theme("harbor").unwrap(); // banner: full width
        let narrow = *crate::deck::theme::theme("ocean").unwrap(); // sidebar
        let xs = |t: &ThemeSpec| {
            let built = agenda(&slide(8), &ctx(t));
            let mut xs: Vec<i64> = built
                .items
                .iter()
                .filter_map(|i| match &i.el {
                    El::Sp(s) if s.name.starts_with("Agenda item") => s.rect.map(|r| r.x),
                    _ => None,
                })
                .collect();
            xs.sort();
            xs.dedup();
            xs.len()
        };
        assert_eq!(xs(&wide), 2);
        assert_eq!(xs(&narrow), 1);
    }

    #[test]
    fn numerals_read_in_every_theme_and_palette() {
        for base in THEMES {
            for pal in palettes() {
                for style in [AgendaStyle::Rows, AgendaStyle::Grid] {
                    for card in [CardStyle::Sticker, CardStyle::Bento, CardStyle::Outlined, CardStyle::Filled] {
                        let th = ThemeSpec {
                            look: Look { agenda: style, ..base.look },
                            card,
                            ..base.with_palette(pal)
                        };
                        let built = agenda(&slide(4), &ctx(&th));
                        let page = page_hex(&th);
                        let tile_bg = th.resolve(&Clr::bg2()).0;
                        for it in &built.items {
                            let El::Sp(sh) = &it.el else { continue };
                            if !sh.name.starts_with("Agenda number") {
                                continue;
                            }
                            let r = &sh.text.as_ref().unwrap().paras[0].runs[0];
                            let ink = r.color.unwrap();
                            let who = format!("{} in {} {style:?} {card:?}", base.id, pal.id);
                            match &sh.fill {
                                Fill::Solid(f) => {
                                    let c = contrast_of(&th, f, &ink);
                                    assert!(c >= 3.0 && r.bold == Some(true) && r.size.unwrap() >= 14.0, "{who}: chip {c:.2}");
                                }
                                _ => {
                                    // Open text: on the page, or on a tile (bg2).
                                    let on_tile = style == AgendaStyle::Grid && matches!(card, CardStyle::Bento | CardStyle::Raised | CardStyle::Filled);
                                    let mut c = contrast(&th.resolve(&ink).0, page);
                                    if on_tile {
                                        c = c.min(contrast(&th.resolve(&ink).0, &tile_bg));
                                    }
                                    assert!(c >= 3.0, "{who}: numeral {c:.2}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
