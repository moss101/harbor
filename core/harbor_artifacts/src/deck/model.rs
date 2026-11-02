//! What a designed deck is made of: content (what to say) kept apart from
//! design (how it looks and moves). A slide states its text, optionally a
//! layout; everything visual is the design engine's decision.

use crate::pptx::{ChartSpec, SlideImage};

/// A named unit of content on a slide: a card, a stat, a milestone, a
/// column, a team member. Layouts read the fields they need.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Block {
    /// Heading: card title, milestone name, person's name, column title.
    pub heading: String,
    /// Body lines: card text, milestone detail, role + bio.
    pub text: Vec<String>,
    /// A headline figure (`"$4.2M"`, `"38%"`, `"2.1x"`) for stat tiles.
    pub value: Option<String>,
    /// What the figure measures, or the milestone's date (`"Q3 2026"`).
    pub label: Option<String>,
}

impl Block {
    pub fn card(heading: impl Into<String>, text: impl Into<String>) -> Self {
        Block {
            heading: heading.into(),
            text: vec![text.into()],
            ..Default::default()
        }
    }
    pub fn stat(value: impl Into<String>, label: impl Into<String>) -> Self {
        Block {
            value: Some(value.into()),
            label: Some(label.into()),
            ..Default::default()
        }
    }
    pub fn step(
        when: impl Into<String>,
        heading: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Block {
            heading: heading.into(),
            text: vec![detail.into()],
            label: Some(when.into()),
            ..Default::default()
        }
    }
}

/// The layout families. `None` on a slide means "infer from content".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    Cover,
    Section,
    Bullets,
    TwoColumn,
    Comparison,
    Stats,
    Cards,
    Timeline,
    Quote,
    ImageSplit,
    Chart,
    Agenda,
    Team,
    Statement,
    Closing,
}

impl LayoutKind {
    pub const ALL: [LayoutKind; 15] = [
        LayoutKind::Cover,
        LayoutKind::Section,
        LayoutKind::Bullets,
        LayoutKind::TwoColumn,
        LayoutKind::Comparison,
        LayoutKind::Stats,
        LayoutKind::Cards,
        LayoutKind::Timeline,
        LayoutKind::Quote,
        LayoutKind::ImageSplit,
        LayoutKind::Chart,
        LayoutKind::Agenda,
        LayoutKind::Team,
        LayoutKind::Statement,
        LayoutKind::Closing,
    ];

    pub fn id(self) -> &'static str {
        match self {
            LayoutKind::Cover => "cover",
            LayoutKind::Section => "section",
            LayoutKind::Bullets => "bullets",
            LayoutKind::TwoColumn => "two_column",
            LayoutKind::Comparison => "comparison",
            LayoutKind::Stats => "stats",
            LayoutKind::Cards => "cards",
            LayoutKind::Timeline => "timeline",
            LayoutKind::Quote => "quote",
            LayoutKind::ImageSplit => "image_split",
            LayoutKind::Chart => "chart",
            LayoutKind::Agenda => "agenda",
            LayoutKind::Team => "team",
            LayoutKind::Statement => "statement",
            LayoutKind::Closing => "closing",
        }
    }

    pub fn parse(s: &str) -> Option<LayoutKind> {
        LayoutKind::ALL.iter().copied().find(|k| k.id() == s)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DesignedSlide {
    pub title: String,
    /// Small caps label above the title (`"PROBLEM"`, `"Q3 RESULTS"`).
    pub kicker: Option<String>,
    pub layout: Option<LayoutKind>,
    pub bullets: Vec<String>,
    pub blocks: Vec<Block>,
    pub notes: Option<String>,
    pub chart: Option<ChartSpec>,
    pub image: Option<SlideImage>,
    /// Who said it (quote layout) or the byline.
    pub attribution: Option<String>,
}

impl DesignedSlide {
    pub fn new(title: impl Into<String>) -> Self {
        DesignedSlide {
            title: title.into(),
            ..Default::default()
        }
    }
    pub fn layout(mut self, k: LayoutKind) -> Self {
        self.layout = Some(k);
        self
    }
    pub fn kicker(mut self, k: impl Into<String>) -> Self {
        self.kicker = Some(k.into());
        self
    }
    pub fn bullets<I: IntoIterator<Item = S>, S: Into<String>>(mut self, b: I) -> Self {
        self.bullets = b.into_iter().map(Into::into).collect();
        self
    }
    pub fn blocks(mut self, b: Vec<Block>) -> Self {
        self.blocks = b;
        self
    }
    pub fn notes(mut self, n: impl Into<String>) -> Self {
        self.notes = Some(n.into());
        self
    }
    pub fn attribution(mut self, a: impl Into<String>) -> Self {
        self.attribution = Some(a.into());
        self
    }
    pub fn chart(mut self, c: ChartSpec) -> Self {
        self.chart = Some(c);
        self
    }

    /// The layout this slide gets: the stated one, else inferred from its
    /// content and position.
    pub fn resolved_layout(&self, index: usize, total: usize) -> LayoutKind {
        if let Some(k) = self.layout {
            return k;
        }
        infer_layout(self, index, total)
    }
}

fn looks_numeric(s: &str) -> bool {
    let t = s.trim();
    let first = t.chars().next();
    matches!(first, Some(c) if c.is_ascii_digit() || matches!(c, '$' | '€' | '£' | '+' | '-' | '~' | '#'))
        && t.chars().take(10).any(|c| c.is_ascii_digit())
}

/// Split `"38% churn reduction"` into (`"38%"`, `"churn reduction"`).
pub fn split_stat(s: &str) -> Option<(String, String)> {
    let t = s.trim();
    if !looks_numeric(t) {
        return None;
    }
    let mut it = t.splitn(2, char::is_whitespace);
    let value = it
        .next()?
        .trim_end_matches([':', ',', '—', '-'])
        .to_string();
    let rest = it
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches(['—', '-', ':'])
        .trim();
    if value.chars().count() > 12 {
        return None;
    }
    Some((value, rest.to_string()))
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// A slide that is just a way to reach someone, or says thank you: the
/// shape of a closing slide whatever its position.
pub fn closing_like(s: &DesignedSlide) -> bool {
    let t = s.title.to_lowercase();
    let title = [
        "thank you",
        "thanks",
        "questions",
        "contact",
        "get in touch",
        "let's talk",
        "let\u{2019}s talk",
    ]
    .iter()
    .any(|w| t.contains(w));
    let contact = !s.bullets.is_empty()
        && s.bullets.len() <= 4
        && s.bullets.iter().all(|b| word_count(b) <= 6)
        && s.bullets.iter().any(|b| {
            let l = b.to_lowercase();
            l.contains('@')
                || l.contains("http")
                || l.contains("www.")
                || l.contains(".com")
                || l.contains(".dev")
                || l.contains(".org")
        });
    title || contact
}

/// Content-driven layout choice, deterministic and explainable: the rules
/// are applied in this order and the first that fits wins.
pub fn infer_layout(s: &DesignedSlide, index: usize, total: usize) -> LayoutKind {
    if index == 0 {
        return LayoutKind::Cover;
    }
    if s.chart.is_some() {
        return LayoutKind::Chart;
    }
    if s.image.is_some() {
        return LayoutKind::ImageSplit;
    }
    if !s.blocks.is_empty() {
        let n = s.blocks.len();
        if s.blocks.iter().all(|b| b.value.is_some()) && (1..=4).contains(&n) {
            return LayoutKind::Stats;
        }
        if s.blocks.iter().all(|b| b.label.is_some()) && (2..=6).contains(&n) {
            return LayoutKind::Timeline;
        }
        if n == 2 && s.blocks.iter().all(|b| !b.heading.is_empty()) {
            return LayoutKind::Comparison;
        }
        if (2..=6).contains(&n) {
            return LayoutKind::Cards;
        }
    }
    let t = s.title.to_lowercase();
    if index + 1 == total && closing_like(s) {
        return LayoutKind::Closing;
    }
    if s.bullets.len() == 1
        && (s.bullets[0].starts_with('"') || s.bullets[0].starts_with('\u{201c}'))
    {
        return LayoutKind::Quote;
    }
    if s.bullets.is_empty() {
        return if word_count(&s.title) <= 4 && index + 1 < total {
            LayoutKind::Section
        } else {
            LayoutKind::Statement
        };
    }
    let stats: Vec<_> = s.bullets.iter().filter_map(|b| split_stat(b)).collect();
    if stats.len() == s.bullets.len() && (2..=4).contains(&stats.len()) {
        return LayoutKind::Stats;
    }
    if (t.contains(" vs ") || t.contains(" versus ") || t.contains("compar"))
        && s.bullets.len() >= 2
    {
        return LayoutKind::Comparison;
    }
    let dated = s
        .bullets
        .iter()
        .filter(|b| {
            let head: String = b.chars().take(12).collect::<String>().to_lowercase();
            head.starts_with('q') && head.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
                || head.starts_with("week ")
                || head.starts_with("phase ")
                || head.starts_with("20") && head.chars().take(4).all(|c| c.is_ascii_digit())
        })
        .count();
    if dated >= 3 && dated == s.bullets.len() && s.bullets.len() <= 6 {
        return LayoutKind::Timeline;
    }
    if t.contains("agenda")
        || t.contains("overview")
            && s.bullets.len() >= 3
            && s.bullets.iter().all(|b| word_count(b) <= 6)
    {
        return LayoutKind::Agenda;
    }
    if (3..=4).contains(&s.bullets.len()) && s.bullets.iter().all(|b| word_count(b) <= 14) {
        return LayoutKind::Cards;
    }
    if s.bullets.len() >= 7 {
        return LayoutKind::TwoColumn;
    }
    LayoutKind::Bullets
}

/// Everything about how a deck looks and behaves, apart from its content.
#[derive(Debug, Clone, PartialEq)]
pub struct Design {
    /// A theme id from the catalogue (the design: layout and shape).
    pub theme: String,
    /// A palette id that recolours the theme's design; `None` keeps the
    /// theme's own colours.
    pub palette: Option<String>,
    pub motion: super::motion::MotionPreset,
    /// Right-to-left deck (Arabic, Hebrew): mirrored layouts and alignment.
    pub rtl: bool,
    /// Footer text on content slides; defaults to the deck title.
    pub footer: Option<String>,
    pub slide_numbers: bool,
    /// Reveal bullet lists one at a time on click instead of automatically.
    pub progressive_bullets: bool,
}

impl Default for Design {
    fn default() -> Self {
        Design {
            theme: "harbor".into(),
            palette: None,
            motion: super::motion::MotionPreset::Subtle,
            rtl: false,
            footer: None,
            slide_numbers: true,
            progressive_bullets: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DesignedDeck {
    pub title: String,
    pub subtitle: Option<String>,
    pub slides: Vec<DesignedSlide>,
    pub design: Design,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(title: &str, bullets: &[&str]) -> DesignedSlide {
        DesignedSlide::new(title).bullets(bullets.iter().copied())
    }

    #[test]
    fn inference_follows_the_documented_rule_order() {
        use LayoutKind::*;
        let total = 8;
        assert_eq!(infer_layout(&s("Deck", &[]), 0, total), Cover);
        assert_eq!(
            infer_layout(
                &s("Growth", &["38% faster", "2.1x pipeline", "$4.2M ARR"]),
                2,
                total
            ),
            Stats
        );
        assert_eq!(
            infer_layout(
                &s(
                    "Plan",
                    &[
                        "Q1 2026 launch beta",
                        "Q2 2026 go to market",
                        "Q3 2026 scale"
                    ]
                ),
                3,
                total
            ),
            Timeline
        );
        assert_eq!(
            infer_layout(
                &s("Build vs buy", &["Faster start", "More control"]),
                3,
                total
            ),
            Comparison
        );
        assert_eq!(
            infer_layout(
                &s("Why us", &["Fast delivery", "Fair pricing", "Local team"]),
                3,
                total
            ),
            Cards
        );
        assert_eq!(
            infer_layout(&s("Thank you", &["hello@harbor.dev"]), 7, total),
            Closing
        );
        assert_eq!(
            infer_layout(&s("Voice", &["\u{201c}It just works.\u{201d}"]), 4, total),
            Quote
        );
        assert_eq!(infer_layout(&s("Market", &[]), 3, total), Section);
        assert_eq!(
            infer_layout(
                &s(
                    "We exist to make every office document trustworthy and local",
                    &[]
                ),
                3,
                total
            ),
            Statement
        );
        let long: Vec<String> = (0..8)
            .map(|i| format!("Point number {i} with some words to read"))
            .collect();
        let long_refs: Vec<&str> = long.iter().map(String::as_str).collect();
        assert_eq!(infer_layout(&s("Notes", &long_refs), 3, total), TwoColumn);
        assert_eq!(
            infer_layout(
                &s(
                    "Findings",
                    &[
                        "The pipeline slowed in the second half of the year because deals stalled",
                        "Churn improved but only in the enterprise segment of the base"
                    ]
                ),
                3,
                total
            ),
            Bullets
        );
        // An explicit layout always wins.
        assert_eq!(
            s("Anything", &["a", "b", "c"])
                .layout(Timeline)
                .resolved_layout(3, total),
            Timeline
        );
    }

    #[test]
    fn stat_splitting_handles_currency_percent_and_junk() {
        assert_eq!(
            split_stat("38% churn reduction"),
            Some(("38%".into(), "churn reduction".into()))
        );
        assert_eq!(
            split_stat("$4.2M ARR"),
            Some(("$4.2M".into(), "ARR".into()))
        );
        assert_eq!(
            split_stat("2.1x — pipeline"),
            Some(("2.1x".into(), "pipeline".into()))
        );
        assert_eq!(split_stat("Revenue grew"), None);
        assert_eq!(
            split_stat("1234567890123456 something"),
            None,
            "a figure that long is not a headline"
        );
    }

    #[test]
    fn layout_ids_round_trip() {
        for k in LayoutKind::ALL {
            assert_eq!(LayoutKind::parse(k.id()), Some(k));
        }
        assert_eq!(LayoutKind::parse("nope"), None);
    }
}
