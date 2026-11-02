//! Hero slides: covers, closings and section dividers.
//!
//! Seven cover compositions (`CoverStyle`) and three section compositions
//! (`SectionStyle`). Each is described ONCE as a [`Plan`] (backdrop, art,
//! text frames, inks); the master layout XML and the slide-level builders both
//! read the same plan, so a slide's placeholders always sit exactly where the
//! layout puts them and text inherits correctly.
//!
//! Colour is decided by what the text sits ON, not by the theme's name: a
//! colour field is an accent darkened or lightened until its ink reads
//! (`field`), a page is `bg1` with `tx1` ink. That is what lets any of the
//! twenty palettes go on any design.

use crate::deck::grid::*;
use crate::deck::layouts::kit::{factor_major, factor_minor, Built, Ctx, B};
use crate::deck::look::*;
use crate::deck::model::DesignedSlide;
use crate::deck::motion::Role;
use crate::deck::ornament::*;
use crate::deck::parts::{layout_shell, ph_body, ph_title, LayoutPart};
use crate::deck::theme::{contrast, luminance, Mode, ThemeSpec};
use crate::deck::xml::*;

const M: i64 = MARGIN_X;
const W: i64 = SLIDE_W;
const H: i64 = SLIDE_H;

// ---------------------------------------------------------------------------
// Colour: what a hero slide is painted on

/// A hero is dark when the deck is, or when a light theme inverts its
/// covers for drama (the layout then overrides the colour map).
pub(crate) fn hero_dark(t: &ThemeSpec) -> bool {
    t.is_dark() || t.inverse_hero
}

/// A cover that is a vivid colour gradient with light (or dark) type on it:
/// the Aurora composition on a light, non-inverted theme (sunrise, playground).
fn vivid(t: &ThemeSpec) -> bool {
    t.cover == CoverStyle::Aurora && !hero_dark(t)
}

/// A hero on a light page with dark ink: the page colour, not a colour field.
pub fn paper_hero(t: &ThemeSpec) -> bool {
    !hero_dark(t) && !vivid(t)
}

/// The theme as the hero layouts see it: the colour map `bg1`/`tx1` resolve
/// through on those layouts (inverted for dark and inverse heroes).
pub(crate) fn view(t: &ThemeSpec) -> ThemeSpec {
    ThemeSpec {
        mode: if hero_dark(t) {
            Mode::Dark
        } else {
            Mode::Light
        },
        ..*t
    }
}

fn hex_of(v: &ThemeSpec, c: &Clr) -> String {
    v.resolve(c).0
}

/// Text colour on the hero's page (or gradient) backdrop.
pub fn hero_ink(t: &ThemeSpec) -> Clr {
    if vivid(t) {
        vivid_parts(t).0
    } else {
        Clr::tx1()
    }
}

/// An accent made readable under `ink`: unchanged when it already reaches
/// `min` contrast, else darkened (light ink) or lightened (dark ink).
fn fit_fill(v: &ThemeSpec, base: Clr, ink: &str, min: f64) -> Clr {
    let hex = hex_of(v, &base);
    if contrast(&hex, ink) >= min {
        return base;
    }
    let darker: [(u32, u32); 8] = [
        (92, 0),
        (84, 0),
        (76, 0),
        (68, 0),
        (58, 0),
        (48, 0),
        (38, 0),
        (28, 0),
    ];
    let lighter: [(u32, u32); 8] = [
        (90, 10),
        (80, 20),
        (70, 30),
        (60, 40),
        (50, 50),
        (40, 60),
        (30, 70),
        (20, 80),
    ];
    let steps = if luminance(ink) > luminance(&hex) {
        darker
    } else {
        lighter
    };
    let mut last = base;
    for (m, o) in steps {
        last = base.lum(m, o);
        if contrast(&hex_of(v, &last), ink) >= min {
            break;
        }
    }
    last
}

/// A solid colour field in accent `n` and the ink that reads on it.
fn field(v: &ThemeSpec, n: u8, min: f64) -> (Clr, Clr) {
    let hex = v.palette.accents[(n as usize - 1) % 6];
    let (lt, dk) = (v.palette.lt1, v.palette.dk1);
    let (ink_hex, ink) = if contrast(hex, lt) >= contrast(hex, dk) {
        (lt, Clr::lt1())
    } else {
        (dk, Clr::dk1())
    };
    (fit_fill(v, Clr::accent(n), ink_hex, min), ink)
}

/// Accent `n` for a line or small mark on the page: itself when it shows
/// against the page, else the page ink.
fn mark(v: &ThemeSpec, n: u8) -> Clr {
    let hex = v.palette.accents[(n as usize - 1) % 6];
    let page = hex_of(v, &Clr::bg1());
    if contrast(hex, &page) >= 1.8 {
        Clr::accent(n)
    } else {
        Clr::tx1()
    }
}

/// Ink and gradient of a vivid cover (accent 1 to accent 6).
fn vivid_parts(t: &ThemeSpec) -> (Clr, Fill) {
    let (ink, _, fill) = vivid_full(t);
    (ink, fill)
}

/// `vivid_parts` plus the ink as hex (to fit further colours against).
fn vivid_full(t: &ThemeSpec) -> (Clr, &'static str, Fill) {
    let v = view(t);
    let (a1, a6) = (v.palette.accents[0], v.palette.accents[5]);
    let (lt, dk) = (v.palette.lt1, v.palette.dk1);
    let worst = |ink: &str| contrast(a1, ink).min(contrast(a6, ink));
    let (ink_hex, ink) = if worst(lt) >= 2.6 || worst(lt) >= worst(dk) {
        (lt, Clr::lt1())
    } else {
        (dk, Clr::dk1())
    };
    (
        ink,
        ink_hex,
        Fill::Linear {
            angle: 135,
            stops: vec![
                (0, fit_fill(&v, Clr::accent(1), ink_hex, 3.2)),
                (100, fit_fill(&v, Clr::accent(6), ink_hex, 3.2)),
            ],
        },
    )
}

/// Hero (cover / closing) backdrop.
pub fn hero_fill(t: &ThemeSpec) -> Fill {
    plan(t, LayoutPart::Cover).fill
}

// ---------------------------------------------------------------------------
// The plan

#[derive(Clone)]
pub(crate) struct Kick {
    pub rect: R,
    pub ink: Clr,
    pub pill: bool,
    pub align: Align,
}

#[derive(Clone)]
pub(crate) struct Meta {
    pub rect: R,
    pub ink: Clr,
    pub align: Align,
    /// Point size; small caps when 11, a name set in type when larger.
    pub pt: f64,
}

#[derive(Clone)]
pub(crate) struct Numeral {
    pub rect: R,
    pub ink: Clr,
    pub pt: f64,
    pub align: Align,
    pub anchor: Anchor,
}

/// Everything about one hero composition.
#[derive(Clone)]
pub(crate) struct Plan {
    pub fill: Fill,
    /// Art in the layout, drawn behind the slide's text, in z-order.
    pub deco: Vec<Shape>,
    pub kicker: Option<Kick>,
    pub title: R,
    pub title_anchor: Anchor,
    pub title_align: Align,
    pub title_ink: Clr,
    pub title_max: f64,
    pub title_min: f64,
    pub sub: R,
    pub sub_anchor: Anchor,
    pub sub_align: Align,
    pub sub_ink: Clr,
    pub sub_max: f64,
    pub sub_italic: bool,
    pub meta: Option<Meta>,
    pub numeral: Option<Numeral>,
    /// A small mark that leads the kicker (drawn only when there is one).
    pub kicker_mark: Option<(R, Clr)>,
}

fn plan_base(fill: Fill, ink: Clr) -> Plan {
    Plan {
        fill,
        deco: Vec::new(),
        kicker: None,
        title: R::new(M, 1_700_000, 6_700_000, 2_600_000),
        title_anchor: Anchor::Bottom,
        title_align: Align::Left,
        title_ink: ink,
        title_max: 52.0,
        title_min: 22.0,
        sub: R::new(M, 4_450_000, 6_300_000, 1_150_000),
        sub_anchor: Anchor::Top,
        sub_align: Align::Left,
        sub_ink: ink.alpha(88),
        sub_max: 22.0,
        sub_italic: false,
        meta: None,
        numeral: None,
        kicker_mark: None,
    }
}

/// The plan for a cover or closing slide.
pub(crate) fn plan(t: &ThemeSpec, _part: LayoutPart) -> Plan {
    match t.cover {
        CoverStyle::Aurora => plan_aurora(t),
        CoverStyle::Split => plan_split(t),
        CoverStyle::Editorial => plan_editorial(t),
        CoverStyle::Centered => plan_centered(t),
        CoverStyle::Poster => plan_poster(t),
        CoverStyle::Band => plan_band(t),
        CoverStyle::Bento => plan_bento(t),
    }
}

fn flat_page() -> Fill {
    Fill::Solid(Clr::bg1())
}

// --- Aurora: full-bleed light, soft orbs, title bottom-left ----------------

fn plan_aurora(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let (ink, fill, orbs) = if vivid(t) {
        // Orbs on a vivid gradient stay dark (or light) enough for the ink.
        let (ink, ink_hex, fill) = vivid_full(t);
        let o = |n: u8| fit_fill(&v, Clr::accent(n), ink_hex, 3.6);
        (ink, fill, [o(1), o(2), o(3)])
    } else {
        (
            Clr::tx1(),
            Fill::Linear {
                angle: 140,
                stops: vec![
                    (0, Clr::bg1()),
                    (62, Clr::bg2()),
                    (100, Clr::accent(1).lum(38, 0)),
                ],
            },
            [Clr::accent(1), Clr::accent(2), Clr::accent(3)],
        )
    };
    let mut p = plan_base(fill, ink);
    if vivid(t) {
        p.sub_ink = ink;
    }
    p.deco = vec![
        orb("Orb violet", 9_600_000, 1_300_000, 7_600_000, orbs[0], 62),
        orb("Orb cyan", 11_300_000, 5_600_000, 6_000_000, orbs[1], 48),
        orb("Orb pink", 1_200_000, 6_900_000, 6_400_000, orbs[2], 34),
        ring("Ring", 9_900_000, 3_400_000, 4_600_000, ink.alpha(18), 1.0),
        ring(
            "Ring outer",
            9_900_000,
            3_400_000,
            6_200_000,
            ink.alpha(10),
            1.0,
        ),
    ];
    if vivid(t) && t.card == CardStyle::Sticker {
        p.deco = sticker_art(t);
    }
    p.kicker = Some(Kick {
        rect: R::new(M, 620_000, 6_200_000, 340_000),
        ink,
        pill: true,
        align: Align::Left,
    });
    p.meta = Some(Meta {
        rect: R::new(M, 6_250_000, 6_000_000, 300_000),
        ink: if vivid(t) { ink } else { ink.alpha(78) },
        align: Align::Left,
        pt: 11.0,
    });
    p
}

/// Friendly-sticker art for the vivid Aurora cover on sticker designs: flat
/// shapes with a thick outline and a hard offset shadow instead of soft
/// orbs. A centred header (playground) gets confetti, otherwise a sun.
fn sticker_art(t: &ThemeSpec) -> Vec<Shape> {
    let edge = Clr::dk1();
    let sticker = |name: &str, geom: Geom, r: R, fill: Clr, rot: f64| -> [Shape; 2] {
        let back = Shape::deco(
            format!("{name} shadow"),
            geom.clone(),
            R::new(r.x + 100_000, r.y + 100_000, r.w, r.h),
            Fill::Solid(edge),
        )
        .rot(rot);
        let mut front = Shape::deco(name, geom, r, Fill::Solid(fill)).rot(rot);
        front.line = Some(Line::new(edge, 2.5));
        [back, front]
    };
    let mut d = Vec::new();
    if t.look.header == Header::Centered {
        d.extend(sticker(
            "Confetti disc",
            Geom::Ellipse,
            R::new(8_300_000, 700_000, 2_300_000, 2_300_000),
            Clr::accent(3),
            0.0,
        ));
        d.extend(sticker(
            "Confetti tile",
            Geom::Round(22),
            R::new(10_000_000, 3_100_000, 1_700_000, 1_700_000),
            Clr::accent(4),
            14.0,
        ));
        d.extend(sticker(
            "Confetti triangle",
            Geom::Triangle,
            R::new(7_600_000, 3_700_000, 1_800_000, 1_600_000),
            Clr::accent(2),
            -10.0,
        ));
        d.push(ring(
            "Confetti ring",
            11_300_000,
            1_300_000,
            900_000,
            edge,
            3.0,
        ));
    } else {
        d.push(ring(
            "Sun halo",
            9_650_000,
            2_900_000,
            4_200_000,
            edge.alpha(45),
            1.5,
        ));
        d.extend(sticker(
            "Sun",
            Geom::Ellipse,
            R::new(7_900_000, 1_200_000, 3_400_000, 3_400_000),
            Clr::accent(3),
            0.0,
        ));
        d.extend(sticker(
            "Sun cloud",
            Geom::Round(50),
            R::new(9_300_000, 4_700_000, 2_400_000, 1_000_000),
            Clr::accent(2),
            0.0,
        ));
        d.extend(sticker(
            "Sun spot",
            Geom::Ellipse,
            R::new(7_700_000, 5_200_000, 800_000, 800_000),
            Clr::accent(4),
            0.0,
        ));
    }
    d
}

// --- Split: a hard colour panel with its own motif, a structured text column

fn plan_split(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let mut p = plan_base(flat_page(), ink);
    let px = 7_200_000;
    let lt = v.palette.lt1;
    let a = fit_fill(&v, Clr::accent(1), lt, 3.0);
    let b = fit_fill(&v, Clr::accent(2), lt, 3.0);
    let mut d = vec![block(
        "Art panel",
        R::new(px, 0, W - px, H),
        Fill::Linear {
            angle: 135,
            stops: vec![(0, a), (100, b)],
        },
    )];
    let l = Clr::lt1();
    match t.deco {
        Deco::Grid => {
            for k in 1..=4 {
                d.push(vline(
                    &format!("Grid column {k}"),
                    7_300_000 + k * 1_000_000,
                    0,
                    H,
                    l.alpha(22),
                    0.75,
                ));
            }
            for k in 1..=6 {
                d.push(hairline(
                    &format!("Grid row {k}"),
                    px,
                    k * 1_000_000,
                    W - px,
                    l.alpha(22),
                    0.75,
                ));
            }
            d.push(ring(
                "Plot ring",
                10_300_000,
                3_000_000,
                2_000_000,
                l.alpha(75),
                1.75,
            ));
            d.push(disc(
                "Plot point",
                10_300_000,
                3_000_000,
                260_000,
                Fill::Solid(l.alpha(95)),
            ));
        }
        Deco::Bar => {
            let base_y = 5_750_000;
            let heights = [1_300_000i64, 2_000_000, 2_700_000, 2_200_000, 3_500_000];
            for (i, h) in heights.iter().enumerate() {
                let x = 8_000_000 + i as i64 * 800_000;
                let last = i == heights.len() - 1;
                d.push(block(
                    &format!("Bar {}", i + 1),
                    R::new(x, base_y - h, 540_000, *h),
                    Fill::Solid(l.alpha(if last { 92 } else { 30 })),
                ));
            }
            d.push(hairline(
                "Baseline",
                7_700_000,
                base_y,
                4_150_000,
                l.alpha(70),
                1.25,
            ));
            d.push(ring(
                "Marker",
                8_000_000 + 4 * 800_000 + 270_000,
                5_750_000 - 3_500_000 - 420_000,
                440_000,
                l.alpha(85),
                1.75,
            ));
        }
        _ => {
            for (i, dia) in [1_900_000i64, 3_200_000, 4_600_000, 6_000_000]
                .iter()
                .enumerate()
            {
                d.push(ring(
                    &format!("Art ring {}", i + 1),
                    10_400_000,
                    3_000_000,
                    *dia,
                    l.alpha(40 - i as u32 * 7),
                    1.25,
                ));
            }
            d.push(disc(
                "Art disc",
                10_400_000,
                3_000_000,
                1_000_000,
                Fill::Solid(l.alpha(90)),
            ));
        }
    }
    p.deco = d;
    // Quiet structure on the page side: a rule under the title, a rule above the footer.
    p.deco.push(hairline(
        "Title rule",
        M,
        4_560_000,
        5_900_000,
        ink.alpha(40),
        1.0,
    ));
    p.kicker_mark = Some((R::new(M, 652_000, 150_000, 150_000), mark(&v, 1)));
    p.kicker = Some(Kick {
        rect: R::new(M + 290_000, 560_000, 5_400_000, 340_000),
        ink,
        pill: false,
        align: Align::Left,
    });
    p.title = R::new(M, 1_150_000, 6_000_000, 3_200_000);
    p.title_max = 50.0;
    p.sub = R::new(M, 4_760_000, 5_800_000, 1_150_000);
    p.meta = Some(Meta {
        rect: R::new(M, 6_250_000, 5_800_000, 300_000),
        ink: ink.alpha(72),
        align: Align::Left,
        pt: 11.0,
    });
    p
}

// --- Editorial: paper, serif masthead, a colour band at the foot -------------

fn plan_editorial(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let (band, band_ink) = field(&v, 1, 4.5);
    let mut p = plan_base(flat_page(), ink);
    let band_y = 5_020_000;
    p.deco = vec![
        hairline("Masthead rule", M, 900_000, CONTENT_W, ink.alpha(85), 2.25),
        hairline(
            "Masthead hairline",
            M,
            960_000,
            CONTENT_W,
            ink.alpha(45),
            0.75,
        ),
        block(
            "Foot band",
            R::new(0, band_y, W, H - band_y),
            Fill::Solid(band),
        ),
        ring(
            "Band ring",
            W - 1_500_000,
            band_y + 1_000_000,
            2_300_000,
            band_ink.alpha(40),
            1.25,
        ),
        disc(
            "Band disc",
            W - 1_500_000,
            band_y + 1_000_000,
            420_000,
            Fill::Solid(band_ink.alpha(88)),
        ),
    ];
    p.kicker = Some(Kick {
        rect: R::new(M, 470_000, 6_400_000, 340_000),
        ink,
        pill: false,
        align: Align::Left,
    });
    p.meta = Some(Meta {
        rect: R::new(W - M - 4_200_000, 470_000, 4_200_000, 340_000),
        ink: ink.alpha(72),
        align: Align::Right,
        pt: 11.0,
    });
    p.title = R::new(M, 1_250_000, 9_700_000, 3_500_000);
    p.title_anchor = Anchor::Top;
    p.title_max = 66.0;
    p.title_min = 24.0;
    p.sub = R::new(M, band_y + 330_000, 7_600_000, 1_250_000);
    p.sub_anchor = Anchor::Middle;
    p.sub_ink = band_ink.alpha(94);
    p.sub_max = 24.0;
    p.sub_italic = true;
    p
}

// --- Centered: a formal, framed, symmetrical page -----------------------------

fn plan_centered(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let acc = mark(&v, 1);
    let mut p = plan_base(
        Fill::Radial {
            stops: vec![(0, Clr::bg2()), (100, Clr::bg1())],
        },
        ink,
    );
    let cx = W / 2;
    let mut outer = Shape::deco(
        "Frame",
        Geom::Rect,
        R::new(300_000, 300_000, W - 600_000, H - 600_000),
        Fill::None,
    );
    outer.line = Some(Line::new(acc.alpha(70), 1.25));
    let mut inner = Shape::deco(
        "Frame inner",
        Geom::Rect,
        R::new(390_000, 390_000, W - 780_000, H - 780_000),
        Fill::None,
    );
    inner.line = Some(Line::new(ink.alpha(22), 0.75));
    p.deco = vec![
        outer,
        inner,
        hairline(
            "Crest rule left",
            cx - 2_300_000,
            1_040_000,
            1_700_000,
            ink.alpha(45),
            1.0,
        ),
        hairline(
            "Crest rule right",
            cx + 600_000,
            1_040_000,
            1_700_000,
            ink.alpha(45),
            1.0,
        ),
        ring("Crest ring", cx, 1_040_000, 520_000, acc, 1.5),
        disc("Crest dot", cx, 1_040_000, 150_000, Fill::Solid(acc)),
        hairline(
            "Divider left",
            cx - 1_500_000,
            4_560_000,
            1_330_000,
            acc.alpha(80),
            1.0,
        ),
        hairline(
            "Divider right",
            cx + 170_000,
            4_560_000,
            1_330_000,
            acc.alpha(80),
            1.0,
        ),
        diamond(
            "Divider mark",
            cx,
            4_560_000,
            130_000,
            Fill::Solid(acc),
            None,
        ),
    ];
    p.kicker = Some(Kick {
        rect: R::new(1_500_000, 1_540_000, W - 3_000_000, 340_000),
        ink: ink.alpha(85),
        pill: false,
        align: Align::Center,
    });
    p.title = R::new(1_500_000, 2_000_000, W - 3_000_000, 2_250_000);
    p.title_anchor = Anchor::Middle;
    p.title_align = Align::Center;
    p.title_max = 50.0;
    p.sub = R::new(2_300_000, 4_800_000, W - 4_600_000, 950_000);
    p.sub_align = Align::Center;
    p.sub_max = 22.0;
    p.meta = Some(Meta {
        rect: R::new(2_000_000, 5_990_000, W - 4_000_000, 320_000),
        ink: ink.alpha(72),
        align: Align::Center,
        pt: 11.0,
    });
    p
}

// --- Poster: the type is the design ------------------------------------------

fn plan_poster(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let mut p = plan_base(flat_page(), ink);
    p.deco = vec![
        block(
            "Heavy rule",
            R::new(M, 930_000, CONTENT_W, 70_000),
            Fill::Solid(ink),
        ),
        hairline("Foot rule", M, 5_470_000, CONTENT_W, ink.alpha(55), 1.0),
        block(
            "Full stop",
            R::new(W - M - 360_000, 5_680_000, 360_000, 360_000),
            Fill::Solid(mark(&v, 1)),
        ),
    ];
    p.kicker = Some(Kick {
        rect: R::new(M, 450_000, 6_000_000, 340_000),
        ink,
        pill: false,
        align: Align::Left,
    });
    p.meta = Some(Meta {
        rect: R::new(W - M - 4_400_000, 450_000, 4_400_000, 340_000),
        ink: ink.alpha(72),
        align: Align::Right,
        pt: 11.0,
    });
    p.title = R::new(M, 1_130_000, CONTENT_W, 4_180_000);
    p.title_anchor = Anchor::Top;
    p.title_max = 120.0;
    p.title_min = 22.0;
    p.sub = R::new(M, 5_600_000, 8_800_000, 900_000);
    p.sub_anchor = Anchor::Top;
    p.sub_max = 20.0;
    p
}

// --- Band: a colour band across the top third carries the title ----------------

fn plan_band(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let (band, band_ink) = field(&v, 1, 4.5);
    let mut p = plan_base(flat_page(), ink);
    let band_h = 2_700_000;
    let mut d = vec![
        block("Title band", R::new(0, 0, W, band_h), Fill::Solid(band)),
        block(
            "Chart field",
            R::new(6_500_000, band_h, W - 6_500_000, H - band_h),
            Fill::Solid(Clr::bg2()),
        ),
    ];
    let base_y = 6_200_000;
    let heights = [
        700_000i64, 1_000_000, 1_350_000, 1_200_000, 1_750_000, 2_150_000, 2_700_000,
    ];
    for k in 1..=3 {
        d.push(hairline(
            &format!("Grid line {k}"),
            6_900_000,
            base_y - k * 1_000_000,
            4_530_000,
            ink.alpha(12),
            0.75,
        ));
    }
    for (i, h) in heights.iter().enumerate() {
        let last = i == heights.len() - 1;
        d.push(block(
            &format!("Bar {}", i + 1),
            R::new(7_250_000 + i as i64 * 620_000, base_y - h, 380_000, *h),
            Fill::Solid(if last { Clr::accent(2) } else { ink.alpha(14) }),
        ));
    }
    d.push(hairline(
        "Baseline",
        6_900_000,
        base_y,
        4_530_000,
        ink.alpha(45),
        1.0,
    ));
    d.push(block(
        "Band stub",
        R::new(M, band_h + 330_000, 700_000, 50_000),
        Fill::Solid(mark(&v, 2)),
    ));
    p.deco = d;
    p.kicker = Some(Kick {
        rect: R::new(M, 470_000, 8_000_000, 340_000),
        ink: band_ink,
        pill: false,
        align: Align::Left,
    });
    p.title = R::new(M, 860_000, CONTENT_W, 1_680_000);
    p.title_ink = band_ink;
    p.title_max = 46.0;
    p.title_min = 22.0;
    p.sub = R::new(M, band_h + 560_000, 5_300_000, 1_800_000);
    p.sub_max = 22.0;
    p.meta = Some(Meta {
        rect: R::new(M, 6_250_000, 5_100_000, 300_000),
        ink: ink.alpha(72),
        align: Align::Left,
        pt: 11.0,
    });
    p
}

// --- Bento: rounded tiles on a 12-column grid -----------------------------------

const BENTO_X: i64 = 457_200;
const BENTO_GAP: i64 = 137_160;

/// The rectangle spanning columns `c0..=c1` and rows `r0..=r1` (of 3) on the
/// 12-column bento grid.
fn bento_cell(c0: i64, c1: i64, r0: i64, r1: i64) -> R {
    let col_w = (W - 2 * BENTO_X - 11 * BENTO_GAP) / 12;
    let row_h = (H - 2 * BENTO_X - 2 * BENTO_GAP) / 3;
    R::new(
        BENTO_X + c0 * (col_w + BENTO_GAP),
        BENTO_X + r0 * (row_h + BENTO_GAP),
        (c1 - c0 + 1) * col_w + (c1 - c0) * BENTO_GAP,
        (r1 - r0 + 1) * row_h + (r1 - r0) * BENTO_GAP,
    )
}

fn plan_bento(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let rad = 230_000;
    let surface = Fill::Solid(Clr::bg2());
    let (hero_fill, hero_ink) = field(&v, 1, 4.0);
    let t_tile = bento_cell(0, 7, 0, 1);
    let a_tile = bento_cell(8, 11, 0, 0);
    let b_tile = bento_cell(8, 11, 1, 1);
    let s_tile = bento_cell(0, 4, 2, 2);
    let d_tile = bento_cell(5, 7, 2, 2);
    let n_tile = bento_cell(8, 11, 2, 2);
    let mut p = plan_base(flat_page(), ink);
    let mut d = vec![
        tile("Title tile", t_tile, rad, Fill::Solid(hero_fill)),
        tile("Ring tile", a_tile, rad, surface.clone()),
        tile(
            "Gradient tile",
            b_tile,
            rad,
            Fill::Linear {
                angle: 35,
                stops: vec![(0, Clr::accent(2)), (100, Clr::accent(3))],
            },
        ),
        tile("Subtitle tile", s_tile, rad, surface.clone()),
        tile("Dot tile", d_tile, rad, surface.clone()),
        tile("Name tile", n_tile, rad, surface),
    ];
    let (ax, ay) = (a_tile.x + a_tile.w / 2, a_tile.y + a_tile.h / 2);
    d.push(ring(
        "Tile ring outer",
        ax,
        ay,
        1_300_000,
        Clr::accent(2),
        2.0,
    ));
    d.push(ring(
        "Tile ring inner",
        ax,
        ay,
        640_000,
        Clr::accent(3),
        2.0,
    ));
    let (dx, dy) = (d_tile.x + d_tile.w / 2, d_tile.y + d_tile.h / 2);
    for (i, (ox, oy)) in [
        (-190_000i64, -190_000i64),
        (190_000, -190_000),
        (-190_000, 190_000),
        (190_000, 190_000),
    ]
    .iter()
    .enumerate()
    {
        let fill = if i == 0 || i == 3 {
            Clr::accent(1)
        } else {
            ink.alpha(35)
        };
        d.push(disc(
            &format!("Dot {}", i + 1),
            dx + ox,
            dy + oy,
            250_000,
            Fill::Solid(fill),
        ));
    }
    p.deco = d;
    let inset = 400_000;
    p.kicker = Some(Kick {
        rect: R::new(t_tile.x + inset, t_tile.y + 360_000, 5_600_000, 340_000),
        ink: hero_ink,
        pill: false,
        align: Align::Left,
    });
    p.title = R::new(
        t_tile.x + inset,
        t_tile.y + 900_000,
        t_tile.w - 2 * inset,
        t_tile.h - 900_000 - 420_000,
    );
    p.title_ink = hero_ink;
    p.title_max = 54.0;
    p.sub = R::new(
        s_tile.x + 320_000,
        s_tile.y + 200_000,
        s_tile.w - 640_000,
        s_tile.h - 400_000,
    );
    p.sub_anchor = Anchor::Middle;
    p.sub_max = 20.0;
    p.meta = Some(Meta {
        rect: R::new(
            n_tile.x + 320_000,
            n_tile.y + 200_000,
            n_tile.w - 640_000,
            n_tile.h - 400_000,
        ),
        ink: ink.alpha(88),
        align: Align::Left,
        pt: 20.0,
    });
    p
}

// ---------------------------------------------------------------------------
// Sections

/// The plan for a section divider, by the theme's `SectionStyle`.
pub(crate) fn section_plan(t: &ThemeSpec) -> Plan {
    match t.look.section {
        SectionStyle::Numeral => section_numeral(t),
        SectionStyle::Split => section_split(t),
        SectionStyle::Band => section_band(t),
    }
}

fn section_numeral(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let mut p = plan_base(
        Fill::Linear {
            angle: 150,
            stops: vec![(0, Clr::bg1()), (100, Clr::bg2())],
        },
        ink,
    );
    p.deco = vec![
        orb(
            "Glow",
            W - 1_000_000,
            H - 500_000,
            6_000_000,
            Clr::accent(1),
            26,
        ),
        block(
            "Section rule",
            R::new(M, 3_700_000, 900_000, 50_000),
            Fill::Solid(mark(&v, 2)),
        ),
    ];
    p.numeral = Some(Numeral {
        rect: R::new(M, 300_000, 7_400_000, 3_250_000),
        ink: ink.alpha(17),
        pt: 210.0,
        align: Align::Left,
        anchor: Anchor::Top,
    });
    p.title = R::new(M, 3_950_000, 8_800_000, 1_300_000);
    p.title_anchor = Anchor::Top;
    p.title_max = 52.0;
    p.sub = R::new(M, 5_350_000, 8_000_000, 1_000_000);
    p.sub_max = 20.0;
    p
}

fn section_split(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let (fld, fld_ink) = field(&v, 1, 4.5);
    let fw = 5_400_000;
    let mut p = plan_base(flat_page(), ink);
    p.deco = vec![
        block("Number field", R::new(0, 0, fw, H), Fill::Solid(fld)),
        ring("Field ring", 0, H, 3_000_000, fld_ink.alpha(36), 1.25),
        ring("Field ring outer", 0, H, 4_200_000, fld_ink.alpha(24), 1.0),
        block(
            "Section rule",
            R::new(fw + 800_000, 4_470_000, 900_000, 50_000),
            Fill::Solid(mark(&v, 1)),
        ),
    ];
    p.numeral = Some(Numeral {
        rect: R::new(M, 800_000, 3_600_000, 3_800_000),
        ink: fld_ink,
        pt: 190.0,
        align: Align::Left,
        anchor: Anchor::Middle,
    });
    let tx = fw + 800_000;
    p.title = R::new(tx, 1_300_000, W - tx - M, 2_950_000);
    p.title_max = 40.0;
    p.sub = R::new(tx, 4_700_000, W - tx - M, 1_300_000);
    p.sub_max = 20.0;
    p
}

fn section_band(t: &ThemeSpec) -> Plan {
    let v = view(t);
    let ink = Clr::tx1();
    let (band, band_ink) = field(&v, 1, 4.5);
    let (by, bh) = (2_150_000, 2_550_000);
    let mut p = plan_base(flat_page(), ink);
    let mut d = Vec::new();
    let mut band_shape = block("Section band", R::new(0, by, W, bh), Fill::Solid(band));
    if t.card == CardStyle::Sticker {
        // Sticker designs: a thick outline and a hard offset shadow.
        d.push(block(
            "Section band shadow",
            R::new(90_000, by + 90_000, W, bh),
            Fill::Solid(Clr::tx1()),
        ));
        band_shape.line = Some(Line::new(Clr::tx1(), 2.25));
    }
    d.push(band_shape);
    d.push(vline(
        "Number divider",
        2_950_000,
        by + 620_000,
        bh - 1_240_000,
        band_ink.alpha(55),
        1.0,
    ));
    p.deco = d;
    p.numeral = Some(Numeral {
        rect: R::new(M, by + 120_000, 2_000_000, bh - 240_000),
        ink: band_ink,
        pt: 60.0,
        align: Align::Left,
        anchor: Anchor::Middle,
    });
    p.title = R::new(3_300_000, by + 200_000, W - 3_300_000 - M, bh - 400_000);
    p.title_anchor = Anchor::Middle;
    p.title_ink = band_ink;
    p.title_max = 44.0;
    p.title_min = 22.0;
    p.sub = R::new(3_300_000, by + bh + 280_000, W - 3_300_000 - M, 1_150_000);
    p.sub_max = 20.0;
    p.meta = Some(Meta {
        rect: R::new(M, 1_300_000, 6_000_000, 340_000),
        ink: ink.alpha(72),
        align: Align::Left,
        pt: 11.0,
    });
    p
}

// ---------------------------------------------------------------------------
// Slide builders

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

/// Align a paragraph; left is the default and is left to the layout so an
/// RTL deck mirrors it.
fn aligned(p: Para, a: Align) -> Para {
    if a == Align::Left {
        p
    } else {
        p.align(a)
    }
}

/// How many characters of letter-spaced caps at `pt` fit `w_emu` on one line.
fn caps_chars(w_emu: i64, pt: f64, spacing_pt: f64) -> usize {
    ((w_emu as f64 / EMU_PER_PT) / (pt * 0.66 + spacing_pt))
        .floor()
        .max(4.0) as usize
}

fn kicker_shape(text: &str, k: &Kick) -> Shape {
    let text = ellipsize(text, 44.min(caps_chars(k.rect.w, 11.0, 1.6)));
    let mut rect = k.rect;
    let geom = if k.pill {
        // A pill hugs its label.
        rect.w = (text.chars().count() as i64 * 112_000 + 520_000).min(rect.w);
        Geom::Round(50)
    } else {
        Geom::Rect
    };
    let mut sh = Shape::new("Kicker", geom, rect);
    if k.pill {
        sh.line = Some(Line::new(k.ink.alpha(55), 1.0));
    } else {
        sh.text_box = true;
    }
    let para = Para::from_runs(vec![Run::new(text)
        .size(11.0)
        .bold()
        .caps()
        .spacing(1.6)
        .color(k.ink)])
    .no_bullet();
    let para = if k.pill {
        para.align(Align::Center)
    } else {
        aligned(para, k.align)
    };
    sh.text = Some(Text::one(para).anchor(Anchor::Middle));
    sh
}

fn meta_shape(deck: &str, m: &Meta) -> Shape {
    let fit = if m.pt > 12.0 {
        ((m.rect.w as f64 / EMU_PER_PT) / (m.pt * 0.62))
            .floor()
            .max(4.0) as usize
    } else {
        caps_chars(m.rect.w, m.pt, 1.2)
    };
    let run = Run::new(ellipsize(deck, fit.min(52)))
        .size(m.pt)
        .color(m.ink);
    let run = if m.pt > 12.0 {
        run.bold().font(FontRef::Major)
    } else {
        run.caps().spacing(1.2)
    };
    let para = Para::from_runs(vec![run]).no_bullet();
    Shape::text_box(
        "Deck name",
        m.rect,
        Text::one(aligned(para, m.align)).anchor(Anchor::Middle),
    )
}

/// Fitted title size inside the plan's frame (bold faces run wide, so the
/// estimate is padded).
fn title_pt(t: &ThemeSpec, text: &str, p: &Plan) -> f64 {
    let pad = if t.bold_titles { 1.06 } else { 1.0 };
    fit_pt(
        &[text.to_string()],
        (p.title.w as f64 * 0.97) as i64,
        (p.title.h as f64 * 0.95) as i64,
        p.title_max,
        p.title_min,
        factor_major(t) * pad,
        0.92,
        0.0,
    )
}

fn title_shape_for(cx: &Ctx, text: &str, p: &Plan, ty: &'static str) -> Shape {
    let pt = title_pt(cx.t, text, p);
    let para = aligned(
        Para::from_runs(vec![Run::new(text).size(pt)]).no_bullet(),
        p.title_align,
    );
    let mut s = Shape::placeholder(
        "Title",
        Ph { ty, idx: None },
        Text::one(para).anchor(p.title_anchor),
    );
    s.rect = Some(p.title);
    s
}

fn sub_lines(s: &DesignedSlide, max: usize) -> Vec<String> {
    s.bullets.iter().take(max).cloned().collect()
}

fn sub_pt(t: &ThemeSpec, lines: &[String], p: &Plan) -> f64 {
    fit_pt(
        lines,
        p.sub.w,
        (p.sub.h as f64 * 0.95) as i64,
        p.sub_max,
        13.0,
        factor_minor(t),
        1.1,
        6.0,
    )
}

fn sub_shape_for(cx: &Ctx, lines: &[String], p: &Plan, ty: &'static str) -> Shape {
    let pt = sub_pt(cx.t, lines, p);
    let paras: Vec<Para> = lines
        .iter()
        .map(|l| {
            let mut r = Run::new(l.clone()).size(pt);
            if p.sub_italic {
                r = r.italic().font(FontRef::Major);
            }
            aligned(Para::from_runs(vec![r]).no_bullet().after(6.0), p.sub_align)
        })
        .collect();
    let mut sub = Shape::placeholder(
        "Subtitle",
        Ph { ty, idx: Some(1) },
        Text::new(paras).anchor(p.sub_anchor),
    );
    sub.rect = Some(p.sub);
    sub
}

fn numeral_shape(n: &Numeral, text: &str) -> Shape {
    let para = aligned(
        Para::from_runs(vec![Run::new(text)
            .size(n.pt)
            .bold()
            .font(FontRef::Major)
            .color(n.ink)])
        .no_bullet(),
        n.align,
    );
    Shape::text_box("Section number", n.rect, Text::one(para).anchor(n.anchor))
}

pub(crate) fn hero(s: &DesignedSlide, cx: &Ctx, part: LayoutPart) -> Built {
    let p = plan(cx.t, part);
    let mut b = B::new(cx);
    if let (Some(k), Some(plan_k)) = (&s.kicker, &p.kicker) {
        if let Some((r, c)) = p.kicker_mark {
            b.sp(block("Kicker mark", r, Fill::Solid(c)), Role::Kicker);
        }
        b.sp(kicker_shape(k, plan_k), Role::Kicker);
    }
    b.sp(title_shape_for(cx, &s.title, &p, "ctrTitle"), Role::Title);
    let lines = sub_lines(s, 3);
    if !lines.is_empty() {
        b.sp(sub_shape_for(cx, &lines, &p, "subTitle"), Role::Subtitle);
    }
    if let Some(m) = &p.meta {
        if !cx.deck_title.is_empty() && cx.deck_title != s.title {
            b.sp(meta_shape(cx.deck_title, m), Role::Footer);
        }
    }
    b.finish(part)
}

pub(crate) fn section(s: &DesignedSlide, cx: &Ctx) -> Built {
    let p = section_plan(cx.t);
    let mut b = B::new(cx);
    if let Some(n) = &p.numeral {
        b.sp(
            numeral_shape(n, &format!("{:02}", cx.section_no.max(1))),
            Role::Kicker,
        );
    }
    if let Some(m) = &p.meta {
        if !cx.deck_title.is_empty() && cx.deck_title != s.title {
            b.sp(meta_shape(cx.deck_title, m), Role::Footer);
        }
    }
    b.sp(title_shape_for(cx, &s.title, &p, "title"), Role::Title);
    let lines = sub_lines(s, 1);
    if !lines.is_empty() {
        b.sp(sub_shape_for(cx, &lines, &p, "body"), Role::Subtitle);
    }
    b.finish(LayoutPart::Section)
}

// ---------------------------------------------------------------------------
// Master layouts

fn algn(a: Align, rtl: bool) -> &'static str {
    match (a, rtl) {
        (Align::Center, _) => "ctr",
        (Align::Left, false) | (Align::Right, true) => "l",
        _ => "r",
    }
}

fn anchor_str(a: Anchor) -> &'static str {
    match a {
        Anchor::Top => "t",
        Anchor::Middle => "ctr",
        Anchor::Bottom => "b",
    }
}

/// The layout XML for a plan: backdrop, art, and the two placeholders whose
/// text styles the slide's runs inherit.
fn layout_from_plan(
    t: &ThemeSpec,
    p: &Plan,
    rtl: bool,
    names: (&str, &str),
    phs: (&str, &str),
    prompts: (&str, &str),
) -> String {
    let (name, ty) = names;
    let (title_ph, sub_ph) = phs;
    let (tr, sr) = if rtl {
        (p.title.mirrored(), p.sub.mirrored())
    } else {
        (p.title, p.sub)
    };
    let mut shapes = sp_list(&p.deco, 10, rtl);
    let title_lst = format!(
        "<a:lstStyle><a:lvl1pPr algn=\"{}\" rtl=\"{}\"><a:lnSpc><a:spcPct val=\"90000\"/></a:lnSpc><a:buNone/><a:defRPr sz=\"{}\" b=\"{}\"><a:solidFill>{}</a:solidFill><a:latin typeface=\"+mj-lt\"/><a:cs typeface=\"+mj-cs\"/></a:defRPr></a:lvl1pPr></a:lstStyle>",
        algn(p.title_align, rtl),
        if rtl { 1 } else { 0 },
        (p.title_max * 100.0) as i64,
        if t.bold_titles { 1 } else { 0 },
        p.title_ink.xml(),
    );
    let italic = if p.sub_italic { " i=\"1\"" } else { "" };
    let sub_lst = format!(
        "<a:lstStyle><a:lvl1pPr marL=\"0\" indent=\"0\" algn=\"{}\" rtl=\"{}\"><a:buNone/><a:defRPr sz=\"{}\"{italic}><a:solidFill>{}</a:solidFill></a:defRPr></a:lvl1pPr></a:lstStyle>",
        algn(p.sub_align, rtl),
        if rtl { 1 } else { 0 },
        (p.sub_max * 100.0) as i64,
        p.sub_ink.xml(),
    );
    shapes.push_str(&ph_title(
        2,
        title_ph,
        "Title 1",
        tr,
        &title_lst,
        anchor_str(p.title_anchor),
        prompts.0,
    ));
    let sub_idx = 1;
    shapes.push_str(&ph_body(
        3,
        sub_ph,
        sub_idx,
        "Subtitle 2",
        sr,
        &sub_lst,
        anchor_str(p.sub_anchor),
        prompts.1,
    ));
    let inverse = t.inverse_hero && !t.is_dark();
    layout_shell(name, ty, false, Some(&p.fill), inverse, &shapes)
}

pub(crate) fn hero_layout(t: &ThemeSpec, part: LayoutPart, rtl: bool) -> String {
    let name = if part == LayoutPart::Closing {
        "Closing"
    } else {
        "Cover"
    };
    let p = plan(t, part);
    layout_from_plan(
        t,
        &p,
        rtl,
        (name, "title"),
        ("ctrTitle", "subTitle"),
        (
            "Click to edit Master title style",
            "Click to edit Master subtitle style",
        ),
    )
}

pub(crate) fn section_layout(t: &ThemeSpec, rtl: bool) -> String {
    let p = section_plan(t);
    layout_from_plan(
        t,
        &p,
        rtl,
        ("Section Header", "secHead"),
        ("title", "body"),
        (
            "Click to edit Master title style",
            "Click to edit Master text styles",
        ),
    )
}

// ---------------------------------------------------------------------------
// Tests: every composition x every palette

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::layouts::kit::El;
    use crate::deck::palette::PALETTES;
    use crate::deck::theme::{hex_rgb, theme, THEMES};

    const COVERS: [CoverStyle; 7] = [
        CoverStyle::Aurora,
        CoverStyle::Split,
        CoverStyle::Editorial,
        CoverStyle::Centered,
        CoverStyle::Poster,
        CoverStyle::Band,
        CoverStyle::Bento,
    ];
    const SECTIONS: [SectionStyle; 3] = [
        SectionStyle::Numeral,
        SectionStyle::Split,
        SectionStyle::Band,
    ];

    /// Every theme x palette x cover x section x inverse/plain hero.
    fn variants() -> Vec<(String, ThemeSpec)> {
        let mut out = Vec::new();
        for base in THEMES {
            for pal in PALETTES {
                for inverse in [false, true] {
                    for (ci, cover) in COVERS.iter().enumerate() {
                        let sec = SECTIONS[ci % 3];
                        let mut t = base.with_palette(pal);
                        t.cover = *cover;
                        t.inverse_hero = inverse;
                        t.look.section = sec;
                        out.push((
                            format!(
                                "{} in {} {cover:?}/{sec:?} inverse={inverse}",
                                base.id, pal.id
                            ),
                            t,
                        ));
                    }
                    // Every section style under every theme, whatever its cover.
                    for sec in SECTIONS {
                        let mut t = base.with_palette(pal);
                        t.inverse_hero = inverse;
                        t.look.section = sec;
                        out.push((
                            format!("{} in {} {sec:?} inverse={inverse}", base.id, pal.id),
                            t,
                        ));
                    }
                }
            }
        }
        out
    }

    fn ctx<'a>(t: &'a ThemeSpec, deck: &'a str) -> Ctx<'a> {
        Ctx {
            t,
            frame: frame(&t.look),
            rtl: false,
            footer: String::new(),
            numbers: false,
            index: 0,
            deck_title: deck,
            section_no: 3,
        }
    }

    // ---- paint model: what colour is behind a text frame ----------------------

    fn mix(top: &str, a: f64, behind: &str) -> String {
        let (x, y) = (hex_rgb(top), hex_rgb(behind));
        let c = |i: usize| ((x[i] * a + y[i] * (1.0 - a)) * 255.0).round() as u8;
        format!("{:02X}{:02X}{:02X}", c(0), c(1), c(2))
    }

    /// (colour hex, opacity) of each stop of a fill; radial fades count at 60%.
    fn stops_of(v: &ThemeSpec, fill: &Fill) -> Vec<(String, f64)> {
        let one = |c: &Clr, k: f64| {
            let (hex, a) = v.resolve(c);
            (hex, a * k)
        };
        match fill {
            Fill::None => vec![],
            Fill::Solid(c) => vec![one(c, 1.0)],
            Fill::Linear { stops, .. } => stops.iter().map(|(_, c)| one(c, 1.0)).collect(),
            Fill::Radial { stops } => stops.iter().take(1).map(|(_, c)| one(c, 0.6)).collect(),
        }
    }

    fn within(outer: &R, inner: &R) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right()
            && inner.bottom() <= outer.bottom()
    }

    fn overlaps(a: &R, b: &R) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    /// (touches, covers) of a filled shape against a text frame. Circles are
    /// tested as circles, so an orb whose bounding box clips a corner does not
    /// count.
    fn reach(sh: &Shape, spot: &R) -> (bool, bool) {
        let Some(r) = sh.rect else {
            return (false, false);
        };
        if sh.geom == Geom::Ellipse && r.w == r.h {
            let (cx, cy, rad) = (
                (r.x + r.w / 2) as f64,
                (r.y + r.h / 2) as f64,
                r.w as f64 / 2.0,
            );
            let nx = cx.clamp(spot.x as f64, spot.right() as f64);
            let ny = cy.clamp(spot.y as f64, spot.bottom() as f64);
            let touches = ((nx - cx).powi(2) + (ny - cy).powi(2)).sqrt() < rad;
            let corners = [
                (spot.x, spot.y),
                (spot.right(), spot.y),
                (spot.x, spot.bottom()),
                (spot.right(), spot.bottom()),
            ];
            let covers = corners
                .iter()
                .all(|(x, y)| ((*x as f64 - cx).powi(2) + (*y as f64 - cy).powi(2)).sqrt() <= rad);
            (touches, covers)
        } else {
            (overlaps(&r, spot), within(&r, spot))
        }
    }

    /// Every colour the frame can sit on, worst cases included.
    fn backdrops(v: &ThemeSpec, p: &Plan, spot: &R) -> Vec<String> {
        let mut cands: Vec<String> = stops_of(v, &p.fill).into_iter().map(|(h, _)| h).collect();
        for sh in &p.deco {
            if matches!(sh.fill, Fill::None) || sh.geom == Geom::Line {
                continue;
            }
            let (touches, covers) = reach(sh, spot);
            if !touches {
                continue;
            }
            let mut next = Vec::new();
            for (hex, a) in stops_of(v, &sh.fill) {
                for b in &cands {
                    next.push(mix(&hex, a, b));
                }
            }
            if covers {
                cands = next;
            } else {
                cands.extend(next);
            }
        }
        cands
    }

    /// The frames that carry real text, with their ink.
    fn spots(p: &Plan) -> Vec<(&'static str, R, Clr)> {
        let mut v = vec![
            ("title", p.title, p.title_ink),
            ("subtitle", p.sub, p.sub_ink),
        ];
        if let Some(k) = &p.kicker {
            v.push(("kicker", k.rect, k.ink));
        }
        if let Some(m) = &p.meta {
            v.push(("deck name", m.rect, m.ink));
        }
        if let Some(n) = &p.numeral {
            // A watermark numeral is texture, not text.
            if n.ink.alpha.unwrap_or(100) >= 50 {
                v.push(("numeral", n.rect, n.ink));
            }
        }
        v
    }

    fn check_contrast(who: &str, t: &ThemeSpec, p: &Plan, fails: &mut Vec<String>) {
        let v = view(t);
        for (name, rect, ink) in spots(p) {
            let (ink_hex, a) = v.resolve(&ink);
            for bd in backdrops(&v, p, &rect) {
                let seen = mix(&ink_hex, a, &bd);
                let c = contrast(&seen, &bd);
                if c < 3.0 {
                    fails.push(format!("{who}: {name} ink {seen} on {bd}: {c:.2}"));
                }
            }
        }
    }

    fn report(fails: Vec<String>) {
        let mut f = fails;
        f.sort();
        f.dedup();
        assert!(
            f.is_empty(),
            "{} failure(s); first few:\n{}",
            f.len(),
            f.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
        );
    }

    #[test]
    fn hero_text_reads_on_its_backdrop_under_every_palette() {
        let mut fails = Vec::new();
        for (who, t) in variants() {
            for part in [LayoutPart::Cover, LayoutPart::Closing] {
                check_contrast(&who, &t, &plan(&t, part), &mut fails);
            }
            check_contrast(&who, &t, &section_plan(&t), &mut fails);
        }
        report(fails);
    }

    // ---- geometry ----------------------------------------------------------------

    fn outline_crosses(sh: &Shape, spot: &R) -> bool {
        let Some(r) = sh.rect else { return false };
        match sh.geom {
            Geom::Line => {
                // A horizontal or vertical rule through the frame.
                let seg = R::new(r.x, r.y, r.w.max(1), r.h.max(1));
                overlaps(&seg, spot)
            }
            Geom::Ellipse if r.w == r.h => {
                let (cx, cy, rad) = (
                    (r.x + r.w / 2) as f64,
                    (r.y + r.h / 2) as f64,
                    r.w as f64 / 2.0,
                );
                let nx = cx.clamp(spot.x as f64, spot.right() as f64);
                let ny = cy.clamp(spot.y as f64, spot.bottom() as f64);
                let near = ((nx - cx).powi(2) + (ny - cy).powi(2)).sqrt();
                let far = [
                    (spot.x, spot.y),
                    (spot.right(), spot.y),
                    (spot.x, spot.bottom()),
                    (spot.right(), spot.bottom()),
                ]
                .iter()
                .map(|(x, y)| ((*x as f64 - cx).powi(2) + (*y as f64 - cy).powi(2)).sqrt())
                .fold(0.0, f64::max);
                near <= rad && rad <= far
            }
            _ => overlaps(&r, spot) && !within(&r, spot) && !within(spot, &r),
        }
    }

    fn check_geometry(who: &str, p: &Plan, fails: &mut Vec<String>) {
        let page = R::new(0, 0, W, H);
        let sp = spots(p);
        let mut all: Vec<(&str, R)> = sp.iter().map(|(n, r, _)| (*n, *r)).collect();
        if let Some(n) = &p.numeral {
            if !all.iter().any(|(name, _)| *name == "numeral") {
                all.push(("numeral", n.rect));
            }
        }
        for (name, r) in &all {
            if !within(&page, r) || r.w <= 0 || r.h <= 0 {
                fails.push(format!("{who}: {name} frame off the page {r:?}"));
            }
        }
        for (i, (a, ra)) in all.iter().enumerate() {
            for (b, rb) in &all[i + 1..] {
                if overlaps(ra, rb) {
                    fails.push(format!("{who}: text frames {a} and {b} overlap"));
                }
            }
        }
        for sh in &p.deco {
            let Some(r) = sh.rect else { continue };
            if r.w < 0 || r.h < 0 {
                fails.push(format!("{who}: {} negative size", sh.name));
            }
            if !sh.decorative {
                fails.push(format!(
                    "{who}: layout art {} is not marked decorative",
                    sh.name
                ));
            }
            // Outlines and rules may not run through text unless they are ghosts.
            let ghost = sh
                .line
                .as_ref()
                .is_some_and(|l| l.clr.alpha.is_some_and(|a| a <= 20));
            if !ghost && (matches!(sh.fill, Fill::None) || sh.geom == Geom::Line) {
                for (name, spot) in &all {
                    if outline_crosses(sh, spot) {
                        fails.push(format!("{who}: {} runs through the {name} frame", sh.name));
                    }
                }
            }
        }
        // Inside the page, a frame may not sit half on a solid block it is not
        // fully inside (text straddling a colour edge reads badly).
        for (name, spot) in &all {
            for sh in &p.deco {
                if matches!(sh.fill, Fill::None)
                    || sh.geom == Geom::Line
                    || sh.geom == Geom::Ellipse
                {
                    continue;
                }
                let Some(r) = sh.rect else { continue };
                let solid = matches!(&sh.fill, Fill::Solid(c) if c.alpha.is_none_or(|a| a >= 60));
                if solid
                    && overlaps(&r, spot)
                    && !within(&r, spot)
                    && r.w > 200_000
                    && r.h > 200_000
                {
                    fails.push(format!(
                        "{who}: {name} frame straddles the edge of {}",
                        sh.name
                    ));
                }
            }
        }
    }

    #[test]
    fn hero_frames_stay_on_the_page_apart_and_clear_of_outlines() {
        let mut fails = Vec::new();
        for (who, t) in variants() {
            check_geometry(&who, &plan(&t, LayoutPart::Cover), &mut fails);
            check_geometry(&who, &section_plan(&t), &mut fails);
        }
        report(fails);
    }

    // ---- fitting --------------------------------------------------------------------

    fn long_title() -> String {
        "Why a quarter of regional teams moved their client files off the cloud suite and onto one local-first workspace this year"
            .to_string()
    }

    fn arabic_title() -> String {
        "لماذا انتقل ربع الفرق الإقليمية بملفات عملائها من الحزمة السحابية إلى مساحة عمل واحدة تعمل محلياً أولاً هذا العام".to_string()
    }

    fn fits(t: &ThemeSpec, text: &str, p: &Plan) -> bool {
        let pt = title_pt(t, text, p);
        let pad = if t.bold_titles { 1.06 } else { 1.0 };
        stack_height(
            &[text.to_string()],
            pt,
            (p.title.w as f64 * 0.97) as i64,
            factor_major(t) * pad,
            0.92,
            0.0,
        ) <= p.title.h
    }

    #[test]
    fn titles_of_ninety_plus_characters_still_fit_every_frame() {
        let mut fails = Vec::new();
        for (who, t) in variants() {
            for text in [long_title(), arabic_title()] {
                assert!(text.chars().count() >= 90);
                for (label, p) in [
                    ("cover", plan(&t, LayoutPart::Cover)),
                    ("section", section_plan(&t)),
                ] {
                    if !fits(&t, &text, &p) {
                        fails.push(format!(
                            "{who}: {label} title of {} chars overflows its frame",
                            text.chars().count()
                        ));
                    }
                }
            }
        }
        report(fails);
    }

    fn sub_fits(t: &ThemeSpec, lines: &[String], p: &Plan) -> bool {
        let pt = sub_pt(t, lines, p);
        stack_height(lines, pt, p.sub.w, factor_minor(t), 1.1, 6.0) <= p.sub.h
    }

    /// Estimated height a non-placeholder text box needs (caps run wide).
    fn box_need(sh: &Shape) -> (i64, i64) {
        let text = sh.text.as_ref().unwrap();
        let rect = sh.rect.unwrap();
        let mut need = 0.0;
        for p in &text.paras {
            let s: String = p.runs.iter().map(|r| r.text.as_str()).collect();
            let r0 = &p.runs[0];
            let pt = r0.size.unwrap_or(18.0);
            let per_char = if r0.caps {
                pt * 0.66 + r0.spacing.unwrap_or(0.0)
            } else {
                pt * 0.6
            };
            let per_line = ((rect.w as f64 / EMU_PER_PT) / per_char).floor().max(1.0) as usize;
            let lines = s.chars().count().div_ceil(per_line).max(1) as f64;
            need += lines * pt * 1.2;
        }
        ((need * EMU_PER_PT) as i64, rect.h)
    }

    #[test]
    fn subtitles_kickers_and_names_fit_their_boxes() {
        let subs: Vec<String> = [
            "Series A funding round, October 2026, strictly confidential",
            "hello@harbor.dev",
            "harbor.dev",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let long_kicker =
            "An unusually long kicker label that nobody should have written but somebody will"
                .to_string();
        let long_deck =
            "A deck whose title runs to a great many characters because the author never edits"
                .to_string();
        let mut fails = Vec::new();
        for (who, t) in variants() {
            for (label, p) in [
                ("cover", plan(&t, LayoutPart::Cover)),
                ("section", section_plan(&t)),
            ] {
                if !sub_fits(&t, &subs, &p) {
                    fails.push(format!("{who}: {label} subtitle overflows"));
                }
            }
            let mut s = DesignedSlide::new("Short title").kicker(long_kicker.clone());
            s.bullets = subs.clone();
            let cx = ctx(&t, &long_deck);
            for built in [hero(&s, &cx, LayoutPart::Cover), section(&s, &cx)] {
                for it in &built.items {
                    if let El::Sp(sh) = &it.el {
                        if sh.ph.is_some() || sh.text.is_none() {
                            continue;
                        }
                        let (need, have) = box_need(sh);
                        // A numeral is one line at its own size and may use the whole box.
                        if need as f64 > have as f64 * 1.05 + 20_000.0 {
                            fails.push(format!("{who}: '{}' needs {need} but has {have}", sh.name));
                        }
                    }
                }
            }
        }
        report(fails);
    }

    // ---- structure ---------------------------------------------------------------------

    #[test]
    fn placeholders_on_slides_match_the_layouts_and_roles_are_set() {
        for base in THEMES {
            let t = *base;
            let cx = ctx(&t, "Deck");
            let mut s = DesignedSlide::new("A title").kicker("Kicker");
            s.bullets = vec!["Sub".into()];
            for part in [LayoutPart::Cover, LayoutPart::Closing] {
                let layout = hero_layout(&t, part, false);
                assert!(
                    layout.contains("type=\"ctrTitle\"")
                        && layout.contains("type=\"subTitle\" idx=\"1\""),
                    "{}",
                    t.id
                );
                let built = hero(&s, &cx, part);
                let mut seen = (false, false);
                for it in &built.items {
                    let El::Sp(sh) = &it.el else { continue };
                    match sh.name.as_str() {
                        "Title" => {
                            assert_eq!(
                                sh.ph,
                                Some(Ph {
                                    ty: "ctrTitle",
                                    idx: None
                                })
                            );
                            assert_eq!(it.role, Role::Title);
                            seen.0 = true;
                        }
                        "Subtitle" => {
                            assert_eq!(
                                sh.ph,
                                Some(Ph {
                                    ty: "subTitle",
                                    idx: Some(1)
                                })
                            );
                            assert_eq!(it.role, Role::Subtitle);
                            seen.1 = true;
                        }
                        "Kicker" | "Kicker mark" => assert_eq!(it.role, Role::Kicker),
                        "Deck name" => assert_eq!(it.role, Role::Footer),
                        other => panic!("{}: unexpected hero shape {other}", t.id),
                    }
                }
                assert!(seen.0 && seen.1, "{}", t.id);
            }
            let layout = section_layout(&t, false);
            assert!(
                layout.contains("type=\"title\"") && layout.contains("type=\"body\" idx=\"1\""),
                "{}",
                t.id
            );
            let built = section(&s, &cx);
            for it in &built.items {
                let El::Sp(sh) = &it.el else { continue };
                match sh.name.as_str() {
                    "Title" => assert_eq!(
                        (sh.ph.clone(), it.role),
                        (
                            Some(Ph {
                                ty: "title",
                                idx: None
                            }),
                            Role::Title
                        )
                    ),
                    "Subtitle" => assert_eq!(
                        (sh.ph.clone(), it.role),
                        (
                            Some(Ph {
                                ty: "body",
                                idx: Some(1)
                            }),
                            Role::Subtitle
                        )
                    ),
                    "Section number" => assert_eq!(it.role, Role::Kicker),
                    "Deck name" => assert_eq!(it.role, Role::Footer),
                    other => panic!("{}: unexpected section shape {other}", t.id),
                }
            }
        }
    }

    #[test]
    fn shape_budget_and_well_formed_layouts_in_both_directions() {
        for (who, t) in variants().into_iter().step_by(7) {
            let cx = ctx(&t, "Deck");
            let mut s = DesignedSlide::new("A title").kicker("Kicker");
            s.bullets = vec!["One".into(), "Two".into()];
            for (label, p, built) in [
                (
                    "cover",
                    plan(&t, LayoutPart::Cover),
                    hero(&s, &cx, LayoutPart::Cover),
                ),
                ("section", section_plan(&t), section(&s, &cx)),
            ] {
                let n = p.deco.len() + built.items.len();
                assert!(n <= 25, "{who}: {label} has {n} shapes");
            }
            for rtl in [false, true] {
                for part in [LayoutPart::Cover, LayoutPart::Closing] {
                    roxmltree::Document::parse(&hero_layout(&t, part, rtl))
                        .unwrap_or_else(|e| panic!("{who} {part:?} rtl={rtl}: {e}"));
                }
                roxmltree::Document::parse(&section_layout(&t, rtl))
                    .unwrap_or_else(|e| panic!("{who} section rtl={rtl}: {e}"));
            }
            assert_eq!(
                hero_layout(&t, LayoutPart::Cover, false),
                hero_layout(&t, LayoutPart::Cover, false),
                "deterministic"
            );
        }
    }

    /// Where a plan puts its weight: the title's cell on a 4x3 grid, how much
    /// of the page is a hard-edged solid, and the title's reading shape.
    fn fingerprint(p: &Plan, t: &ThemeSpec) -> (Vec<bool>, (i64, i64), i64, bool, bool) {
        let mut mask = vec![false; 12 * 6];
        for sh in &p.deco {
            let solid = matches!(&sh.fill, Fill::Solid(c) if c.alpha.is_none_or(|a| a >= 40))
                || matches!(&sh.fill, Fill::Linear { .. });
            let Some(r) = sh.rect else { continue };
            if !solid || sh.geom == Geom::Line || sh.geom == Geom::Ellipse {
                continue;
            }
            for cy in 0..6i64 {
                for cx in 0..12i64 {
                    let cell = R::new(cx * W / 12, cy * H / 6, W / 12, H / 6);
                    let ov_w = (r.right().min(cell.right()) - r.x.max(cell.x)).max(0);
                    let ov_h = (r.bottom().min(cell.bottom()) - r.y.max(cell.y)).max(0);
                    if ov_w * ov_h * 2 > cell.w * cell.h {
                        mask[(cy * 12 + cx) as usize] = true;
                    }
                }
            }
        }
        let _ = t;
        let c = (p.title.x + p.title.w / 2, p.title.y + p.title.h / 2);
        (
            mask,
            (c.0 * 4 / W, c.1 * 3 / H),
            p.title.w * 3 / CONTENT_W,
            p.title_align == Align::Center,
            p.title_anchor == Anchor::Top,
        )
    }

    #[test]
    fn the_seven_covers_and_three_sections_are_different_compositions() {
        let base = theme("aurora").unwrap();
        let dist = |a: &(Vec<bool>, (i64, i64), i64, bool, bool),
                    b: &(Vec<bool>, (i64, i64), i64, bool, bool)| {
            let m = a.0.iter().zip(&b.0).filter(|(x, y)| x != y).count();
            m + 6
                * ((a.1 != b.1) as usize
                    + (a.2 != b.2) as usize
                    + (a.3 != b.3) as usize
                    + (a.4 != b.4) as usize)
        };
        let covers: Vec<_> = COVERS
            .iter()
            .map(|c| {
                let mut t = *base;
                t.cover = *c;
                (*c, fingerprint(&plan(&t, LayoutPart::Cover), &t))
            })
            .collect();
        for (i, (a, fa)) in covers.iter().enumerate() {
            for (b, fb) in &covers[i + 1..] {
                assert!(
                    dist(fa, fb) >= 8,
                    "{a:?} and {b:?} are too alike ({})",
                    dist(fa, fb)
                );
            }
        }
        let sections: Vec<_> = SECTIONS
            .iter()
            .map(|s| {
                let mut t = *base;
                t.look.section = *s;
                (*s, fingerprint(&section_plan(&t), &t))
            })
            .collect();
        for (i, (a, fa)) in sections.iter().enumerate() {
            for (b, fb) in &sections[i + 1..] {
                assert!(
                    dist(fa, fb) >= 8,
                    "{a:?} and {b:?} are too alike ({})",
                    dist(fa, fb)
                );
            }
        }
    }

    #[test]
    fn the_poster_title_is_the_design() {
        let t = *theme("mono").unwrap();
        let p = plan(&t, LayoutPart::Cover);
        let title = "Harbor: office work that stays on your device";
        let pt = title_pt(&t, title, &p);
        assert!(pt >= 80.0, "poster title only {pt} pt");
        // And it takes most of the slide's height.
        let used = stack_height(
            &[title.to_string()],
            pt,
            (p.title.w as f64 * 0.97) as i64,
            factor_major(&t) * 1.06,
            0.92,
            0.0,
        );
        assert!(used as f64 > H as f64 * 0.45, "poster title block {used}");
        assert!(p.deco.len() <= 4, "a poster carries almost no ornament");
    }

    #[test]
    fn the_bento_cover_keeps_to_twelve_art_shapes_and_the_band_cover_to_a_third() {
        let mut t = *theme("obsidian").unwrap();
        t.cover = CoverStyle::Bento;
        assert!(plan(&t, LayoutPart::Cover).deco.len() <= 12);
        t.cover = CoverStyle::Band;
        let p = plan(&t, LayoutPart::Cover);
        let band = p
            .deco
            .iter()
            .find(|s| s.name == "Title band")
            .unwrap()
            .rect
            .unwrap();
        assert!(band.w == W && band.y == 0 && band.h >= H / 3 - 300_000 && band.h <= H * 2 / 5);
        assert!(p.title.bottom() <= band.bottom());
    }
}
