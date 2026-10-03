//! Per-layout slide builders, grouped by what they draw.

pub mod agenda;
pub mod bullets;
pub mod cards;
pub mod compare;
pub mod kit;
pub mod stats;
pub mod timeline;

pub use kit::{on_accent, Built, Ctx, El, Item};

use super::cover;
use super::model::{DesignedSlide, LayoutKind};
use super::parts::LayoutPart;

pub fn build(kind: LayoutKind, s: &DesignedSlide, cx: &Ctx) -> Built {
    match kind {
        LayoutKind::Cover => cover::hero(s, cx, LayoutPart::Cover),
        LayoutKind::Closing => cover::hero(s, cx, LayoutPart::Closing),
        LayoutKind::Section => cover::section(s, cx),
        LayoutKind::Bullets => bullets::bullets(s, cx),
        LayoutKind::TwoColumn => bullets::two_column(s, cx),
        LayoutKind::Comparison => compare::comparison(s, cx),
        LayoutKind::Stats => stats::stats(s, cx),
        LayoutKind::Cards => cards::cards(s, cx),
        LayoutKind::Timeline => timeline::timeline(s, cx),
        LayoutKind::Quote => bullets::quote(s, cx),
        LayoutKind::ImageSplit => bullets::image_split(s, cx),
        LayoutKind::Chart => bullets::chart(s, cx),
        LayoutKind::Agenda => agenda::agenda(s, cx),
        LayoutKind::Team => cards::team(s, cx),
        LayoutKind::Statement => bullets::statement(s, cx),
    }
}

#[cfg(test)]
mod tests {
    use crate::deck::theme::{theme, THEMES};
    use super::*;
    use super::kit::*;
    use crate::deck::grid::*;
    use crate::deck::model::*;
    use crate::deck::xml::*;
    use crate::deck::theme::*;
    use crate::deck::look::*;

    fn ctx<'a>(t: &'a ThemeSpec, deck: &'a str) -> Ctx<'a> {
        Ctx {
            t,
            frame: frame(&t.look),
            rtl: false,
            footer: deck.to_string(),
            numbers: true,
            index: 3,
            deck_title: deck,
            section_no: 2,
        }
    }

    fn rich_slide() -> DesignedSlide {
        DesignedSlide::new("Why customers choose Harbor over a stack of tools")
            .kicker("Why us")
            .bullets(["38% faster close", "2.1x pipeline coverage", "$4.2M ARR added"])
            .blocks(vec![
                Block::card("Local first", "Documents never leave the device unless you say so."),
                Block::card("Verified formulas", "Only functions proven against Excel are marked verified."),
                Block::card("One quiet app", "Documents, sheets, decks and PDF in one place."),
            ])
            .notes("n")
    }

    #[test]
    fn every_layout_builds_for_every_theme_with_sane_geometry() {
        for t in THEMES {
            for kind in LayoutKind::ALL {
                let mut s = rich_slide();
                s.layout = Some(kind);
                if kind == LayoutKind::Stats {
                    s.blocks = vec![Block::stat("38%", "faster close"), Block::stat("2.1x", "pipeline"), Block::stat("$4.2M", "ARR")];
                }
                if kind == LayoutKind::Timeline {
                    s.blocks = vec![Block::step("Q1", "Beta", "Ten design partners"), Block::step("Q2", "Launch", "Public"), Block::step("Q3", "Scale", "EMEA")];
                }
                if kind == LayoutKind::Chart {
                    s.chart = Some(crate::pptx::ChartSpec {
                        kind: crate::pptx::ChartKind::Bar,
                        title: "Rev".into(),
                        categories: vec!["a".into(), "b".into()],
                        series: vec![("s".into(), vec![1.0, 2.0])],
                    });
                }
                let built = build(kind, &s, &ctx(t, "Harbor"));
                assert!(!built.items.is_empty(), "{} {kind:?}", t.id);
                for it in &built.items {
                    if let El::Sp(sh) = &it.el {
                        if let Some(r) = sh.rect {
                            assert!(r.w >= 0 && r.h >= 0, "{} {kind:?} {}: negative size {r:?}", t.id, sh.name);
                            // Content stays on the slide (ornaments may bleed).
                            if !sh.decorative {
                                assert!(r.x >= 0 && r.y >= 0 && r.right() <= SLIDE_W && r.bottom() <= SLIDE_H,
                                    "{} {kind:?} {}: off slide {r:?}", t.id, sh.name);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn on_accent_text_is_always_legible_for_bold_chip_text() {
        for t in THEMES {
            for n in 1..=6u8 {
                let hex = t.palette.accents[n as usize - 1];
                let fg = match on_accent(t, n) {
                    c if c == Clr::lt1() => t.palette.lt1,
                    _ => t.palette.dk1,
                };
                assert!(
                    contrast(hex, fg) >= 3.0,
                    "{} accent{n} {hex} vs {fg}: {:.2}",
                    t.id,
                    contrast(hex, fg)
                );
            }
        }
    }

    #[test]
    fn accent_text_falls_back_to_ink_when_the_accent_would_not_read() {
        let sunrise = theme("sunrise").unwrap();
        // Sunrise coral on its cream page is below 4.5 for small text.
        assert_eq!(accent_text(sunrise, 1, 4.5), Clr::tx1());
        let aurora = theme("aurora").unwrap();
        assert_eq!(accent_text(aurora, 2, 4.5), Clr::accent(2));
    }

    #[test]
    fn stats_derive_from_bullets_and_cap_at_four() {
        let t = theme("harbor").unwrap();
        let s = DesignedSlide::new("Growth").bullets(["38% faster", "2.1x pipeline", "$4.2M ARR", "9 new logos", "extra 5"]);
        let built = super::stats::stats(&s, &ctx(t, "D"));
        let ticks = built.items.iter().filter(|i| matches!(&i.el, El::Sp(s) if s.name.starts_with("Stat tick"))).count();
        assert_eq!(ticks, 4);
    }

    #[test]
    fn initials_and_comparison_titles() {
        assert_eq!(initials("Layla Hassan"), "LH");
        assert_eq!(initials("Plato"), "P");
        assert_eq!(super::compare::comparison_titles("Build vs buy"), ("Build".into(), "buy".into()));
        assert_eq!(super::compare::comparison_titles("Pricing"), ("Option A".into(), "Option B".into()));
    }

    // ---- the generic quality gates every layout variant must pass ----------

    /// Estimated height (EMU) the text of a shape needs, and the height it
    /// has. Sizes come from the runs; unsized runs count as 18 pt.
    fn text_need(t: &ThemeSpec, sh: &Shape) -> Option<(i64, i64)> {
        let text = sh.text.as_ref()?;
        let rect = sh.rect?;
        let (l, tp, r, b) = text.insets;
        let base_w = rect.w - l - r;
        let avail_h = rect.h - tp - b;
        let mut need = 0.0f64;
        for p in &text.paras {
            let s: String = p.runs.iter().map(|r| r.text.as_str()).collect();
            let pt = p.runs.iter().filter_map(|r| r.size).fold(0.0, f64::max).max(if p.runs.iter().any(|r| r.size.is_some()) { 0.0 } else { 18.0 });
            let major = p.runs.first().map_or(false, |r| r.font == Some(FontRef::Major));
            let factor = glyph_factor(if major { t.fonts.major } else { t.fonts.minor });
            let w = base_w - p.margin.map_or(0, |(m, _)| m.max(0));
            let lines = line_count(&s, pt, w.max(1), factor) as f64;
            let spacing = p.line.map_or(1.0, |l| l as f64 / 100.0);
            need += lines * pt * 1.2 * spacing + p.before.unwrap_or(0.0) + p.after.unwrap_or(0.0);
        }
        Some(((need * EMU_PER_PT) as i64, avail_h))
    }

    fn dense_slide(kind: LayoutKind) -> DesignedSlide {
        let mut s = DesignedSlide::new("Why customers choose Harbor over a stack of tools").kicker("Why us");
        s.layout = Some(kind);
        s.bullets = vec![
            "38% faster close".into(),
            "2.1x pipeline coverage".into(),
            "$4.2M ARR added".into(),
            "9 new logos this quarter".into(),
        ];
        s.blocks = match kind {
            LayoutKind::Stats => vec![
                Block::stat("$4.2B", "addressable market"),
                Block::stat("38%", "yearly growth"),
                Block::stat("12M", "teams on legacy suites"),
                Block::stat("2.1x", "pipeline coverage"),
            ],
            LayoutKind::Timeline => vec![
                Block::step("Q1", "Private beta", "Ten design partners"),
                Block::step("Q2", "Public launch", "Self-serve onboarding"),
                Block::step("Q3", "Teams", "Shared workspaces"),
                Block::step("Q4", "EMEA", "Arabic first-class"),
            ],
            LayoutKind::Team => (0..4)
                .map(|i| Block {
                    heading: format!("Person Number{i}"),
                    text: vec!["Role or title".into(), "One line about what they bring to the team".into()],
                    ..Default::default()
                })
                .collect(),
            LayoutKind::Comparison => vec![
                Block { heading: "Legacy suites".into(), text: vec!["Cloud by default".into(), "Per-seat pricing".into(), "Opaque formulas".into()], ..Default::default() },
                Block { heading: "Harbor".into(), text: vec!["Local by default".into(), "Flat pricing".into(), "Verified formulas".into()], ..Default::default() },
            ],
            _ => (0..4)
                .map(|i| Block::card(format!("Idea number {i}"), "Documents never leave the device unless you say so, and it stays that way."))
                .collect(),
        };
        s
    }

    /// Every figure/card/timeline/compare/agenda/team VARIANT, under every
    /// header composition and every theme's fonts: text boxes must fit their
    /// boxes, and nothing may leave the page.
    #[test]
    fn no_variant_overflows_its_text_boxes_or_the_page() {
        use crate::deck::look::*;
        let kinds = [
            LayoutKind::Stats,
            LayoutKind::Cards,
            LayoutKind::Timeline,
            LayoutKind::Comparison,
            LayoutKind::Agenda,
            LayoutKind::Team,
        ];
        let headers = [
            Header::TopLeft,
            Header::Banner,
            Header::Sidebar { right: false },
            Header::Sidebar { right: true },
            Header::Rail,
            Header::Centered,
            Header::Poster,
        ];
        let mut failures: Vec<String> = Vec::new();
        for base in THEMES {
            for header in headers {
                for stats in [StatsStyle::Tiles, StatsStyle::Rows, StatsStyle::Hero, StatsStyle::Columns] {
                    for cards in [CardsStyle::Grid, CardsStyle::Rows, CardsStyle::Columns, CardsStyle::Stickers, CardsStyle::Bento] {
                        // One axis varied at a time keeps this tractable while
                        // still crossing every variant with every header.
                        for (axis, look) in [
                            ("stats", Look { header, stats, ..base.look }),
                            ("cards", Look { header, cards, ..base.look }),
                            ("timeline", Look { header, timeline: TimelineStyle::Chevrons, ..base.look }),
                            ("timeline", Look { header, timeline: TimelineStyle::Vertical, ..base.look }),
                            ("timeline", Look { header, timeline: TimelineStyle::Spine, ..base.look }),
                            ("compare", Look { header, compare: CompareStyle::Rows, ..base.look }),
                            ("compare", Look { header, compare: CompareStyle::Fields, ..base.look }),
                            ("compare", Look { header, compare: CompareStyle::Panels, ..base.look }),
                            ("agenda", Look { header, agenda: AgendaStyle::Grid, ..base.look }),
                            ("agenda", Look { header, agenda: AgendaStyle::Rows, ..base.look }),
                            ("team", Look { header, team: TeamStyle::Inline, ..base.look }),
                            ("team", Look { header, team: TeamStyle::Cards, ..base.look }),
                        ] {
                            let t = ThemeSpec { look, ..*base };
                            for kind in kinds {
                                let cx = Ctx {
                                    t: &t,
                                    frame: frame(&t.look),
                                    rtl: false,
                                    footer: "Harbor".into(),
                                    numbers: true,
                                    index: 3,
                                    deck_title: "Harbor",
                                    section_no: 1,
                                };
                                let built = build(kind, &dense_slide(kind), &cx);
                                for it in &built.items {
                                    let El::Sp(sh) = &it.el else { continue };
                                    let who = format!("{} {header:?} {axis} {kind:?} '{}'", base.id, sh.name);
                                    if let Some(r) = sh.rect {
                                        if !sh.decorative
                                            && (r.x < 0 || r.y < 0 || r.right() > SLIDE_W || r.bottom() > SLIDE_H)
                                        {
                                            failures.push(format!("{who}: off page {r:?}"));
                                        }
                                    }
                                    if sh.ph.is_some() {
                                        continue;
                                    }
                                    if let Some((need, have)) = text_need(&t, sh) {
                                        if need as f64 > have as f64 * 1.06 + 25_000.0 {
                                            failures.push(format!("{who}: text needs {need} but box has {have}"));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        failures.sort();
        failures.dedup();
        assert!(
            failures.is_empty(),
            "{} overflow(s); first few:\n{}",
            failures.len(),
            failures.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}
