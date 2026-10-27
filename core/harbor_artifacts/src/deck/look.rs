//! Design languages.
//!
//! A theme used to be a palette plus a font; that makes fourteen themes
//! one design in fourteen colours. A *design language* is the part that
//! stays when you recolour: how a slide's header is composed, what a card
//! is made of, and which of several genuinely different arrangements a
//! layout takes (figures as tiles, as ruled rows, or as one hero number;
//! cards as a grid, as numbered columns or as stickers; a timeline as a
//! spine, as chevrons or as a list).
//!
//! `Look` names those choices; `Frame` is the geometry the header choice
//! implies (where the title, kicker, body and footer sit), so every layout
//! builder works inside `frame.body` instead of fixed page coordinates.
//! Palettes are separate (`palette.rs`) and can be swapped onto any look.

use super::grid::*;
use super::xml::{Align, Clr, R, SLIDE_H, SLIDE_W};

/// How a content slide's header (kicker + title + furniture) is composed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Header {
    /// Kicker over title at the top left; a small tab or ornament.
    TopLeft,
    /// A full-width colour band across the top carrying the title.
    Banner,
    /// A tall colour panel down one side carries kicker and title; the
    /// content sits beside it. `right` puts the panel on the other side.
    Sidebar { right: bool },
    /// A narrow left rail with the section label and a large slide
    /// number; title and content to its right, a hairline between.
    Rail,
    /// Everything centred under a small ornament.
    Centered,
    /// An oversized flush-left title over a heavy rule.
    Poster,
}

/// How figures (stat tiles) are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsStyle {
    /// A row of boxed tiles.
    Tiles,
    /// Ruled rows: huge figure on the left, its label on the right,
    /// a hairline between (an editorial table).
    Rows,
    /// The first figure large on one side, the others stacked beside it.
    Hero,
    /// Unboxed columns separated by thin vertical rules.
    Columns,
}

/// How cards (ideas, features, pillars) are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardsStyle {
    /// Boxed cards in a row/grid with a number badge.
    Grid,
    /// Numbered rows: large numeral, heading, text (no boxes).
    Rows,
    /// Unboxed columns separated by thin vertical rules.
    Columns,
    /// Cards slightly rotated like stickers, with hard outlines.
    Stickers,
    /// A bento grid: one large tile and smaller ones beside it.
    Bento,
}

/// How a timeline is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineStyle {
    /// A horizontal line with ring nodes.
    Spine,
    /// A row of chevron arrows, one per step (a process).
    Chevrons,
    /// A vertical list with dates in a left column and a rail.
    Vertical,
}

/// How a two-sided comparison is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareStyle {
    /// Two boxed panels with coloured headers and a VS badge.
    Panels,
    /// A table: criteria down the middle, each side a column of rules.
    Rows,
    /// Two full-height colour fields, text on colour.
    Fields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgendaStyle {
    /// Numbered rows with hairlines.
    Rows,
    /// A grid of large numerals with labels beneath.
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamStyle {
    /// Boxed cards with an initials avatar.
    Cards,
    /// Unboxed: large initials circle and text, side by side.
    Inline,
}

/// How a section divider is composed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionStyle {
    /// A large watermark numeral over the title.
    Numeral,
    /// Half the slide a colour field with the numeral, half the title.
    Split,
    /// A full-width colour band across the middle carrying the title.
    Band,
}

/// The cover composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverStyle {
    /// Full-bleed gradient, soft orbs, left-aligned title bottom-left.
    Aurora,
    /// A solid panel carries the title, geometric art fills the rest.
    Split,
    /// Oversized serif title on a plain field, a colour block at the edge.
    Editorial,
    /// Centred title inside a ring ornament; formal.
    Centered,
    /// Huge type filling the slide, tiny meta in the corners.
    Poster,
    /// A top colour band with the title, a wide field of art below.
    Band,
    /// A bento grid of tiles, one carrying the title.
    Bento,
}

/// Ornament in the master, behind everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deco {
    Bar,
    Orbs,
    Rule,
    Arcs,
    Grid,
    None,
}

/// What a card is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStyle {
    /// Flat tinted fill.
    Filled,
    /// Hairline outline, no fill.
    Outlined,
    /// White card, soft shadow, on a tinted page.
    Raised,
    /// Translucent fill with a hairline (dark themes).
    Glass,
    /// Thick outline plus a hard offset shadow (neo-brutalist stickers).
    Sticker,
    /// No box at all: content separated by hairline rules.
    Rule,
    /// Large soft tiles with no border (bento).
    Bento,
}

/// A complete design language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub header: Header,
    pub stats: StatsStyle,
    pub cards: CardsStyle,
    pub timeline: TimelineStyle,
    pub compare: CompareStyle,
    pub agenda: AgendaStyle,
    pub team: TeamStyle,
    pub section: SectionStyle,
}

impl Look {
    /// The original single design (glass cards, top-left header).
    pub const GLASS: Look = Look {
        header: Header::TopLeft,
        stats: StatsStyle::Tiles,
        cards: CardsStyle::Grid,
        timeline: TimelineStyle::Spine,
        compare: CompareStyle::Panels,
        agenda: AgendaStyle::Rows,
        team: TeamStyle::Cards,
        section: SectionStyle::Numeral,
    };
}

/// Where everything sits on a content slide for a header composition.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub kicker: R,
    pub title: R,
    /// The area layout builders place content in.
    pub body: R,
    pub ftr: R,
    pub num: R,
    /// Default title size in points and its vertical anchor.
    pub title_pt: f64,
    pub title_anchor: &'static str,
    pub title_align: Align,
    /// Ink for kicker, title and footer when they sit on a colour field
    /// (`None`: the page ink).
    pub on_field: bool,
    /// Width of the side panel (Sidebar/Rail), else 0.
    pub side_w: i64,
    pub side_right: bool,
}

const FOOT_Y: i64 = FOOTER_Y;

pub fn frame(look: &Look) -> Frame {
    let foot = |x: i64, w: i64| R::new(x, FOOT_Y, w, FOOTER_H);
    match look.header {
        Header::TopLeft => Frame {
            kicker: kicker_rect(),
            title: title_rect(),
            body: body_rect(),
            ftr: foot(MARGIN_X, 7_400_000),
            num: foot(SLIDE_W - MARGIN_X - 900_000, 900_000),
            title_pt: 34.0,
            title_anchor: "t",
            title_align: Align::Left,
            on_field: false,
            side_w: 0,
            side_right: false,
        },
        Header::Banner => Frame {
            kicker: R::new(MARGIN_X, 300_000, CONTENT_W, 250_000),
            title: R::new(MARGIN_X, 540_000, CONTENT_W, 820_000),
            body: R::new(MARGIN_X, 1_900_000, CONTENT_W, 4_280_000),
            ftr: foot(MARGIN_X, 7_400_000),
            num: foot(SLIDE_W - MARGIN_X - 900_000, 900_000),
            title_pt: 30.0,
            title_anchor: "t",
            title_align: Align::Left,
            on_field: true,
            side_w: 0,
            side_right: false,
        },
        Header::Sidebar { right } => {
            let side = 3_900_000;
            let (px, cx) = if right { (SLIDE_W - side, 0) } else { (0, side) };
            let content_x = cx + 700_000;
            let content_w = SLIDE_W - side - 700_000 - MARGIN_X + 200_000;
            Frame {
                kicker: R::new(px + 480_000, 640_000, side - 900_000, 260_000),
                title: R::new(px + 480_000, 1_050_000, side - 900_000, 3_700_000),
                body: R::new(content_x, 700_000, content_w, 5_480_000),
                ftr: foot(content_x, content_w - 900_000),
                num: R::new(px + 480_000, 5_950_000, 1_400_000, 520_000),
                title_pt: 28.0,
                title_anchor: "t",
                title_align: Align::Left,
                on_field: true,
                side_w: side,
                side_right: right,
            }
        }
        Header::Rail => {
            let rail = 2_700_000;
            let x = rail + 700_000;
            let w = SLIDE_W - x - MARGIN_X;
            Frame {
                kicker: R::new(MARGIN_X, 640_000, rail - MARGIN_X - 300_000, 520_000),
                title: R::new(x, 600_000, w, 1_100_000),
                body: R::new(x, 1_900_000, w, 4_280_000),
                ftr: foot(x, w - 900_000),
                num: R::new(MARGIN_X, 5_300_000, rail - MARGIN_X - 200_000, 900_000),
                title_pt: 32.0,
                title_anchor: "t",
                title_align: Align::Left,
                on_field: false,
                side_w: rail,
                side_right: false,
            }
        }
        Header::Centered => Frame {
            kicker: R::new(1_300_000, 420_000, SLIDE_W - 2_600_000, 250_000),
            title: R::new(1_300_000, 700_000, SLIDE_W - 2_600_000, 1_100_000),
            body: R::new(900_000, 2_050_000, SLIDE_W - 1_800_000, 4_130_000),
            ftr: foot(MARGIN_X, 7_400_000),
            num: foot(SLIDE_W - MARGIN_X - 900_000, 900_000),
            title_pt: 34.0,
            title_anchor: "t",
            title_align: Align::Center,
            on_field: false,
            side_w: 0,
            side_right: false,
        },
        Header::Poster => Frame {
            kicker: R::new(MARGIN_X, 330_000, CONTENT_W, 260_000),
            title: R::new(MARGIN_X, 600_000, CONTENT_W, 1_500_000),
            body: R::new(MARGIN_X, 2_500_000, CONTENT_W, 3_680_000),
            ftr: foot(MARGIN_X, 7_400_000),
            num: foot(SLIDE_W - MARGIN_X - 900_000, 900_000),
            title_pt: 46.0,
            title_anchor: "b",
            title_align: Align::Left,
            on_field: false,
            side_w: 0,
            side_right: false,
        },
    }
}

/// The full-height page used by tests to check content stays on it.
pub const PAGE: R = R {
    x: 0,
    y: 0,
    w: SLIDE_W,
    h: SLIDE_H,
};

/// Ink for text sitting on a header colour field.
pub fn field_ink() -> Clr {
    Clr::lt1()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_headers() -> Vec<Header> {
        vec![
            Header::TopLeft,
            Header::Banner,
            Header::Sidebar { right: false },
            Header::Sidebar { right: true },
            Header::Rail,
            Header::Centered,
            Header::Poster,
        ]
    }

    #[test]
    fn every_frame_keeps_its_regions_on_the_page_and_the_body_clear_of_the_title() {
        for h in all_headers() {
            let look = Look { header: h, ..Look::GLASS };
            let f = frame(&look);
            for (name, r) in [("kicker", f.kicker), ("title", f.title), ("body", f.body), ("ftr", f.ftr), ("num", f.num)] {
                assert!(
                    r.x >= 0 && r.y >= 0 && r.right() <= SLIDE_W && r.bottom() <= SLIDE_H && r.w > 0 && r.h > 0,
                    "{h:?} {name} {r:?}"
                );
            }
            // Body and title never overlap (side-by-side headers separate in x).
            let overlap_x = f.body.x < f.title.right() && f.title.x < f.body.right();
            let overlap_y = f.body.y < f.title.bottom() && f.title.y < f.body.bottom();
            assert!(!(overlap_x && overlap_y), "{h:?}: body overlaps the title");
            assert!(f.body.w >= 6_000_000 && f.body.h >= 3_400_000, "{h:?}: body too small {:?}", f.body);
        }
    }

    #[test]
    fn a_right_sidebar_mirrors_the_left_one() {
        let l = frame(&Look { header: Header::Sidebar { right: false }, ..Look::GLASS });
        let r = frame(&Look { header: Header::Sidebar { right: true }, ..Look::GLASS });
        assert!(l.body.x > l.title.x && r.body.x < r.title.x);
        assert_eq!(l.side_w, r.side_w);
    }
}
