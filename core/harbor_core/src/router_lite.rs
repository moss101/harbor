//! Skill routing without an embedding model.
//!
//! The semantic router ([`crate::router`]) needs an embedder, and a phone
//! that has only a chat model installed has none, so every task request
//! used to dead-end in a "choose a skill yourself" list. This router works
//! from what every install has:
//!
//! 1. **Keywords.** A request that names the thing a skill makes ("slides",
//!    "spreadsheet", "CV", "email") picks that skill when exactly one skill
//!    matches. Deterministic, instant, and no model can misread it.
//! 2. **The chat model.** Otherwise the user's chat model picks one of the
//!    skills that fit the context, or "none", under a constrained enum
//!    schema; the answer is validated against the offered ids either way.
//!
//! The security rule is the semantic router's: the result is a
//! recommendation that opens a run sheet and nothing else. A run still
//! passes the executor's admission and approval gates.

use harbor_inference::provider::{ModelProvider, ModelRef};

use crate::router::{Disambiguation, RouteHit};
use crate::skills::SkillManifest;

/// How a lite route was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteVia {
    /// Exactly one skill's keywords matched.
    Keywords,
    /// The chat model chose.
    Chat,
    /// Nothing decided (no chat model, or its answer was unusable).
    Undecided,
}

impl LiteVia {
    pub fn as_str(self) -> &'static str {
        match self {
            LiteVia::Keywords => "keywords",
            LiteVia::Chat => "chat",
            LiteVia::Undecided => "undecided",
        }
    }
}

/// The outcome of [`route`].
#[derive(Debug)]
pub struct LiteRoute {
    /// The recommended skill, first in `ranked` when present.
    pub chosen: Option<String>,
    /// The chat model said no skill fits: answer in chat instead.
    pub none_fit: bool,
    /// Candidates considered, the chosen one first.
    pub ranked: Vec<RouteHit>,
    pub via: LiteVia,
    /// The chat model's raw outcome, when it was asked.
    pub llm: Option<Disambiguation>,
}

/// Phrases that name what a skill makes or works on. Matched on whole
/// words after lower-casing, so "deck" does not match "decked". Whether a
/// file is attached is settled by [`fits`] before any cue is read.
struct Cue {
    skill: &'static str,
    phrases: &'static [&'static str],
}

const CUES: &[Cue] = &[
    Cue {
        skill: "presentation-builder",
        phrases: &[
            "slides",
            "slide",
            "slide deck",
            "deck",
            "presentation",
            "pptx",
            "ppt",
            "powerpoint",
            "pitch deck",
            "keynote",
            "slideshow",
            "عرض تقديمي",
            "عرضا تقديميا",
            "تقديمي",
            "تقديميا",
            "تقديمية",
            "شرائح",
            "بوربوينت",
        ],
    },
    Cue {
        skill: "report-to-slides",
        phrases: &[
            "slides",
            "slide",
            "deck",
            "presentation",
            "pptx",
            "powerpoint",
            "شرائح",
        ],
    },
    Cue {
        skill: "sheet-builder",
        phrases: &[
            "spreadsheet",
            "excel",
            "xlsx",
            "workbook",
            "budget",
            "tracker",
            "expense sheet",
            "جدول بيانات",
            "إكسل",
            "اكسل",
            "ميزانية",
        ],
    },
    Cue {
        skill: "email-drafting",
        phrases: &[
            "email",
            "e-mail",
            "mail",
            "reply to",
            "respond to",
            "thank-you note",
            "thank you note",
            "بريد",
            "إيميل",
            "ايميل",
        ],
    },
    Cue {
        skill: "document-drafter",
        phrases: &[
            "cv",
            "resume",
            "résumé",
            "curriculum vitae",
            "cover letter",
            "letter",
            "proposal",
            "report",
            "memo",
            "docx",
            "word document",
            "word doc",
            "resignation",
            "write-up",
            "one-pager",
            "one pager",
            "سيرة ذاتية",
            "رسالة",
            "خطاب",
            "تقرير",
            "مقترح",
        ],
    },
    Cue {
        skill: "meeting-notes",
        phrases: &[
            "meeting notes",
            "minutes",
            "transcript",
            "standup call",
            "standup",
            "stand-up",
            "notes from this call",
            "call notes",
            "notes from the meeting",
            "محضر",
            "محضر الاجتماع",
        ],
    },
    Cue {
        skill: "team-update",
        phrases: &[
            "team update",
            "status update",
            "weekly update",
            "3p",
            "newsletter",
            "تحديث الفريق",
        ],
    },
    Cue {
        skill: "thread-summary",
        phrases: &["email thread", "thread", "conversation thread", "سلسلة"],
    },
    Cue {
        skill: "supplier-invoice-review",
        phrases: &[
            "invoice",
            "invoices",
            "overdue",
            "unpaid bills",
            "فاتورة",
            "فواتير",
        ],
    },
    Cue {
        skill: "second-look",
        phrases: &[
            "second look",
            "double check",
            "double-check",
            "fact check",
            "sanity check",
        ],
    },
    Cue {
        skill: "formula-audit",
        phrases: &[
            "formula",
            "formulas",
            "#ref",
            "#div/0",
            "broken formulas",
            "صيغ",
        ],
    },
    Cue {
        skill: "table-cleanup",
        phrases: &[
            "clean",
            "cleanup",
            "clean up",
            "tidy",
            "duplicates",
            "تنظيف",
        ],
    },
    Cue {
        skill: "placeholder-fill",
        phrases: &[
            "fill",
            "placeholder",
            "placeholders",
            "fill in",
            "form",
            "املأ",
        ],
    },
    Cue {
        skill: "financial-model-review",
        phrases: &["financial model", "model review", "النموذج المالي"],
    },
    Cue {
        skill: "deck-review",
        phrases: &[
            "review this deck",
            "review the deck",
            "deck review",
            "check the slides",
        ],
    },
    Cue {
        skill: "document-style-review",
        phrases: &[
            "style review",
            "typography",
            "layout review",
            "formatting review",
        ],
    },
];

/// Whether `skill` can open a run sheet in this context: graph skills
/// only; with a file, those that take one; without, those that do not
/// require one. Mirrors the app's `RequestRouter.fits`.
pub fn fits(skill: &SkillManifest, has_file: bool) -> bool {
    let Some(graph) = &skill.graph else {
        return false;
    };
    let takes_file = graph
        .inputs
        .get("properties")
        .and_then(|p| p.get("artifact_id"))
        .is_some();
    let needs_file = graph
        .inputs
        .get("required")
        .and_then(|r| r.as_array())
        .is_some_and(|r| r.iter().any(|v| v.as_str() == Some("artifact_id")));
    if has_file {
        takes_file
    } else {
        !needs_file
    }
}

/// ` word word ` form for whole-word phrase search.
fn normalize(text: &str) -> String {
    let mut out = String::from(" ");
    let mut last_space = true;
    for ch in text.chars().flat_map(char::to_lowercase) {
        let keep = ch.is_alphanumeric() || matches!(ch, '#' | '/' | '-');
        if keep {
            out.push(ch);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    if !last_space {
        out.push(' ');
    }
    out
}

/// Skills (among `pool`) whose cues appear in `request`, in `CUES` order.
pub fn keyword_matches(request: &str, pool: &[&SkillManifest]) -> Vec<String> {
    let text = normalize(request);
    let mut hits = Vec::new();
    for cue in CUES {
        if !pool.iter().any(|s| s.id == cue.skill) {
            continue;
        }
        let hit = cue.phrases.iter().any(|p| text.contains(&normalize(p)));
        if hit {
            hits.push(cue.skill.to_string());
        }
    }
    hits
}

/// What each skill makes, in the words people use when they ask for it.
/// A 2B model picks far better from these lines than from descriptions
/// written for the Skills screen.
const MAKES: &[(&str, &str)] = &[
    ("presentation-builder", "a slide deck or presentation (PowerPoint) for a talk, pitch or meeting"),
    ("email-drafting", "an email or message to someone: to tell, ask, inform, thank or reply"),
    ("document-drafter", "a Word document: a letter (cover, resignation, complaint), CV or resume, proposal, report or memo"),
    ("sheet-builder", "an Excel spreadsheet: a budget, tracker, plan or table with totals"),
    ("meeting-notes", "notes from a meeting or call: decisions, owners, actions"),
    ("team-update", "a weekly team status update: progress, plans, problems"),
    ("thread-summary", "a summary of an email thread the user pasted"),
    ("supplier-invoice-review", "a review of invoices or bills: which are unpaid or overdue"),
    ("report-to-slides", "slides summarising the attached report"),
    ("doc-coauthoring", "a guided rewrite of the attached document"),
    ("placeholder-fill", "the attached document with its blanks or placeholders filled in"),
    ("deck-review", "a quality review of the attached slide deck"),
    ("document-style-review", "a typography and layout review of the attached document"),
    ("financial-model-review", "an audit of the attached financial model workbook"),
    ("formula-audit", "repaired formulas in the attached workbook"),
    ("table-cleanup", "a cleaned table in the attached workbook"),
];

/// Skills the chat model is not offered: they are reached by asking for
/// them by name. Second Look ("check this answer") otherwise becomes the
/// small model's catch-all for anything vague.
const BY_NAME_ONLY: &[&str] = &["second-look"];

/// The first sentence of a description: small models choose better from
/// short option lines than from paragraphs.
fn first_sentence(text: &str) -> &str {
    let end = text
        .char_indices()
        .find(|&(i, c)| c == '.' && text[i + 1..].starts_with(' '))
        .map(|(i, _)| i + 1)
        .unwrap_or(text.len());
    &text[..end]
}

fn hit(s: &SkillManifest) -> RouteHit {
    let makes = MAKES
        .iter()
        .find(|(id, _)| *id == s.id)
        .map(|(_, m)| format!("makes {m}"))
        .unwrap_or_else(|| first_sentence(&s.description).to_string());
    RouteHit {
        skill_id: s.id.clone(),
        title: s.title.clone(),
        description: makes,
        score: 0.0,
    }
}

/// Verbs of making. A chat-model pick stands only when the request asks
/// for something to be made: Qwen2.5-1.5B otherwise opened Document
/// Drafter for "recommend a book" and Email Drafting for "tell me a joke".
const MAKING: &[&str] = &[
    "write",
    "draft",
    "make",
    "create",
    "prepare",
    "build",
    "compose",
    "generate",
    "design",
    "produce",
    "put together",
    "plan",
    "track",
    "send",
    "reply",
    "respond",
    "turn",
    "convert this",
    "summarise",
    "summarize",
    "recap",
    "let",
    "notify",
    "inform",
    "ask",
    "invite",
    "request",
    "need",
    "want",
    "set up",
    "اكتب",
    "أكتب",
    "أنشئ",
    "انشئ",
    "اعمل",
    "جهز",
    "حضر",
    "صمم",
    "أرسل",
    "ارسل",
    "لخص",
];

fn asks_to_make(request: &str) -> bool {
    let text = normalize(request);
    MAKING.iter().any(|v| text.contains(&normalize(v)))
}

/// Ask the chat model twice, the options in opposite orders, and keep a
/// skill only when both answers name it. Small models lean toward an
/// option's position (Qwen2.5-1.5B picked the first-listed document
/// skill for "recommend a book"); a pick that survives reordering is the
/// model's judgment, not its bias. Any disagreement answers in chat,
/// which is never worse than opening the wrong run sheet.
fn agreed_choice(
    provider: &dyn ModelProvider,
    model: &ModelRef,
    request: &str,
    offered: &[RouteHit],
) -> Disambiguation {
    let first = crate::router::choose(provider, model, request, offered);
    let Disambiguation::Chose { skill_id } = &first else {
        return first;
    };
    if offered.len() < 2 {
        return first;
    }
    let reversed: Vec<RouteHit> = offered.iter().rev().cloned().collect();
    match crate::router::choose(provider, model, request, &reversed) {
        Disambiguation::Chose { skill_id: again } if &again == skill_id => first,
        Disambiguation::Unavailable(why) => Disambiguation::Unavailable(why),
        _ => Disambiguation::NoneFit,
    }
}

/// Route `request` without an embedder. `chat` is the user's chat model
/// when one is installed. `has_file`: a file is attached to the request.
pub fn route(
    chat: Option<(&dyn ModelProvider, &ModelRef)>,
    request: &str,
    skills: &[SkillManifest],
    has_file: bool,
) -> LiteRoute {
    let pool: Vec<&SkillManifest> = skills.iter().filter(|s| fits(s, has_file)).collect();
    let by_id = |id: &str| pool.iter().find(|s| s.id == id).copied();
    let keywords = keyword_matches(request, &pool);
    if let [only] = keywords.as_slice() {
        let mut ranked = vec![hit(by_id(only).expect("matched from pool"))];
        ranked.extend(pool.iter().filter(|s| &s.id != only).map(|s| hit(s)));
        return LiteRoute {
            chosen: Some(only.clone()),
            none_fit: false,
            ranked,
            via: LiteVia::Keywords,
            llm: None,
        };
    }
    // Several keyword hits narrow the choice; none leaves every skill
    // that fits in play.
    let offered: Vec<RouteHit> = if keywords.is_empty() {
        pool.iter()
            .filter(|s| !BY_NAME_ONLY.contains(&s.id.as_str()))
            .map(|s| hit(s))
            .collect()
    } else {
        keywords
            .iter()
            .filter_map(|id| by_id(id))
            .map(hit)
            .collect()
    };
    let Some((provider, model)) = chat else {
        return LiteRoute {
            chosen: None,
            none_fit: false,
            ranked: offered,
            via: LiteVia::Undecided,
            llm: None,
        };
    };
    let mut outcome = agreed_choice(provider, model, request, &offered);
    if matches!(outcome, Disambiguation::Chose { .. }) && !asks_to_make(request) {
        outcome = Disambiguation::NoneFit;
    }
    let mut ranked = offered;
    match &outcome {
        Disambiguation::Chose { skill_id } => {
            if let Some(pos) = ranked.iter().position(|h| &h.skill_id == skill_id) {
                let first = ranked.remove(pos);
                ranked.insert(0, first);
            }
            LiteRoute {
                chosen: Some(skill_id.clone()),
                none_fit: false,
                ranked,
                via: LiteVia::Chat,
                llm: Some(outcome),
            }
        }
        Disambiguation::NoneFit => LiteRoute {
            chosen: None,
            none_fit: true,
            ranked,
            via: LiteVia::Chat,
            llm: Some(outcome),
        },
        Disambiguation::Unavailable(_) => LiteRoute {
            chosen: None,
            none_fit: false,
            ranked,
            via: LiteVia::Undecided,
            llm: Some(outcome),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skills() -> Vec<SkillManifest> {
        crate::skills::builtin_skills().unwrap()
    }

    fn pick(request: &str, has_file: bool) -> Option<String> {
        route(None, request, &skills(), has_file).chosen
    }

    #[test]
    fn a_named_output_picks_its_skill_without_any_model() {
        assert_eq!(
            pick("create a cv for me in pdf", false).as_deref(),
            Some("document-drafter")
        );
        assert_eq!(
            pick("make slides about our Q3 launch", false).as_deref(),
            Some("presentation-builder")
        );
        assert_eq!(
            pick("build a budget spreadsheet for a wedding", false).as_deref(),
            Some("sheet-builder")
        );
        assert_eq!(
            pick("draft an email to Sara about the delay", false).as_deref(),
            Some("email-drafting")
        );
        assert_eq!(
            pick("اكتب سيرة ذاتية لمهندس", false).as_deref(),
            Some("document-drafter")
        );
        assert_eq!(
            pick("أنشئ عرضا تقديميا عن الطاقة الشمسية", false).as_deref(),
            Some("presentation-builder")
        );
        // A meeting mentioned in passing is not a request for minutes.
        assert_eq!(
            pick("I need a ppt for tomorrow's board meeting", false).as_deref(),
            Some("presentation-builder")
        );
        // Writing to a supplier is an email, not an invoice review.
        assert_eq!(
            pick("ask the supplier for a refund on order 4471", false),
            None
        );
    }

    #[test]
    fn a_file_narrows_to_skills_that_take_one() {
        assert_eq!(
            pick("turn this into slides", true).as_deref(),
            Some("report-to-slides")
        );
        assert_eq!(
            pick("fix the broken formulas", true).as_deref(),
            Some("formula-audit")
        );
    }

    #[test]
    fn whole_words_only_and_no_guess_without_a_model() {
        // "mail" inside "Gmail" is not a cue; nothing else names a skill.
        assert_eq!(pick("tell me a joke about Gmail", false), None);
        // Two skills named: no model, no pick.
        let r = route(None, "email me the slides", &skills(), false);
        assert_eq!(r.chosen, None);
        assert_eq!(r.via, LiteVia::Undecided);
        assert_eq!(r.ranked.len(), 2);
    }

    #[test]
    fn second_look_is_reached_by_name_only() {
        assert_eq!(
            pick("double check this answer: Paris is in Spain", false).as_deref(),
            Some("second-look")
        );
        let r = route(None, "hello", &skills(), false);
        assert!(r.ranked.iter().all(|h| h.skill_id != "second-look"));
    }

    #[test]
    fn a_model_pick_needs_a_request_to_make_something() {
        assert!(asks_to_make("let my manager know I'm off sick"));
        assert!(asks_to_make("أنشئ جدولا"));
        assert!(!asks_to_make("tell me a joke"));
        assert!(!asks_to_make("recommend a book about habits"));
        // Whole words: "rewrite" is not "write".
        assert!(!asks_to_make("rewrite this sentence to sound friendlier"));
    }

    #[test]
    fn option_lines_are_one_sentence() {
        assert_eq!(first_sentence("Build a deck. Then more."), "Build a deck.");
        assert_eq!(first_sentence("v1.2 is fine"), "v1.2 is fine");
    }
}
