//! Purposes: what a deck is *for*. A purpose carries a storyline (the
//! slide-by-slide arc that audience expects), a default theme and motion,
//! and a rule for mapping a slide's content onto the layout that suits its
//! role in the story. Skills use the storyline to brief the model that
//! writes the outline; the design engine uses the roles to pick layouts.

use super::model::{closing_like, split_stat, DesignedDeck, DesignedSlide, LayoutKind};
use super::motion::MotionPreset;

/// One beat of a storyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// Short label shown above the slide title.
    pub kicker: &'static str,
    /// The layout this beat usually takes when the content supports it.
    pub layout: LayoutKind,
    /// What the slide must say (briefs the outline writer).
    pub says: &'static str,
}

const fn b(kicker: &'static str, layout: LayoutKind, says: &'static str) -> Beat {
    Beat {
        kicker,
        layout,
        says,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PurposeSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub theme: &'static str,
    /// Other themes that suit it, best first.
    pub alt_themes: &'static [&'static str],
    pub motion: MotionPreset,
    /// Reveal bullets one at a time on click (teaching, workshops).
    pub progressive: bool,
    pub beats: &'static [Beat],
}

use LayoutKind as L;
use MotionPreset as M;

pub const PURPOSES: &[PurposeSpec] = &[
    PurposeSpec {
        id: "pitch_deck",
        name: "Pitch deck",
        summary: "Raise a round: problem, solution, market, traction, team, ask.",
        theme: "aurora",
        alt_themes: &["obsidian", "midnight_gold"],
        motion: M::Cinematic,
        progressive: false,
        beats: &[
            b("", L::Cover, "Company name and the one-line promise."),
            b(
                "Problem",
                L::Statement,
                "The pain, in one sentence the audience feels.",
            ),
            b(
                "Solution",
                L::Cards,
                "What you built, as three short ideas.",
            ),
            b(
                "Product",
                L::ImageSplit,
                "How it works; what the user sees.",
            ),
            b(
                "Market",
                L::Stats,
                "Market size and the wedge, as headline figures.",
            ),
            b(
                "Business model",
                L::Cards,
                "Who pays, how, and why it compounds.",
            ),
            b("Traction", L::Chart, "Growth that proves demand."),
            b(
                "Competition",
                L::Comparison,
                "Why you win against the alternative.",
            ),
            b(
                "Go to market",
                L::Timeline,
                "How customers find you, in order.",
            ),
            b("Team", L::Team, "Who you are and why you."),
            b("Financials", L::Stats, "Revenue, burn, runway."),
            b("The ask", L::Stats, "How much, and what it buys."),
            b("", L::Closing, "Contact and the next step."),
        ],
    },
    PurposeSpec {
        id: "investor_update",
        name: "Investor update",
        summary: "Periodic update: highlights, metrics, progress, asks.",
        theme: "ivory",
        alt_themes: &["midnight_gold", "harbor"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Company, period and date."),
            b(
                "Highlights",
                L::Stats,
                "The four numbers that matter this period.",
            ),
            b("Metrics", L::Chart, "The trend behind the headline."),
            b("Product", L::Cards, "What shipped and what it unlocked."),
            b("Customers", L::Quote, "A customer voice that proves it."),
            b("Financials", L::Stats, "Cash, burn, runway."),
            b("Team", L::Team, "Hires and changes."),
            b(
                "Risks and asks",
                L::Comparison,
                "What worries you; where investors can help.",
            ),
            b("Next quarter", L::Timeline, "The priorities, in order."),
            b("", L::Closing, "Thanks and contact."),
        ],
    },
    PurposeSpec {
        id: "annual_report",
        name: "Annual report",
        summary: "A year in review for stakeholders.",
        theme: "ivory",
        alt_themes: &["midnight_gold", "forest"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "The year and the theme."),
            b("Agenda", L::Agenda, "What this report covers."),
            b("Letter", L::Quote, "The leadership message in a sentence."),
            b("Highlights", L::Stats, "The year in four figures."),
            b("Financials", L::Chart, "Revenue and margin over time."),
            b("Strategy", L::Cards, "The pillars and progress on each."),
            b("Governance", L::Bullets, "Board, risk and compliance."),
            b("Outlook", L::Timeline, "What the next year holds."),
            b("", L::Closing, "Thanks and contact."),
        ],
    },
    PurposeSpec {
        id: "board",
        name: "Board briefing",
        summary: "Decisions first: what the board must know and approve.",
        theme: "midnight_gold",
        alt_themes: &["ivory", "harbor"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Meeting and date."),
            b("Agenda", L::Agenda, "The order of business."),
            b("Decisions", L::Cards, "The decisions required today."),
            b("Summary", L::Bullets, "The executive summary."),
            b("KPIs", L::Stats, "Performance against plan."),
            b("Financials", L::Chart, "The numbers behind the decisions."),
            b("Risks", L::Comparison, "Exposure and mitigation."),
            b("Resolutions", L::Bullets, "The resolutions for approval."),
            b("", L::Closing, "Next meeting and contact."),
        ],
    },
    PurposeSpec {
        id: "project_report",
        name: "Project report",
        summary: "Where the project stands: scope, milestones, progress, risks, budget.",
        theme: "slate",
        alt_themes: &["harbor", "blueprint"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Project, period and owner."),
            b("Summary", L::Bullets, "Status in three sentences."),
            b("Scope", L::Cards, "Goals and what is in and out."),
            b("Milestones", L::Timeline, "Delivered, due, upcoming."),
            b(
                "Progress",
                L::Stats,
                "Percent complete, on-time, on-budget.",
            ),
            b("Risks", L::Comparison, "Risks and issues, with mitigation."),
            b("Budget", L::Chart, "Spend against plan."),
            b("Next steps", L::Agenda, "What happens next, in order."),
            b("Decisions", L::Bullets, "What you need from the sponsor."),
            b("", L::Closing, "Owner and contact."),
        ],
    },
    PurposeSpec {
        id: "status_update",
        name: "Status update",
        summary: "A short weekly update.",
        theme: "slate",
        alt_themes: &["harbor"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Team and week."),
            b("Summary", L::Stats, "The week in numbers."),
            b("Done", L::Bullets, "What shipped."),
            b("Next", L::Bullets, "What is next."),
            b("Risks", L::Comparison, "Blockers and asks."),
        ],
    },
    PurposeSpec {
        id: "product_launch",
        name: "Product launch",
        summary: "Introduce a product: insight, reveal, features, how it works, pricing, plan.",
        theme: "sunrise",
        alt_themes: &["aurora", "obsidian", "royal"],
        motion: M::Cinematic,
        progressive: false,
        beats: &[
            b("", L::Cover, "The product name and its promise."),
            b(
                "Insight",
                L::Statement,
                "The truth about the customer that led here.",
            ),
            b("Introducing", L::Section, "The reveal."),
            b("Features", L::Cards, "Three things it does best."),
            b(
                "How it works",
                L::Timeline,
                "Three steps from start to result.",
            ),
            b("See it", L::ImageSplit, "The product in use."),
            b("Pricing", L::Comparison, "Plans and what each includes."),
            b("Launch plan", L::Timeline, "Dates and channels."),
            b("Goals", L::Stats, "What success looks like."),
            b("", L::Closing, "Where to get it; the call to action."),
        ],
    },
    PurposeSpec {
        id: "quarterly_review",
        name: "Quarterly business review",
        summary: "Results, drivers, initiatives and outlook for a quarter.",
        theme: "harbor",
        alt_themes: &["slate", "ocean"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Quarter and business unit."),
            b("Agenda", L::Agenda, "What this review covers."),
            b("Summary", L::Bullets, "The quarter in three sentences."),
            b("KPIs", L::Stats, "Headline KPIs against target."),
            b("Revenue", L::Chart, "Revenue and its drivers."),
            b("Segments", L::Chart, "Where growth came from."),
            b("Initiatives", L::Cards, "Progress on the big bets."),
            b(
                "Risks",
                L::Comparison,
                "What could derail the next quarter.",
            ),
            b("Outlook", L::Timeline, "Priorities and milestones ahead."),
            b("", L::Closing, "Questions and contact."),
        ],
    },
    PurposeSpec {
        id: "client_proposal",
        name: "Client proposal",
        summary: "Win the work: understanding, approach, deliverables, plan, investment.",
        theme: "ocean",
        alt_themes: &["terracotta", "harbor"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Client, project and date."),
            b(
                "Understanding",
                L::Bullets,
                "Their situation, in their words.",
            ),
            b("Approach", L::Cards, "How you will solve it."),
            b("Deliverables", L::Cards, "What they receive."),
            b("Timeline", L::Timeline, "Phases and dates."),
            b("Team", L::Team, "Who does the work."),
            b("Investment", L::Comparison, "Options and prices."),
            b("Why us", L::Stats, "Proof you can deliver."),
            b("Next steps", L::Agenda, "How we start."),
            b("", L::Closing, "Contact and signature."),
        ],
    },
    PurposeSpec {
        id: "strategy",
        name: "Strategy",
        summary: "Where to play and how to win: vision, pillars, priorities, investment.",
        theme: "ocean",
        alt_themes: &["blueprint", "harbor"],
        motion: M::Dynamic,
        progressive: false,
        beats: &[
            b("", L::Cover, "The strategy and horizon."),
            b("Context", L::Bullets, "What changed and why now."),
            b("Vision", L::Statement, "The destination in one sentence."),
            b("Pillars", L::Cards, "The three to four bets."),
            b("Priorities", L::Timeline, "What happens first."),
            b("Investment", L::Chart, "Where the money goes."),
            b("Risks", L::Comparison, "What could go wrong; how to hedge."),
            b(
                "Governance",
                L::Bullets,
                "Who decides and how it is tracked.",
            ),
            b("", L::Closing, "Next step and owner."),
        ],
    },
    PurposeSpec {
        id: "marketing_plan",
        name: "Marketing plan",
        summary: "Objectives, audience, channels, calendar, budget, KPIs.",
        theme: "sunrise",
        alt_themes: &["royal", "playground"],
        motion: M::Dynamic,
        progressive: false,
        beats: &[
            b("", L::Cover, "Campaign and period."),
            b("Objectives", L::Stats, "What the plan must achieve."),
            b("Audience", L::Cards, "Who we are talking to."),
            b("Channels", L::Cards, "Where we reach them."),
            b("Calendar", L::Timeline, "What goes live, when."),
            b("Budget", L::Chart, "Spend by channel."),
            b("KPIs", L::Stats, "How we will measure it."),
            b("", L::Closing, "Owner and next step."),
        ],
    },
    PurposeSpec {
        id: "training",
        name: "Training",
        summary: "Teach a topic: objectives, concepts, practice, recap.",
        theme: "playground",
        alt_themes: &["forest", "harbor"],
        motion: M::Dynamic,
        progressive: true,
        beats: &[
            b("", L::Cover, "Course and audience."),
            b("Agenda", L::Agenda, "What we will cover."),
            b("Objectives", L::Cards, "What you will be able to do."),
            b("Concept", L::Bullets, "The idea, simply."),
            b("Example", L::ImageSplit, "A worked example."),
            b("Practice", L::Statement, "The exercise, in one line."),
            b("Recap", L::Bullets, "The three takeaways."),
            b("", L::Closing, "Resources and questions."),
        ],
    },
    PurposeSpec {
        id: "workshop",
        name: "Workshop",
        summary: "A facilitated session: goals, rules, activities, debrief.",
        theme: "playground",
        alt_themes: &["sunrise", "forest"],
        motion: M::Dynamic,
        progressive: true,
        beats: &[
            b("", L::Cover, "Workshop title and facilitator."),
            b("Agenda", L::Agenda, "The flow of the session."),
            b("Goals", L::Cards, "What we leave with."),
            b("Ground rules", L::Bullets, "How we work together."),
            b("Activity", L::Statement, "The prompt for the exercise."),
            b("Debrief", L::Bullets, "What we noticed."),
            b("Next steps", L::Timeline, "What happens after today."),
            b("", L::Closing, "Thanks and contact."),
        ],
    },
    PurposeSpec {
        id: "case_study",
        name: "Case study",
        summary: "A customer story: challenge, approach, solution, results.",
        theme: "terracotta",
        alt_themes: &["mono", "ocean"],
        motion: M::Dynamic,
        progressive: false,
        beats: &[
            b("", L::Cover, "Customer and headline result."),
            b("Challenge", L::Bullets, "What they were up against."),
            b("Approach", L::Cards, "What we did differently."),
            b("Solution", L::ImageSplit, "What we built, shown."),
            b("Results", L::Stats, "The measurable outcome."),
            b("In their words", L::Quote, "The customer's voice."),
            b("Lessons", L::Bullets, "What carries over."),
            b("", L::Closing, "Contact for a similar result."),
        ],
    },
    PurposeSpec {
        id: "all_hands",
        name: "All-hands",
        summary: "Company update: wins, spotlight, roadmap, people, Q&A.",
        theme: "sunrise",
        alt_themes: &["playground", "harbor"],
        motion: M::Dynamic,
        progressive: false,
        beats: &[
            b("", L::Cover, "Meeting and date."),
            b("Wins", L::Stats, "What we achieved."),
            b("Spotlight", L::Quote, "Someone worth celebrating."),
            b("Roadmap", L::Timeline, "What is coming."),
            b("People", L::Team, "Who joined and who moved."),
            b("", L::Closing, "Q and A."),
        ],
    },
    PurposeSpec {
        id: "keynote",
        name: "Keynote talk",
        summary: "One idea at a time: statements, a quote, a number, a close.",
        theme: "obsidian",
        alt_themes: &["mono", "aurora"],
        motion: M::Cinematic,
        progressive: false,
        beats: &[
            b("", L::Cover, "Title of the talk and speaker."),
            b("", L::Statement, "The provocation."),
            b("", L::Statement, "The turn."),
            b("", L::Stats, "The number that proves it."),
            b("", L::Quote, "A voice from outside."),
            b("", L::Statement, "What to do now."),
            b("", L::Closing, "Thank you and where to follow."),
        ],
    },
    PurposeSpec {
        id: "technical_review",
        name: "Technical review",
        summary: "Design review: context, goals, architecture, trade-offs, rollout.",
        theme: "blueprint",
        alt_themes: &["slate", "harbor"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "System and author."),
            b("Context", L::Bullets, "The problem and constraints."),
            b("Goals", L::Comparison, "Goals and non-goals."),
            b("Architecture", L::ImageSplit, "The design, drawn."),
            b(
                "Trade-offs",
                L::Comparison,
                "Options considered and why this one.",
            ),
            b("Risks", L::Cards, "What can fail and how we detect it."),
            b("Rollout", L::Timeline, "Phases and checkpoints."),
            b("Open questions", L::Bullets, "What needs a decision."),
            b("", L::Closing, "Reviewers and contact."),
        ],
    },
    PurposeSpec {
        id: "impact_report",
        name: "Impact report",
        summary: "For donors and partners: mission, impact, stories, finances.",
        theme: "forest",
        alt_themes: &["ivory", "terracotta"],
        motion: M::Dynamic,
        progressive: false,
        beats: &[
            b("", L::Cover, "Organisation and year."),
            b("Mission", L::Statement, "Why we exist."),
            b("Impact", L::Stats, "The lives changed, in numbers."),
            b("Stories", L::Quote, "A person, in their words."),
            b("Programmes", L::Cards, "What we run."),
            b("Finances", L::Chart, "Where every pound goes."),
            b("", L::Closing, "Thank you and how to help."),
        ],
    },
    PurposeSpec {
        id: "research",
        name: "Research findings",
        summary: "Question, method, findings, discussion, conclusions.",
        theme: "mono",
        alt_themes: &["forest", "terracotta"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Title, authors, venue."),
            b("Question", L::Statement, "What we set out to learn."),
            b("Method", L::Cards, "How we studied it."),
            b("Findings", L::Chart, "What the data show."),
            b(
                "Discussion",
                L::Bullets,
                "What it means and what it does not.",
            ),
            b("Conclusions", L::Cards, "The takeaways."),
            b("Next", L::Timeline, "Follow-up work."),
            b("", L::Closing, "Contact and references."),
        ],
    },
    PurposeSpec {
        id: "general",
        name: "General presentation",
        summary: "A clean, flexible deck for anything else.",
        theme: "harbor",
        alt_themes: &["ocean", "slate", "ivory"],
        motion: M::Subtle,
        progressive: false,
        beats: &[
            b("", L::Cover, "Title and subtitle."),
            b("", L::Bullets, "The main points."),
            b("", L::Closing, "Contact."),
        ],
    },
];

pub fn purpose(id: &str) -> Option<&'static PurposeSpec> {
    PURPOSES.iter().find(|p| p.id == id)
}

/// Can `slide` honestly be shown with `kind`? A beat's preferred layout is
/// only used when the content supports it; otherwise inference decides.
fn supports(kind: LayoutKind, s: &DesignedSlide) -> bool {
    let items = if s.blocks.is_empty() {
        s.bullets.len()
    } else {
        s.blocks.len()
    };
    match kind {
        LayoutKind::Cover => true,
        // Only a slide that reads as a sign-off becomes the dark closing
        // slide; ordinary last-slide content ("Next steps") stays content.
        LayoutKind::Closing => closing_like(s),
        LayoutKind::Section | LayoutKind::Statement => s.bullets.len() <= 1 && s.blocks.is_empty(),
        LayoutKind::Bullets => true,
        LayoutKind::TwoColumn => items >= 2,
        LayoutKind::Comparison => items >= 2,
        LayoutKind::Cards => (2..=6).contains(&items),
        LayoutKind::Timeline => (2..=6).contains(&items),
        LayoutKind::Team => (1..=5).contains(&items),
        LayoutKind::Agenda => (2..=8).contains(&items),
        LayoutKind::Quote => !s.bullets.is_empty() && s.blocks.is_empty(),
        LayoutKind::Stats => {
            let from_blocks = !s.blocks.is_empty() && s.blocks.iter().all(|b| b.value.is_some());
            let from_bullets = s.blocks.is_empty()
                && (2..=4).contains(&s.bullets.len())
                && s.bullets.iter().all(|b| split_stat(b).is_some());
            from_blocks || from_bullets
        }
        LayoutKind::Chart => s.chart.is_some(),
        LayoutKind::ImageSplit => !s.bullets.is_empty() || s.image.is_some(),
    }
}

/// Dress a deck for a purpose: theme and motion (unless the caller set
/// them), kickers from the storyline, and beat-driven layouts where the
/// content supports them. Idempotent for already-styled slides.
pub fn apply(
    deck: &mut DesignedDeck,
    purpose_id: &str,
    keep_theme: bool,
    keep_motion: bool,
) -> bool {
    let Some(p) = purpose(purpose_id) else {
        return false;
    };
    if !keep_theme {
        deck.design.theme = p.theme.to_string();
    }
    if !keep_motion {
        deck.design.motion = p.motion;
        deck.design.progressive_bullets = p.progressive;
    }
    let n = deck.slides.len();
    for (i, s) in deck.slides.iter_mut().enumerate() {
        // The storyline's first beat is the cover and its last the close;
        // the middle beats map onto the middle slides in order, squeezed or
        // stretched evenly when the counts differ.
        let beat = if i == 0 {
            Some(&p.beats[0])
        } else if i + 1 == n && n > 1 {
            p.beats.last()
        } else if p.beats.len() > 2 {
            let mid = &p.beats[1..p.beats.len() - 1];
            let inner = n.saturating_sub(2).max(1);
            let at = ((i - 1) * mid.len()) / inner;
            mid.get(at.min(mid.len() - 1))
        } else {
            None
        };
        let Some(beat) = beat else { continue };
        if s.kicker.is_none() && !beat.kicker.is_empty() {
            s.kicker = Some(beat.kicker.to_string());
        }
        if s.layout.is_none() {
            let inferred = s.resolved_layout(i, n);
            // Keep strong inferences (chart, image, explicit blocks); let
            // the beat steer the generic ones.
            let generic = matches!(
                inferred,
                LayoutKind::Bullets | LayoutKind::Cards | LayoutKind::TwoColumn
            );
            // A Bullets beat does not steer: it accepts whatever the content
            // itself suggests (three short points are cards, figures are
            // stats). Only a beat naming a specific layout can override.
            let steers = !matches!(beat.layout, LayoutKind::Bullets | LayoutKind::Cover);
            if generic && steers && supports(beat.layout, s) {
                s.layout = Some(beat.layout);
            } else if i == 0 {
                s.layout = Some(LayoutKind::Cover);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::model::Block;
    use super::super::theme::{theme, THEMES};
    use super::*;

    #[test]
    fn every_purpose_is_coherent() {
        let mut ids: Vec<_> = PURPOSES.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PURPOSES.len(), "unique ids");
        assert!(PURPOSES.len() >= 18);
        for p in PURPOSES {
            let t = theme(p.theme).unwrap_or_else(|| panic!("{} default theme {}", p.id, p.theme));
            assert!(
                t.suits.contains(&p.id) || t.suits.contains(&"general"),
                "{}: default theme {} does not list it",
                p.id,
                t.id
            );
            for a in p.alt_themes {
                assert!(theme(a).is_some(), "{} alt {}", p.id, a);
            }
            assert_eq!(p.beats[0].layout, LayoutKind::Cover, "{}", p.id);
            assert!(
                p.beats.len() >= 3 && p.beats.iter().all(|b| !b.says.is_empty()),
                "{}",
                p.id
            );
        }
    }

    #[test]
    fn every_theme_suit_names_a_real_purpose() {
        for t in THEMES {
            for s in t.suits {
                assert!(purpose(s).is_some(), "{} suits unknown purpose {s}", t.id);
            }
        }
    }

    #[test]
    fn apply_sets_defaults_kickers_and_beat_layouts() {
        let mut deck = DesignedDeck {
            title: "Series A".into(),
            slides: vec![
                DesignedSlide::new("Harbor"),
                DesignedSlide::new("Offices lose control of their files").bullets(["One"]),
                DesignedSlide::new("Three promises").bullets([
                    "Local first",
                    "Verified math",
                    "One app",
                ]),
                DesignedSlide::new("Opportunity").bullets(["$4B market", "38% CAGR", "12M teams"]),
                DesignedSlide::new("Thanks").bullets(["hello@harbor.dev"]),
            ],
            ..Default::default()
        };
        assert!(apply(&mut deck, "pitch_deck", false, false));
        assert_eq!(deck.design.theme, "aurora");
        assert_eq!(deck.design.motion, MotionPreset::Cinematic);
        assert_eq!(deck.slides[0].layout, Some(LayoutKind::Cover));
        assert!(deck.slides[1].kicker.is_some());
        let kinds = deck.resolved_layouts();
        assert_eq!(kinds[0], LayoutKind::Cover);
        assert_eq!(
            kinds[4],
            LayoutKind::Closing,
            "the last slide takes the storyline's close"
        );
        assert_eq!(kinds[3], LayoutKind::Stats);
        assert!(!apply(&mut deck, "no_such_purpose", false, false));
    }

    #[test]
    fn caller_choices_survive() {
        let mut deck = DesignedDeck {
            slides: vec![
                DesignedSlide::new("A"),
                DesignedSlide::new("B").bullets(["x", "y", "z"]),
            ],
            ..Default::default()
        };
        deck.design.theme = "mono".into();
        deck.design.motion = MotionPreset::None;
        apply(&mut deck, "pitch_deck", true, true);
        assert_eq!(deck.design.theme, "mono");
        assert_eq!(deck.design.motion, MotionPreset::None);
    }

    #[test]
    fn beats_never_force_a_layout_the_content_cannot_fill() {
        // A Stats beat with prose bullets must not become a stats slide.
        let prose = DesignedSlide::new("Financials")
            .bullets(["Revenue is growing nicely", "Margins are healthy"]);
        assert!(!supports(LayoutKind::Stats, &prose));
        let figures =
            DesignedSlide::new("Financials").bullets(["$4.2M revenue", "62% gross margin"]);
        assert!(supports(LayoutKind::Stats, &figures));
        assert!(
            !supports(LayoutKind::Chart, &prose),
            "no chart data, no chart layout"
        );
        let blocks = DesignedSlide::new("Team").blocks(vec![Block::card("Layla", "CEO")]);
        assert!(supports(LayoutKind::Team, &blocks));
    }
}
