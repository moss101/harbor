//! Bullets, columns, statement, quote, picture and chart slides.

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

pub(crate) fn bullets(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let rect = cx.frame.body;
    let (paras, _) = bullets_paras(cx, &s.bullets, rect, 22.0);
    let mut body = Shape::placeholder("Content", Ph { ty: "body", idx: Some(1) }, Text::new(paras));
    body.rect = None;
    b.sp(body, Role::Body);
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleContent)
}

pub(crate) fn two_column(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let cols = columns(cx.frame.body, 2, GUTTER * 2);
    let (left, right): (Vec<String>, Vec<String>) = if s.blocks.len() >= 2 {
        let join = |blk: &Block| blk.text.clone();
        (join(&s.blocks[0]), join(&s.blocks[1]))
    } else {
        let half = (s.bullets.len() + 1) / 2;
        (s.bullets[..half].to_vec(), s.bullets[half..].to_vec())
    };
    for (i, (col, lines)) in cols.iter().zip([left, right]).enumerate() {
        let mut area = *col;
        let mut paras: Vec<Para> = Vec::new();
        if let Some(h) = s.blocks.get(i).map(|x| x.heading.clone()).filter(|h| !h.is_empty()) {
            paras.push(
                Para::from_runs(vec![Run::new(h)
                    .size(20.0)
                    .bold()
                    .font(FontRef::Major)
                    .color(accent_text(cx.t, 1 + i as u8, 4.5))])
                .no_bullet()
                .after(6.0),
            );
            area.h -= 520_000;
        }
        let (rest, _) = bullets_paras(cx, &lines, area, 20.0);
        paras.extend(rest);
        let mut ph = Shape::placeholder(
            if i == 0 { "Left column" } else { "Right column" },
            Ph { ty: "body", idx: Some(1 + i as u32) },
            Text::new(paras),
        );
        ph.rect = Some(*col);
        b.sp(ph, Role::Body);
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TwoContent)
}

pub(crate) fn quote(s: &DesignedSlide, cx: &Ctx) -> Built {
    let mut b = B::new(cx);
    let text = s
        .bullets
        .first()
        .cloned()
        .unwrap_or_else(|| s.title.clone())
        .trim_matches(|c| c == '"' || c == '\u{201c}' || c == '\u{201d}')
        .to_string();
    let poster = matches!(cx.t.look.header, Header::Poster);
    let tr = if poster {
        R::new(MARGIN_X, 900_000, CONTENT_W, 4_000_000)
    } else {
        R::new(1_300_000, 1_500_000, 9_600_000, 3_000_000)
    };
    let pt = fit_pt(&[text.clone()], tr.w, tr.h, if poster { 40.0 } else { 36.0 }, 20.0, factor_major(cx.t), 1.0, 0.0);
    let mut title = Shape::placeholder(
        "Quote",
        Ph { ty: "title", idx: None },
        Text::one(Para::from_runs(vec![Run::new(text).size(pt)]).no_bullet())
            .anchor(if poster { Anchor::Bottom } else { Anchor::Middle }),
    );
    title.rect = Some(tr);
    b.sp(title, Role::Title);
    let who = s
        .attribution
        .clone()
        .or_else(|| if s.bullets.len() > 1 { s.bullets.get(1).cloned() } else { None })
        .unwrap_or_else(|| s.title.clone());
    let mut by = Shape::placeholder(
        "Attribution",
        Ph { ty: "body", idx: Some(1) },
        Text::one(Para::from_runs(vec![Run::new(format!("\u{2014} {who}"))]).no_bullet()),
    );
    by.rect = Some(if poster {
        R::new(MARGIN_X, 5_400_000, CONTENT_W, 600_000)
    } else {
        R::new(1_300_000, 4_800_000, 9_600_000, 800_000)
    });
    b.sp(by, Role::Subtitle);
    standard_footer(&mut b);
    b.finish(LayoutPart::Quote)
}

pub(crate) fn image_split(s: &DesignedSlide, cx: &Ctx) -> Built {
    let area = cx.frame.body;
    let mut b = B::new(cx);
    let t = cx.t;
    if let Some(k) = &s.kicker {
        b.sp(kicker_shape(cx, k), Role::Kicker);
    }
    let text_w = (area.w * 54 / 100).min(5_600_000);
    let tr = R::new(area.x, TITLE_Y, text_w, TITLE_H + 500_000);
    let mut title = title_shape(cx, &s.title, Some(tr), 32.0, 22.0, false);
    if let Some(txt) = title.text.as_mut() {
        txt.anchor = Anchor::Top;
    }
    b.sp(title, Role::Title);
    let br = R::new(area.x, area.y + 520_000, text_w, area.h - 520_000);
    let (paras, _) = bullets_paras(cx, &s.bullets, br, 20.0);
    let mut body = Shape::placeholder("Content", Ph { ty: "body", idx: Some(1) }, Text::new(paras));
    body.rect = Some(br);
    b.sp(body, Role::Body);
    let pic = R::new(area.x + text_w + GUTTER * 2, 520_000, area.w - text_w - GUTTER * 2, 5_700_000);
    match &s.image {
        Some(img) => b.items.push(Item {
            el: El::Pic {
                name: "Picture".into(),
                alt: s.title.clone(),
                rect: pic,
                geom: if t.radius == 0 { Geom::Rect } else { Geom::Round(t.radius.min(8)) },
                image: img.clone(),
            },
            role: Role::Image,
        }),
        None => {
            for sh in art_panel(t, pic) {
                b.sp(sh, Role::Image);
            }
        }
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleContent)
}

pub(crate) fn chart(s: &DesignedSlide, cx: &Ctx) -> Built {
    let area = cx.frame.body;
    let mut b = B::new(cx);
    standard_header(&mut b, s);
    let has_side = !s.bullets.is_empty();
    let area = R::new(area.x, area.y, area.w, area.h - 100_000);
    let chart_rect = if has_side {
        R::new(area.x, area.y, area.w * 62 / 100, area.h)
    } else {
        area
    };
    if let Some(spec) = &s.chart {
        b.items.push(Item {
            el: El::Chart {
                name: "Chart".into(),
                alt: format!("{}: {}", spec.title, s.title),
                rect: chart_rect,
                spec: spec.clone(),
            },
            role: Role::Chart,
        });
    }
    if has_side {
        let side = R::new(
            area.x + chart_rect.w + GUTTER,
            area.y,
            area.w - chart_rect.w - GUTTER,
            area.h,
        );
        b.card("Takeaways", side, Role::Item(0));
        b.sp(
            Shape::deco(
                "Takeaway tick",
                Geom::Rect,
                R::new(side.x + 300_000, side.y + 320_000, 520_000, 56_000),
                Fill::Solid(Clr::accent(1)),
            ),
            Role::Item(0),
        );
        let inner = R::new(side.x + 300_000, side.y + 520_000, side.w - 600_000, side.h - 760_000);
        let (paras, _) = bullets_paras(cx, &s.bullets, inner, 18.0);
        let paras: Vec<Para> = paras
            .into_iter()
            .map(|p| p.bullet("\u{2022}", Clr::accent(1)).margin(257_175, -257_175))
            .collect();
        b.sp(Shape::text_box("Takeaway text", inner, Text::new(paras)), Role::Item(0));
    }
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}

pub(crate) fn statement(s: &DesignedSlide, cx: &Ctx) -> Built {
    let area = cx.frame.body;
    let mut b = B::new(cx);
    if let Some(k) = &s.kicker {
        b.sp(kicker_shape(cx, k), Role::Kicker);
    }
    let w = if area.w > 9_000_000 { area.w - 1_600_000 } else { area.w - 300_000 };
    let area = R::new(area.x, 1_500_000, w, 3_300_000);
    let mut title = title_shape(cx, &s.title, Some(area), 48.0, 26.0, false);
    if let Some(txt) = title.text.as_mut() {
        txt.anchor = Anchor::Middle;
    }
    b.sp(title, Role::Title);
    b.sp(
        Shape::deco(
            "Statement rule",
            Geom::Rect,
            R::new(area.x, 5_050_000, 900_000, 56_000),
            Fill::Solid(Clr::accent(1)),
        ),
        Role::Deco,
    );
    standard_footer(&mut b);
    b.finish(LayoutPart::TitleOnly)
}
