//! Shared tokens, card chrome, header/footer pieces and fit helpers for
//! the layout builders.

#![allow(unused_imports)]
use crate::deck::grid::*;
use crate::deck::look::*;
use crate::deck::model::{split_stat, Block, DesignedSlide, LayoutKind};
use crate::deck::motion::Role;
use crate::deck::parts::LayoutPart;
use crate::deck::theme::{contrast, ThemeSpec};
use crate::deck::xml::*;
use crate::pptx::{ChartSpec, SlideImage};

pub struct Ctx<'a> {
    pub t: &'a ThemeSpec,
    /// Where this theme's header composition puts things.
    pub frame: Frame,
    pub rtl: bool,
    pub footer: String,
    pub numbers: bool,
    pub index: usize,
    pub deck_title: &'a str,
    /// 1-based count of section dividers so far (for the numeral).
    pub section_no: usize,
}

pub enum El {
    Sp(Shape),
    Pic {
        name: String,
        alt: String,
        rect: R,
        geom: Geom,
        image: SlideImage,
    },
    Chart {
        name: String,
        alt: String,
        rect: R,
        spec: ChartSpec,
    },
}

pub struct Item {
    pub el: El,
    pub role: Role,
}

pub struct Built {
    pub part: LayoutPart,
    pub items: Vec<Item>,
}

pub(crate) struct B<'a> {
    pub(crate) cx: &'a Ctx<'a>,
    pub(crate) items: Vec<Item>,
}

impl<'a> B<'a> {
    pub(crate) fn new(cx: &'a Ctx<'a>) -> Self {
        B {
            cx,
            items: Vec::new(),
        }
    }
    pub(crate) fn sp(&mut self, s: Shape, role: Role) {
        self.items.push(Item {
            el: El::Sp(s),
            role,
        });
    }
    /// A card's chrome, all shapes tagged with the same motion role.
    pub(crate) fn card(&mut self, name: &str, rect: R, role: Role) {
        for shape in chrome(self.cx.t, name, rect) {
            self.items.push(Item {
                el: El::Sp(shape),
                role,
            });
        }
    }
    pub(crate) fn finish(self, part: LayoutPart) -> Built {
        Built {
            part,
            items: self.items,
        }
    }
}

pub(crate) fn page_hex(t: &ThemeSpec) -> &'static str {
    let (page, _) = t.page_and_ink();
    if t.tinted_page {
        match t.mode {
            crate::deck::theme::Mode::Light => t.palette.lt2,
            crate::deck::theme::Mode::Dark => t.palette.dk2,
        }
    } else {
        page
    }
}

/// Accent `n` (1-based) as TEXT colour on the content page: the accent
/// itself when it reads (>= `min` contrast), otherwise the ink colour.
pub(crate) fn accent_text(t: &ThemeSpec, n: u8, min: f64) -> Clr {
    let hex = t.palette.accents[(n as usize - 1) % 6];
    if contrast(hex, page_hex(t)) >= min {
        Clr::accent(n)
    } else {
        Clr::tx1()
    }
}

/// Text colour that reads on a solid accent `n` fill.
pub fn on_accent(t: &ThemeSpec, n: u8) -> Clr {
    let hex = t.palette.accents[(n as usize - 1) % 6];
    if contrast(hex, t.palette.lt1) >= contrast(hex, t.palette.dk1) {
        Clr::lt1()
    } else {
        Clr::dk1()
    }
}

pub(crate) fn muted(a: u32) -> Clr {
    Clr::tx1().alpha(a)
}

pub(crate) fn radius(t: &ThemeSpec) -> Geom {
    if t.radius == 0 {
        Geom::Rect
    } else {
        Geom::Round(t.radius)
    }
}

pub(crate) fn factor_major(t: &ThemeSpec) -> f64 {
    glyph_factor(t.fonts.major)
}

pub(crate) fn factor_minor(t: &ThemeSpec) -> f64 {
    glyph_factor(t.fonts.minor)
}

/// The shapes that make one card: the box (and, for stickers, its hard
/// offset shadow; for rule cards, just a hairline).
pub(crate) fn chrome(t: &ThemeSpec, name: &str, rect: R) -> Vec<Shape> {
    let round = |pct: u32| {
        if t.radius == 0 {
            Geom::Rect
        } else {
            Geom::Round(pct)
        }
    };
    match t.card {
        CardStyle::Filled => vec![Shape::deco(name, radius(t), rect, Fill::Solid(Clr::bg2()))],
        CardStyle::Outlined => {
            // Opaque on the page colour: a background grid must not run through text.
            let mut s = Shape::deco(name, radius(t), rect, Fill::Solid(Clr::bg1()));
            s.line = Some(Line::new(Clr::tx1().alpha(28), 1.0));
            vec![s]
        }
        CardStyle::Raised => {
            let mut s = Shape::deco(name, radius(t), rect, Fill::Solid(Clr::bg1()));
            s.shadow = Some(Shadow::SOFT);
            vec![s]
        }
        CardStyle::Glass => {
            let mut s = Shape::deco(name, radius(t), rect, Fill::Solid(Clr::tx1().alpha(7)));
            s.line = Some(Line::new(Clr::tx1().alpha(16), 0.75));
            vec![s]
        }
        CardStyle::Sticker => {
            // A hard offset shadow behind a thick-outlined card.
            let off = 80_000;
            let back = Shape::deco(
                format!("{name} shadow"),
                round(t.radius.min(14)),
                R::new(rect.x + off, rect.y + off, rect.w, rect.h),
                Fill::Solid(Clr::tx1()),
            );
            let mut front =
                Shape::deco(name, round(t.radius.min(14)), rect, Fill::Solid(Clr::bg1()));
            front.line = Some(Line::new(Clr::tx1(), 2.25));
            vec![back, front]
        }
        CardStyle::Rule => {
            let mut s = Shape::deco(
                name,
                Geom::Line,
                R::new(rect.x, rect.y, rect.w, 0),
                Fill::None,
            );
            s.line = Some(Line::new(Clr::tx1().alpha(55), 1.0));
            vec![s]
        }
        CardStyle::Bento => vec![Shape::deco(
            name,
            Geom::Round(t.radius.max(14)),
            rect,
            Fill::Solid(if t.is_dark() {
                Clr::tx1().alpha(8)
            } else {
                Clr::bg2()
            }),
        )],
    }
}

pub(crate) fn badge(t: &ThemeSpec, name: &str, rect: R, accent: u8, label: &str, pt: f64) -> Shape {
    let mut s = Shape::new(name, Geom::Ellipse, rect).fill(Fill::Solid(Clr::accent(accent)));
    s.decorative = true;
    s.text = Some(
        Text::one(
            Para::from_runs(vec![Run::new(label)
                .size(pt)
                .bold()
                .color(on_accent(t, accent))
                .font(FontRef::Major)])
            .align(Align::Center)
            .no_bullet(),
        )
        .anchor(Anchor::Middle),
    );
    s
}

pub(crate) fn title_shape(
    cx: &Ctx,
    text: &str,
    rect: Option<R>,
    max_pt: f64,
    min_pt: f64,
    hero: bool,
) -> Shape {
    let area = rect.unwrap_or(cx.frame.title);
    let pt = fit_pt(
        &[text.to_string()],
        area.w,
        area.h,
        max_pt,
        min_pt,
        factor_major(cx.t),
        0.92,
        0.0,
    );
    let mut run = Run::new(text);
    if pt < max_pt - 0.01 || rect.is_some() {
        run = run.size(pt);
    }
    // The layout's title ink is for the colour field (band, side panel); a
    // title placed on the page by explicit rect needs the page ink.
    if rect.is_some() && !hero && cx.frame.on_field {
        run = run.color(Clr::tx1());
    }
    let ty = if hero { "ctrTitle" } else { "title" };
    // The layout supplies the header's alignment and anchor; a title placed
    // by explicit rect (hero, statement, split) states its own.
    let mut para = Para::from_runs(vec![run]).no_bullet();
    if !hero && rect.is_none() && cx.frame.title_align == Align::Center {
        para = para.align(Align::Center);
    }
    let mut s = Shape::placeholder("Title", Ph { ty, idx: None }, Text::one(para));
    if let Some(r) = rect {
        s.rect = Some(r);
    }
    s
}

pub(crate) fn kicker_shape(cx: &Ctx, text: &str) -> Shape {
    // On a colour field (banner, side panel) the kicker is light; elsewhere
    // it is the accent when that reads, else the ink.
    let clr = if cx.frame.on_field {
        Clr::lt1().alpha(78)
    } else {
        accent_text(cx.t, 1, 4.5)
    };
    let mut para = Para::from_runs(vec![Run::new(text)
        .size(12.0)
        .bold()
        .caps()
        .spacing(1.6)
        .color(clr)])
    .no_bullet();
    if cx.frame.title_align == Align::Center {
        para = para.align(Align::Center);
    }
    Shape::text_box(
        "Kicker",
        cx.frame.kicker,
        Text::one(para).anchor(if matches!(cx.t.look.header, Header::Rail) {
            Anchor::Top
        } else {
            Anchor::Bottom
        }),
    )
}

pub(crate) fn footer_shapes(cx: &Ctx) -> Vec<Shape> {
    let mut v = Vec::new();
    if !cx.footer.is_empty() {
        v.push(Shape::placeholder(
            "Footer",
            Ph {
                ty: "ftr",
                idx: Some(11),
            },
            Text::one(Para::from_runs(vec![Run::new(cx.footer.clone())]).no_bullet()),
        ));
    }
    if cx.numbers {
        v.push(Shape::placeholder(
            "Slide Number",
            Ph {
                ty: "sldNum",
                idx: Some(12),
            },
            Text::one(Para::from_runs(vec![Run::slide_number()]).no_bullet()),
        ));
    }
    v
}

pub(crate) fn bullets_paras(
    cx: &Ctx,
    bullets: &[String],
    rect: R,
    max_pt: f64,
) -> (Vec<Para>, f64) {
    let w = rect.w - 320_040;
    let pt = fit_pt(
        bullets,
        w,
        rect.h,
        max_pt,
        14.0,
        factor_minor(cx.t),
        1.04,
        12.0,
    );
    let paras = bullets
        .iter()
        .map(|b| {
            let mut r = Run::new(b.clone());
            if pt < 22.0 - 0.01 || max_pt != 22.0 {
                r = r.size(pt);
            }
            Para::from_runs(vec![r])
        })
        .collect();
    (paras, pt)
}

pub(crate) fn standard_header(b: &mut B, s: &DesignedSlide) {
    if let Some(k) = &s.kicker {
        b.sp(kicker_shape(b.cx, k), Role::Kicker);
    }
    b.sp(
        title_shape(b.cx, &s.title, None, b.cx.frame.title_pt, 20.0, false),
        Role::Title,
    );
}

pub(crate) fn standard_footer(b: &mut B) {
    for f in footer_shapes(b.cx) {
        b.sp(f, Role::Footer);
    }
}

/// The slide's blocks, or blocks derived from its bullets.
pub(crate) fn blocks_of(s: &DesignedSlide) -> Vec<Block> {
    if !s.blocks.is_empty() {
        return s.blocks.clone();
    }
    s.bullets
        .iter()
        .map(|b| {
            // "Fast delivery — ships in days" → heading + text.
            let (h, t) = match b.split_once(" — ").or_else(|| b.split_once(": ")) {
                Some((h, t)) if h.chars().count() <= 40 => (h.to_string(), t.to_string()),
                _ => (String::new(), b.clone()),
            };
            Block {
                heading: h,
                text: if t.is_empty() { vec![] } else { vec![t] },
                ..Default::default()
            }
        })
        .collect()
}

/// The top of a block of height `h` centred in `area`, lifted slightly
/// above true centre (optical centre) and never above the area's top.
pub(crate) fn vcentre(area: R, h: i64) -> i64 {
    let slack = (area.h - h).max(0);
    area.y + slack * 45 / 100
}

pub(crate) fn art_panel(t: &ThemeSpec, rect: R) -> Vec<Shape> {
    let mut v = vec![Shape::deco(
        "Art panel",
        radius(t),
        rect,
        Fill::Linear {
            angle: 135,
            stops: vec![(0, Clr::accent(1)), (100, Clr::accent(2))],
        },
    )];
    let (cx, cy) = (rect.x + rect.w * 2 / 3, rect.y + rect.h / 3);
    for (i, d) in [rect.w / 2, rect.w * 3 / 4, rect.w].iter().enumerate() {
        let mut r = Shape::deco(
            format!("Art ring {}", i + 1),
            Geom::Ellipse,
            R::new(cx - d / 2, cy - d / 2, *d, *d),
            Fill::None,
        );
        r.line = Some(Line::new(Clr::lt1().alpha(38 - i as u32 * 10), 1.25));
        v.push(r);
    }
    v.push(Shape::deco(
        "Art disc",
        Geom::Ellipse,
        R::new(cx - rect.w / 10, cy - rect.w / 10, rect.w / 5, rect.w / 5),
        Fill::Solid(Clr::lt1().alpha(90)),
    ));
    v
}

pub(crate) fn person_name(blk: &Block) -> String {
    if blk.heading.is_empty() {
        blk.text.join(" ")
    } else {
        blk.heading.clone()
    }
}

pub(crate) fn initials(name: &str) -> String {
    let mut it = name.split_whitespace().filter_map(|w| w.chars().next());
    let a = it.next();
    let b = it.next_back();
    a.into_iter()
        .chain(b)
        .flat_map(|c| c.to_uppercase())
        .collect()
}
