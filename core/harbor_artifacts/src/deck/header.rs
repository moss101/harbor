//! Header furniture: what the master draws behind a content slide for each
//! header composition — a colour band, a side panel, a rail, a heavy rule,
//! a small ornament — plus the theme's background ornament (orbs, arcs,
//! grid) underneath. Slides themselves carry content only.

use super::grid::*;
use super::look::*;
use super::ornament::*;
use super::theme::ThemeSpec;
use super::xml::*;

/// The colour of a header field (band or side panel): a deep tone of the
/// palette, readable under light ink.
pub(crate) fn field_fill(t: &ThemeSpec) -> Clr {
    if t.is_dark() {
        Clr::bg2()
    } else {
        Clr::dk2()
    }
}

/// Ornament behind everything, independent of the header composition.
fn background_ornament(t: &ThemeSpec) -> Vec<Shape> {
    let mut out = Vec::new();
    match t.deco {
        Deco::Orbs => {
            out.push(orb("Glow", SLIDE_W - 600_000, 200_000, 5_200_000, Clr::accent(1), 14));
            out.push(orb("Glow 2", 300_000, SLIDE_H + 200_000, 4_400_000, Clr::accent(2), 10));
        }
        Deco::Arcs => {
            for (i, d) in [3_000_000i64, 4_400_000, 5_800_000].iter().enumerate() {
                out.push(ring(
                    &format!("Arc {}", i + 1),
                    SLIDE_W,
                    SLIDE_H,
                    *d,
                    Clr::accent(1).alpha(if i == 0 { 26 } else { 13 }),
                    1.0,
                ));
            }
        }
        Deco::Grid => {
            let step = CONTENT_W / 12;
            for i in 0..=12 {
                out.push(vline(
                    &format!("Grid v{i}"),
                    MARGIN_X + i * step,
                    0,
                    SLIDE_H,
                    Clr::tx1().alpha(5),
                    0.5,
                ));
            }
            for j in 0..=6 {
                out.push(hairline(
                    &format!("Grid h{j}"),
                    0,
                    j * SLIDE_H / 6,
                    SLIDE_W,
                    Clr::tx1().alpha(5),
                    0.5,
                ));
            }
        }
        // Bar and Rule are header marks (below); None draws nothing.
        Deco::Bar | Deco::Rule | Deco::None => {}
    }
    out
}

/// The header composition's own furniture.
fn furniture(t: &ThemeSpec, f: &Frame) -> Vec<Shape> {
    let mut v = Vec::new();
    match t.look.header {
        Header::TopLeft => match t.deco {
            Deco::Bar => v.push(Shape::deco(
                "Header tab",
                Geom::Rect,
                R::new(0, KICKER_Y, 150_000, TITLE_Y + TITLE_H - KICKER_Y),
                Fill::Solid(Clr::accent(1)),
            )),
            Deco::Rule => {
                v.push(hairline(
                    "Header rule",
                    MARGIN_X,
                    BODY_Y - 120_000,
                    CONTENT_W,
                    Clr::tx1().alpha(22),
                    0.75,
                ));
                v.push(Shape::deco(
                    "Rule mark",
                    Geom::Rect,
                    R::new(MARGIN_X, BODY_Y - 120_000 - 22_000, 640_000, 44_000),
                    Fill::Solid(Clr::accent(1)),
                ));
            }
            _ => {}
        },
        Header::Banner => {
            let band_h = 1_500_000;
            v.push(Shape::deco(
                "Header band",
                Geom::Rect,
                R::new(0, 0, SLIDE_W, band_h),
                Fill::Solid(field_fill(t)),
            ));
            v.push(Shape::deco(
                "Band accent",
                Geom::Rect,
                R::new(0, band_h, SLIDE_W, 54_000),
                Fill::Solid(Clr::accent(1)),
            ));
        }
        Header::Sidebar { right } => {
            let x = if right { SLIDE_W - f.side_w } else { 0 };
            v.push(Shape::deco(
                "Side panel",
                Geom::Rect,
                R::new(x, 0, f.side_w, SLIDE_H),
                Fill::Solid(field_fill(t)),
            ));
            let edge = if right { x } else { x + f.side_w - 54_000 };
            v.push(Shape::deco(
                "Panel accent",
                Geom::Rect,
                R::new(edge, 0, 54_000, SLIDE_H),
                Fill::Solid(Clr::accent(1)),
            ));
            // A short rule between the kicker and the title.
            v.push(Shape::deco(
                "Panel rule",
                Geom::Rect,
                R::new(f.kicker.x, f.kicker.bottom() + 70_000, 520_000, 36_000),
                Fill::Solid(Clr::accent(2)),
            ));
        }
        Header::Rail => {
            let x = f.side_w + 230_000;
            v.push(vline("Rail rule", x, 520_000, SLIDE_H - 1_100_000, Clr::tx1().alpha(24), 0.75));
            v.push(Shape::deco(
                "Rail mark",
                Geom::Rect,
                R::new(x - 18_000, 520_000, 36_000, 720_000),
                Fill::Solid(Clr::accent(1)),
            ));
        }
        Header::Centered => {
            let cx = SLIDE_W / 2;
            v.push(hairline("Header rule", cx - 520_000, 1_930_000, 1_040_000, Clr::accent(1), 2.0));
            v.push(Shape::deco(
                "Header dot",
                Geom::Ellipse,
                R::new(cx - 55_000, 1_930_000 - 55_000, 110_000, 110_000),
                Fill::Solid(Clr::accent(1)),
            ));
        }
        Header::Poster => {
            v.push(Shape::deco(
                "Heavy rule",
                Geom::Rect,
                R::new(MARGIN_X, f.title.bottom() + 100_000, CONTENT_W, 64_000),
                Fill::Solid(Clr::tx1()),
            ));
            v.push(Shape::deco(
                "Rule accent",
                Geom::Rect,
                R::new(MARGIN_X, f.title.bottom() + 100_000, 1_900_000, 64_000),
                Fill::Solid(Clr::accent(1)),
            ));
        }
    }
    v
}

/// Everything the master draws behind content, back to front.
pub(crate) fn master_deco(t: &ThemeSpec, f: &Frame) -> Vec<Shape> {
    let mut v = background_ornament(t);
    v.extend(furniture(t, f));
    v
}

#[cfg(test)]
mod tests {
    use super::super::theme::THEMES;
    use super::*;

    #[test]
    fn every_header_draws_something_and_stays_on_the_page() {
        for t in THEMES {
            let f = frame(&t.look);
            let shapes = master_deco(t, &f);
            for s in &shapes {
                let r = s.rect.unwrap();
                assert!(r.w >= 0 && r.h >= 0, "{} {}", t.id, s.name);
            }
            if !matches!(t.look.header, Header::TopLeft) {
                assert!(!shapes.is_empty(), "{}: a {:?} header must draw its furniture", t.id, t.look.header);
            }
        }
    }
}
