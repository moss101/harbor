//! Sample and starter decks.
//!
//! * [`sample_deck`] is a short showcase in a theme (cover, a statement,
//!   cards, figures, a timeline, a close) for the theme gallery: every
//!   theme is judged on the same content, so differences are the design's.
//! * [`starter_deck`] is a ready-to-edit deck for a purpose: one slide per
//!   beat of its storyline, in the layout that beat usually takes, with
//!   placeholder copy that says what to write. It is what "New pitch deck"
//!   creates.
//!
//! Copy exists in English and Arabic; an Arabic deck is right-to-left.

use super::model::{Block, Design, DesignedDeck, DesignedSlide, LayoutKind};
use super::motion::MotionPreset;
use super::purpose::{purpose, Beat};
use crate::pptx::{ChartKind, ChartSpec};

/// Placeholder and showcase strings, by language.
struct Copy {
    deck_title: &'static str,
    deck_subtitle: &'static str,
    statement: &'static str,
    cards_title: &'static str,
    cards: [(&'static str, &'static str); 3],
    stats_title: &'static str,
    stats: [(&'static str, &'static str); 4],
    timeline_title: &'static str,
    steps: [(&'static str, &'static str, &'static str); 4],
    compare: [(&'static str, [&'static str; 3]); 2],
    chart_title: &'static str,
    chart_series: &'static str,
    chart_takeaways: [&'static str; 2],
    people: [(&'static str, &'static str, &'static str); 3],
    quote: &'static str,
    quote_by: &'static str,
    agenda: [&'static str; 4],
    close_title: &'static str,
    contact: [&'static str; 2],
    image_bullets: [&'static str; 3],
    metric_label: &'static str,
    replace_hint: &'static str,
    section_hint: &'static str,
}

const EN: Copy = Copy {
    deck_title: "Your presentation title",
    deck_subtitle: "A line that says why it matters",
    statement: "One sentence your audience will remember",
    cards_title: "Three ideas, one clear story",
    cards: [
        ("First idea", "Say it in one short sentence."),
        ("Second idea", "Back it with a number or an example."),
        ("Third idea", "Close the loop for the reader."),
    ],
    stats_title: "The numbers that matter",
    stats: [
        ("48%", "growth, year on year"),
        ("2.4M", "people reached"),
        ("12", "markets live"),
        ("98%", "customer satisfaction"),
    ],
    timeline_title: "How it unfolds",
    steps: [
        ("Q1", "Start", "Lay the groundwork"),
        ("Q2", "Build", "Ship the first version"),
        ("Q3", "Grow", "Reach the next audience"),
        ("Q4", "Scale", "Make it routine"),
    ],
    compare: [
        (
            "Before",
            ["Slow and manual", "Hard to track", "Costly to change"],
        ),
        (
            "After",
            ["Fast and automatic", "Clear to everyone", "Easy to adapt"],
        ),
    ],
    chart_title: "Growth over time",
    chart_series: "This year",
    chart_takeaways: ["Up and to the right", "The trend is accelerating"],
    people: [
        (
            "Alex Morgan",
            "Role or title",
            "One line about what they bring",
        ),
        (
            "Sam Rivera",
            "Role or title",
            "One line about what they bring",
        ),
        (
            "Jo Carter",
            "Role or title",
            "One line about what they bring",
        ),
    ],
    quote: "\u{201c}A sentence in your customer's own words.\u{201d}",
    quote_by: "Name, role, company",
    agenda: ["First topic", "Second topic", "Third topic", "Questions"],
    close_title: "Thank you",
    contact: ["name@example.com", "example.com"],
    image_bullets: ["What the picture shows", "Why it matters", "What to notice"],
    metric_label: "your metric",
    replace_hint: "Replace this with your own words.",
    section_hint: "What this part is about",
};

const AR: Copy = Copy {
    deck_title: "عنوان عرضك التقديمي",
    deck_subtitle: "سطر يوضح لماذا يهم",
    statement: "جملة واحدة يتذكرها جمهورك",
    cards_title: "ثلاث أفكار وقصة واحدة واضحة",
    cards: [
        ("الفكرة الأولى", "اشرحها في جملة قصيرة."),
        ("الفكرة الثانية", "ادعمها برقم أو مثال."),
        ("الفكرة الثالثة", "أغلق الدائرة للقارئ."),
    ],
    stats_title: "الأرقام المهمة",
    stats: [
        ("٤٨٪", "نمو سنوي"),
        ("٢٫٤ مليون", "شخص وصلنا إليه"),
        ("١٢", "سوقًا نشطًا"),
        ("٩٨٪", "رضا العملاء"),
    ],
    timeline_title: "كيف تتطور الخطة",
    steps: [
        ("الربع ١", "الانطلاق", "وضع الأساس"),
        ("الربع ٢", "البناء", "إطلاق النسخة الأولى"),
        ("الربع ٣", "النمو", "الوصول إلى جمهور جديد"),
        ("الربع ٤", "التوسع", "جعلها روتينًا"),
    ],
    compare: [
        ("قبل", ["بطيء ويدوي", "يصعب تتبعه", "مكلف التغيير"]),
        ("بعد", ["سريع وتلقائي", "واضح للجميع", "سهل التكيّف"]),
    ],
    chart_title: "النمو عبر الزمن",
    chart_series: "هذا العام",
    chart_takeaways: ["اتجاه تصاعدي", "الوتيرة تتسارع"],
    people: [
        ("أحمد خالد", "المسمى الوظيفي", "سطر عمّا يضيفه"),
        ("سارة حسن", "المسمى الوظيفي", "سطر عمّا تضيفه"),
        ("عمر ناصر", "المسمى الوظيفي", "سطر عمّا يضيفه"),
    ],
    quote: "«جملة بكلمات عميلك نفسه.»",
    quote_by: "الاسم، المنصب، الشركة",
    agenda: ["الموضوع الأول", "الموضوع الثاني", "الموضوع الثالث", "الأسئلة"],
    close_title: "شكرًا لكم",
    contact: ["name@example.com", "example.com"],
    image_bullets: ["ما تُظهره الصورة", "لماذا تهم", "ما يستحق الانتباه"],
    metric_label: "مؤشرك",
    replace_hint: "استبدل هذا بكلماتك.",
    section_hint: "عمّا يدور هذا الجزء",
};

fn copy(rtl: bool) -> &'static Copy {
    if rtl {
        &AR
    } else {
        &EN
    }
}

/// The same showcase content in any theme.
pub fn sample_deck(theme: &str, motion: MotionPreset, rtl: bool) -> DesignedDeck {
    let c = copy(rtl);
    let mut slides = vec![
        DesignedSlide::new(c.deck_title)
            .bullets([c.deck_subtitle])
            .layout(LayoutKind::Cover),
        DesignedSlide::new(c.statement).layout(LayoutKind::Statement),
        cards_slide(c),
        stats_slide(c),
        timeline_slide(c),
        DesignedSlide::new(c.close_title)
            .bullets(c.contact)
            .layout(LayoutKind::Closing),
    ];
    for s in &mut slides {
        s.notes = None;
    }
    DesignedDeck {
        title: c.deck_title.into(),
        subtitle: Some(c.deck_subtitle.into()),
        slides,
        design: Design {
            theme: theme.into(),
            motion,
            rtl,
            ..Default::default()
        },
    }
}

fn cards_slide(c: &Copy) -> DesignedSlide {
    DesignedSlide::new(c.cards_title)
        .layout(LayoutKind::Cards)
        .blocks(c.cards.iter().map(|(h, t)| Block::card(*h, *t)).collect())
}

fn stats_slide(c: &Copy) -> DesignedSlide {
    DesignedSlide::new(c.stats_title)
        .layout(LayoutKind::Stats)
        .blocks(c.stats.iter().map(|(v, l)| Block::stat(*v, *l)).collect())
}

fn timeline_slide(c: &Copy) -> DesignedSlide {
    DesignedSlide::new(c.timeline_title)
        .layout(LayoutKind::Timeline)
        .blocks(
            c.steps
                .iter()
                .map(|(w, h, d)| Block::step(*w, *h, *d))
                .collect(),
        )
}

/// Placeholder content for one storyline beat in the layout it usually takes.
fn beat_slide(beat: &Beat, c: &Copy, index: usize, last: bool) -> DesignedSlide {
    let kicker = (!beat.kicker.is_empty()).then(|| beat.kicker.to_string());
    let title = if beat.kicker.is_empty() {
        beat.says.to_string()
    } else {
        beat.kicker.to_string()
    };
    let mut s = match beat.layout {
        LayoutKind::Cover => DesignedSlide::new(c.deck_title).bullets([c.deck_subtitle]),
        LayoutKind::Closing => DesignedSlide::new(c.close_title).bullets(c.contact),
        LayoutKind::Section => DesignedSlide::new(title).bullets([c.section_hint]),
        LayoutKind::Statement => DesignedSlide::new(beat.says),
        LayoutKind::Cards => DesignedSlide::new(title)
            .blocks(c.cards.iter().map(|(h, t)| Block::card(*h, *t)).collect()),
        LayoutKind::Stats => DesignedSlide::new(title).blocks(
            c.stats
                .iter()
                .take(3)
                .map(|(v, _)| Block::stat(*v, c.metric_label))
                .collect(),
        ),
        LayoutKind::Timeline => DesignedSlide::new(title).blocks(
            c.steps
                .iter()
                .map(|(w, h, d)| Block::step(*w, *h, *d))
                .collect(),
        ),
        LayoutKind::Comparison => DesignedSlide::new(title).blocks(
            c.compare
                .iter()
                .map(|(h, pts)| Block {
                    heading: (*h).to_string(),
                    text: pts.iter().map(|p| p.to_string()).collect(),
                    ..Default::default()
                })
                .collect(),
        ),
        LayoutKind::Team => DesignedSlide::new(title).blocks(
            c.people
                .iter()
                .map(|(n, r, b)| Block {
                    heading: (*n).to_string(),
                    text: vec![(*r).to_string(), (*b).to_string()],
                    ..Default::default()
                })
                .collect(),
        ),
        LayoutKind::Quote => DesignedSlide::new(title)
            .bullets([c.quote])
            .attribution(c.quote_by),
        LayoutKind::Agenda => DesignedSlide::new(title).bullets(c.agenda),
        LayoutKind::Chart => {
            DesignedSlide::new(title)
                .bullets(c.chart_takeaways)
                .chart(ChartSpec {
                    kind: ChartKind::Bar,
                    title: c.chart_title.into(),
                    categories: ["Q1", "Q2", "Q3", "Q4"]
                        .iter()
                        .map(|q| q.to_string())
                        .collect(),
                    series: vec![(c.chart_series.into(), vec![12.0, 18.0, 25.0, 34.0])],
                })
        }
        LayoutKind::ImageSplit => DesignedSlide::new(title).bullets(c.image_bullets),
        LayoutKind::Bullets | LayoutKind::TwoColumn => {
            DesignedSlide::new(title).bullets([beat.says, c.replace_hint])
        }
    };
    s.kicker = kicker;
    s.layout = Some(beat.layout);
    // The storyline's guidance rides in the speaker notes, where the author
    // sees it without it ever appearing on a slide.
    s.notes = Some(
        format!(
            "{} {}",
            beat.says,
            if index == 0 || last {
                ""
            } else {
                c.replace_hint
            }
        )
        .trim()
        .to_string(),
    );
    s
}

/// A ready-to-edit deck for `purpose_id`, or `None` for an unknown one.
pub fn starter_deck(
    purpose_id: &str,
    theme: Option<&str>,
    motion: Option<MotionPreset>,
    rtl: bool,
    title: Option<&str>,
) -> Option<DesignedDeck> {
    let p = purpose(purpose_id)?;
    let c = copy(rtl);
    let n = p.beats.len();
    let slides: Vec<DesignedSlide> = p
        .beats
        .iter()
        .enumerate()
        .map(|(i, b)| beat_slide(b, c, i, i + 1 == n))
        .collect();
    let mut deck = DesignedDeck {
        title: title.unwrap_or(c.deck_title).to_string(),
        subtitle: Some(c.deck_subtitle.into()),
        slides,
        design: Design {
            theme: theme.unwrap_or(p.theme).to_string(),
            motion: motion.unwrap_or(p.motion),
            rtl,
            progressive_bullets: p.progressive,
            ..Default::default()
        },
    };
    if let Some(first) = deck.slides.first_mut() {
        first.title = deck.title.clone();
    }
    Some(deck)
}

#[cfg(test)]
mod tests {
    use super::super::purpose::PURPOSES;
    use super::super::theme::THEMES;
    use super::*;

    #[test]
    fn every_purpose_has_a_buildable_starter_in_both_languages() {
        for p in PURPOSES {
            for rtl in [false, true] {
                let deck = starter_deck(p.id, None, None, rtl, None).unwrap();
                assert_eq!(deck.slides.len(), p.beats.len(), "{}", p.id);
                let bytes = deck
                    .to_pptx_bytes()
                    .unwrap_or_else(|e| panic!("{} rtl={rtl}: {e}", p.id));
                let back = crate::pptx::PptxDeck::from_pptx_bytes(&bytes).unwrap();
                assert_eq!(back.slides.len(), p.beats.len());
                assert!(
                    back.slides.iter().all(|s| !s.title.trim().is_empty()),
                    "{}: every slide has a title",
                    p.id
                );
            }
        }
        assert!(starter_deck("nope", None, None, false, None).is_none());
    }

    #[test]
    fn every_theme_builds_the_sample_deck() {
        for t in THEMES {
            for rtl in [false, true] {
                for m in MotionPreset::ALL {
                    sample_deck(t.id, m, rtl)
                        .to_pptx_bytes()
                        .unwrap_or_else(|e| panic!("{} {m:?}: {e}", t.id));
                }
            }
        }
    }

    #[test]
    fn a_starter_follows_its_storyline_and_honours_choices() {
        let d = starter_deck(
            "pitch_deck",
            Some("mono"),
            Some(MotionPreset::None),
            false,
            Some("Acme"),
        )
        .unwrap();
        assert_eq!(d.title, "Acme");
        assert_eq!(d.slides[0].title, "Acme");
        assert_eq!(d.design.theme, "mono");
        assert_eq!(d.design.motion, MotionPreset::None);
        let kinds = d.resolved_layouts();
        assert_eq!(kinds[0], LayoutKind::Cover);
        assert_eq!(*kinds.last().unwrap(), LayoutKind::Closing);
        assert!(kinds.contains(&LayoutKind::Stats) && kinds.contains(&LayoutKind::Team));
        // Guidance lives in the notes, not on the slide.
        assert!(d.slides[1].notes.as_deref().unwrap_or("").len() > 10);
        // The Arabic starter is a right-to-left deck in Arabic copy.
        let ar = starter_deck("project_report", None, None, true, None).unwrap();
        assert!(ar.design.rtl);
        assert!(ar.slides[0]
            .title
            .chars()
            .any(|c| ('\u{0600}'..='\u{06ff}').contains(&c)));
    }
}
