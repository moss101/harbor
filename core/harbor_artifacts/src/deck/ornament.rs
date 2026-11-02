//! Small decorative shapes shared by masters, covers and layouts.

use super::xml::*;

pub(crate) fn orb(name: &str, cx: i64, cy: i64, d: i64, c: Clr, a: u32) -> Shape {
    Shape::deco(
        name,
        Geom::Ellipse,
        R::new(cx - d / 2, cy - d / 2, d, d),
        Fill::Radial {
            stops: vec![(0, c.alpha(a)), (100, c.alpha(0))],
        },
    )
}

pub(crate) fn ring(name: &str, cx: i64, cy: i64, d: i64, c: Clr, w_pt: f64) -> Shape {
    let mut s = Shape::deco(
        name,
        Geom::Ellipse,
        R::new(cx - d / 2, cy - d / 2, d, d),
        Fill::None,
    );
    s.line = Some(Line::new(c, w_pt));
    s
}

pub(crate) fn hairline(name: &str, x: i64, y: i64, w: i64, c: Clr, w_pt: f64) -> Shape {
    let mut s = Shape::deco(name, Geom::Line, R::new(x, y, w, 0), Fill::None);
    s.line = Some(Line::new(c, w_pt));
    s
}

pub(crate) fn vline(name: &str, x: i64, y: i64, h: i64, c: Clr, w_pt: f64) -> Shape {
    let mut s = Shape::deco(name, Geom::Line, R::new(x, y, 0, h), Fill::None);
    s.line = Some(Line::new(c, w_pt));
    s
}

pub(crate) fn sp_list(shapes: &[Shape], first_id: u32, rtl: bool) -> String {
    shapes
        .iter()
        .enumerate()
        .map(|(i, s)| s.xml(first_id + i as u32, rtl))
        .collect()
}

/// A filled disc centred on (`cx`, `cy`).
pub(crate) fn disc(name: &str, cx: i64, cy: i64, d: i64, fill: Fill) -> Shape {
    Shape::deco(
        name,
        Geom::Ellipse,
        R::new(cx - d / 2, cy - d / 2, d, d),
        fill,
    )
}

/// A flat rectangle.
pub(crate) fn block(name: &str, r: R, fill: Fill) -> Shape {
    Shape::deco(name, Geom::Rect, r, fill)
}

/// A rounded tile whose corner radius is `radius` EMU whatever its size.
pub(crate) fn tile(name: &str, r: R, radius: i64, fill: Fill) -> Shape {
    let short = r.w.min(r.h).max(1);
    let pct = ((radius * 100) / short).clamp(0, 50) as u32;
    Shape::deco(name, Geom::Round(pct), r, fill)
}

/// A small square turned 45 degrees (a diamond), centred on (`cx`, `cy`).
pub(crate) fn diamond(
    name: &str,
    cx: i64,
    cy: i64,
    d: i64,
    fill: Fill,
    outline: Option<Line>,
) -> Shape {
    let mut s = Shape::deco(name, Geom::Rect, R::new(cx - d / 2, cy - d / 2, d, d), fill).rot(45.0);
    s.line = outline;
    s
}
