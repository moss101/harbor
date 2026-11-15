//! Exact answers for the questions small models get wrong but a few lines
//! of code get right every time: percentages, unit conversions, calendar
//! arithmetic, letter and word counts, sorting, reversing. A 1–2B model
//! answers "how many r's in strawberry" or "days until New Year" by
//! guessing tokens; Harbor computes them, costs no tokens, and says how.
//!
//! Each matcher accepts only a question it can answer completely. Anything
//! else returns `None` and goes to the model as before, so a near-miss
//! phrasing costs nothing but the model's usual answer.

use std::sync::LazyLock;

use chrono::{Datelike, Duration, NaiveDate};
use regex::Regex;

/// `^…$` regex, case-insensitive.
fn re(p: &str) -> Regex {
    Regex::new(&format!("(?i)^(?:{p})$")).expect("static regex")
}

const NUM: &str = r"(-?\d+(?:\.\d+)?)";

/// The exact answer to `question`, or `None` when no tool can answer all of
/// it. `today` anchors calendar questions.
pub fn answer(question: &str, today: NaiveDate) -> Option<String> {
    let q = clean(question);
    crate::calc::arithmetic_answer(question)
        .or_else(|| percent(&q))
        .or_else(|| units(&q))
        .or_else(|| dates(&q, today))
        .or_else(|| text(question.trim()))
}

/// Lower-case, single spaces, no thousands separators, no closing
/// punctuation, no polite prefix.
fn clean(question: &str) -> String {
    let mut s = String::new();
    let chars: Vec<char> = question.trim().chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let thousands = *c == ','
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())
            && chars.get(i + 4).is_none_or(|d| !d.is_ascii_digit());
        if !thousands {
            s.extend(c.to_lowercase());
        }
    }
    let s = s.trim_end_matches(['?', '.', '!', ' ', '؟']).to_string();
    let mut s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    for prefix in [
        "please ",
        "can you tell me ",
        "tell me ",
        "could you tell me ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
        }
    }
    s
}

fn num(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Exact integers; otherwise at most `dp` decimals, trailing zeros cut.
fn show(v: f64, dp: usize) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.dp$}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// --- word problems --------------------------------------------------------

static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+(?:\.\d+)?").expect("static regex"));

/// A question with at least two numbers that asks for a quantity: worth
/// letting the model write the arithmetic (see `ChatHandle::word_problem`).
pub fn looks_like_word_problem(question: &str) -> bool {
    let q = clean(question);
    if NUMBER.find_iter(&q).count() < 2 {
        return false;
    }
    const ASKS: &[&str] = &[
        "how much",
        "how many",
        "how long",
        "how far",
        "how fast",
        "what is the total",
        "what's the total",
        "total",
        "left",
        "remain",
        "average",
        "speed",
        "cost",
        "pay",
        "price",
        "each",
        "per",
        "change",
        "altogether",
        "in all",
        "need",
    ];
    ASKS.iter().any(|a| q.contains(a))
}

/// Every number in a model-written `expr` comes from `question` (or is its
/// percent as a decimal, or a small everyday constant): the model may set
/// the problem up, never invent its figures.
pub fn uses_question_numbers(expr: &str, question: &str) -> bool {
    let q = clean(question);
    let given: Vec<f64> = NUMBER
        .find_iter(&q)
        .filter_map(|m| num(m.as_str()))
        .collect();
    // A constant counts only when the question's words supply it: models
    // otherwise slip in a "÷ 3" from nowhere (85 × 38 ÷ 3 for a weekly wage).
    const WORDS: &[(f64, &[&str])] = &[
        (2.0, &["half", "halves", "twice", "double", "two", "pair"]),
        (3.0, &["third", "thirds", "three", "triple"]),
        (4.0, &["quarter", "quarters", "four"]),
        (5.0, &["fifth", "five"]),
        (7.0, &["week", "weeks", "weekly", "seven"]),
        (10.0, &["tenth", "ten"]),
        (
            12.0,
            &["dozen", "month", "months", "monthly", "year", "twelve"],
        ),
        (24.0, &["day", "days", "daily", "hour", "hours"]),
        (52.0, &["week", "weeks", "year"]),
        (60.0, &["hour", "hours", "minute", "minutes"]),
        (100.0, &["%", "percent", "per cent", "cm", "metre", "meter"]),
        (
            1000.0,
            &[
                "km", "kg", "gram", "grams", "litre", "liter", "ml", "thousand",
            ],
        ),
        (365.0, &["year", "years", "yearly", "annual"]),
    ];
    let words: Vec<&str> = q
        .split(|c: char| !c.is_alphanumeric() && c != '%')
        .collect();
    let has = |w: &str| {
        if w == "%" {
            q.contains('%')
        } else {
            words.contains(&w)
        }
    };
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let mut any = false;
    for m in NUMBER.find_iter(expr) {
        any = true;
        let Some(v) = num(m.as_str()) else {
            return false;
        };
        let ok = given.iter().any(|g| {
            close(*g, v)
                || close(g / 100.0, v)
                || close(1.0 - g / 100.0, v)
                || close(1.0 + g / 100.0, v)
        }) || close(v, 1.0)
            || WORDS
                .iter()
                .any(|(c, ws)| close(*c, v) && ws.iter().any(|w| has(w)));
        if !ok {
            return false;
        }
    }
    any
}

// --- percentages ------------------------------------------------------------

static PCT_OF: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?:what is |what's |calculate |find |work out )?{NUM} ?(?:%|percent|per cent) of {NUM}"
    ))
});
static PCT_CHANGE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(increase|raise|decrease|reduce|lower) {NUM} by {NUM} ?(?:%|percent|per cent)"
    ))
});
static PCT_WHAT: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"what (?:percent|percentage) (?:is|of) {NUM} (?:is |of |out of )?{NUM}"
    ))
});
static PCT_IS_WHAT: LazyLock<Regex> =
    LazyLock::new(|| re(&format!(r"{NUM} is what (?:percent|percentage) of {NUM}")));

fn percent(q: &str) -> Option<String> {
    if let Some(c) = PCT_OF.captures(q) {
        let (p, of) = (num(&c[1])?, num(&c[2])?);
        let v = p / 100.0 * of;
        return Some(format!(
            "{}% of {} = {}",
            show(p, 6),
            show(of, 6),
            show(v, 4)
        ));
    }
    if let Some(c) = PCT_CHANGE.captures(q) {
        let (base, p) = (num(&c[2])?, num(&c[3])?);
        let up = matches!(&c[1].to_lowercase()[..], "increase" | "raise");
        let v = if up {
            base * (1.0 + p / 100.0)
        } else {
            base * (1.0 - p / 100.0)
        };
        let verb = if up { "increased" } else { "decreased" };
        return Some(format!(
            "{} {verb} by {}% = {}",
            show(base, 6),
            show(p, 6),
            show(v, 4)
        ));
    }
    let (part, whole) = if let Some(c) = PCT_WHAT.captures(q) {
        (num(&c[1])?, num(&c[2])?)
    } else {
        let c = PCT_IS_WHAT.captures(q)?;
        (num(&c[1])?, num(&c[2])?)
    };
    if whole == 0.0 {
        return None;
    }
    Some(format!(
        "{} is {}% of {}",
        show(part, 6),
        show(part / whole * 100.0, 2),
        show(whole, 6)
    ))
}

// --- units ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Length,
    Mass,
    Volume,
    Temp,
    Speed,
}

/// (names, kind, factor to the kind's base unit, shown symbol). Temperature
/// is converted by formula; its factor is unused.
const UNITS: &[(&[&str], Kind, f64, &str)] = &[
    (
        &[
            "km",
            "kms",
            "kilometre",
            "kilometres",
            "kilometer",
            "kilometers",
        ],
        Kind::Length,
        1000.0,
        "km",
    ),
    (
        &["m", "metre", "metres", "meter", "meters"],
        Kind::Length,
        1.0,
        "m",
    ),
    (
        &[
            "cm",
            "centimetre",
            "centimetres",
            "centimeter",
            "centimeters",
        ],
        Kind::Length,
        0.01,
        "cm",
    ),
    (
        &[
            "mm",
            "millimetre",
            "millimetres",
            "millimeter",
            "millimeters",
        ],
        Kind::Length,
        0.001,
        "mm",
    ),
    (&["mi", "mile", "miles"], Kind::Length, 1609.344, "miles"),
    (&["ft", "foot", "feet"], Kind::Length, 0.3048, "ft"),
    (&["in", "inch", "inches"], Kind::Length, 0.0254, "in"),
    (&["yd", "yard", "yards"], Kind::Length, 0.9144, "yd"),
    (
        &["kg", "kgs", "kilo", "kilos", "kilogram", "kilograms"],
        Kind::Mass,
        1.0,
        "kg",
    ),
    (
        &["g", "gram", "grams", "gramme", "grammes"],
        Kind::Mass,
        0.001,
        "g",
    ),
    (
        &["lb", "lbs", "pound", "pounds"],
        Kind::Mass,
        0.453_592_37,
        "lb",
    ),
    (
        &["oz", "ounce", "ounces"],
        Kind::Mass,
        0.028_349_523_125,
        "oz",
    ),
    (
        &["l", "litre", "litres", "liter", "liters"],
        Kind::Volume,
        1.0,
        "L",
    ),
    (
        &[
            "ml",
            "millilitre",
            "millilitres",
            "milliliter",
            "milliliters",
        ],
        Kind::Volume,
        0.001,
        "mL",
    ),
    (
        &["gal", "gallon", "gallons"],
        Kind::Volume,
        3.785_411_784,
        "US gal",
    ),
    (
        &[
            "c",
            "°c",
            "celsius",
            "centigrade",
            "degrees c",
            "degrees celsius",
        ],
        Kind::Temp,
        0.0,
        "°C",
    ),
    (
        &["f", "°f", "fahrenheit", "degrees f", "degrees fahrenheit"],
        Kind::Temp,
        0.0,
        "°F",
    ),
    (&["k", "kelvin"], Kind::Temp, 0.0, "K"),
    (
        &[
            "km/h",
            "kph",
            "kmh",
            "kilometres per hour",
            "kilometers per hour",
        ],
        Kind::Speed,
        1.0 / 3.6,
        "km/h",
    ),
    (&["mph", "miles per hour"], Kind::Speed, 0.447_04, "mph"),
    (
        &["m/s", "metres per second", "meters per second"],
        Kind::Speed,
        1.0,
        "m/s",
    ),
];

fn unit(name: &str) -> Option<(Kind, f64, &'static str)> {
    let n = name.trim();
    UNITS
        .iter()
        .find(|(names, ..)| names.contains(&n))
        .map(|&(_, k, f, sym)| (k, f, sym))
}

static UNIT_TO: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?:convert |what is |what's |how much is |how many is )?{NUM} ?([a-z°/ ]+?) (?:to|in|into|in terms of) ([a-z°/ ]+)"
    ))
});
static HOW_MANY_IN: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"how many ([a-z°/ ]+?) (?:is|are|in|make|are in|are there in|is there in) {NUM} ?([a-z°/ ]+)"
    ))
});

fn units(q: &str) -> Option<String> {
    let (v, from, to) = if let Some(c) = UNIT_TO.captures(q) {
        (num(&c[1])?, c[2].to_string(), c[3].to_string())
    } else {
        let c = HOW_MANY_IN.captures(q)?;
        (num(&c[2])?, c[3].to_string(), c[1].to_string())
    };
    let (fk, ff, fs) = unit(&from)?;
    let (tk, tf, ts) = unit(&to)?;
    if fk != tk || fs == ts {
        return None;
    }
    let out = if fk == Kind::Temp {
        let c = match fs {
            "°C" => v,
            "°F" => (v - 32.0) * 5.0 / 9.0,
            _ => v - 273.15,
        };
        match ts {
            "°C" => c,
            "°F" => c * 9.0 / 5.0 + 32.0,
            _ => c + 273.15,
        }
    } else {
        v * ff / tf
    };
    let space = |s: &str| if s.starts_with('°') { "" } else { " " };
    Some(format!(
        "{}{}{fs} = {}{}{ts}",
        show(v, 6),
        space(fs),
        show(out, 2),
        space(ts)
    ))
}

// --- calendar ---------------------------------------------------------------

const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn month(s: &str) -> Option<u32> {
    let s = s.trim_end_matches('.');
    MONTHS
        .iter()
        .position(|m| {
            *m == s
                || (s.len() >= 3 && m.starts_with(s) && s != "mar" || s == "mar" && *m == "march")
        })
        .map(|i| i as u32 + 1)
}

static DAY_MONTH: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:the )?(\d{1,2})(?:st|nd|rd|th)?(?: of)? ([a-z]+\.?),?(?: (\d{4}))?"));
static MONTH_DAY: LazyLock<Regex> =
    LazyLock::new(|| re(r"([a-z]+\.?) (\d{1,2})(?:st|nd|rd|th)?,?(?: (\d{4}))?"));
static ISO: LazyLock<Regex> = LazyLock::new(|| re(r"(\d{4})-(\d{1,2})-(\d{1,2})"));

/// A date named in `s`; a date without a year is its next occurrence on or
/// after `today` when `upcoming`, else this year's.
fn parse_date(s: &str, today: NaiveDate, upcoming: bool) -> Option<NaiveDate> {
    let s = s.trim().trim_start_matches("the ");
    let pick = |m: u32, d: u32, y: Option<i32>| -> Option<NaiveDate> {
        match y {
            Some(y) => NaiveDate::from_ymd_opt(y, m, d),
            None => {
                let this = NaiveDate::from_ymd_opt(today.year(), m, d)?;
                if upcoming && this < today {
                    NaiveDate::from_ymd_opt(today.year() + 1, m, d)
                } else {
                    Some(this)
                }
            }
        }
    };
    match s {
        "today" | "now" => return Some(today),
        "tomorrow" => return Some(today + Duration::days(1)),
        "yesterday" => return Some(today - Duration::days(1)),
        "new year" | "new year's" | "new years" | "new year's day" | "new years day"
        | "the new year" => return NaiveDate::from_ymd_opt(today.year() + 1, 1, 1),
        "christmas" | "christmas day" | "xmas" => return pick(12, 25, None),
        "new year's eve" | "new years eve" => return pick(12, 31, None),
        _ => {}
    }
    if let Some(c) = ISO.captures(s) {
        return NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
    }
    if let Some(c) = DAY_MONTH.captures(s) {
        let m = month(&c[2])?;
        return pick(
            m,
            c[1].parse().ok()?,
            c.get(3).and_then(|y| y.as_str().parse().ok()),
        );
    }
    if let Some(c) = MONTH_DAY.captures(s) {
        let m = month(&c[1])?;
        return pick(
            m,
            c[2].parse().ok()?,
            c.get(3).and_then(|y| y.as_str().parse().ok()),
        );
    }
    None
}

fn long(d: NaiveDate) -> String {
    d.format("%A, %B %-d, %Y").to_string()
}

static UNTIL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:how many days (?:are there |are left |is it |left )?(?:until|till|til|to|before)|days (?:until|till|to|left until)|how long (?:is it )?(?:until|till|to)) (.+?)(?: from (?:today|now))?",
    )
});
static BETWEEN: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:how many days (?:are there )?(?:between|from)|days between|number of days between) (.+?) (?:and|to|until) (.+)",
    )
});
static WEEKDAY: LazyLock<Regex> = LazyLock::new(|| {
    re(r"what day(?: of the week)? (?:is|was|will be|falls on|does) (.+?)(?: (?:be|fall on|fall))?")
});
static OFFSET: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:what(?: is|'s)? (?:the )?date (?:is |will it be |will be |was it |was )?|what date (?:is it |will it be )?)?(?:in )?(\d+) (day|days|week|weeks)(?: (from (?:today|now)|after today|later|ago|before today|from now))?",
    )
});
static TODAY: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:what is|what's|whats) (?:today's date|the date today|the date|today|the day today)|what day is (?:it )?today|what is the date|today's date",
    )
});

fn dates(q: &str, today: NaiveDate) -> Option<String> {
    if TODAY.is_match(q) {
        return Some(format!("Today is {}.", long(today)));
    }
    if let Some(c) = BETWEEN.captures(q) {
        let a = parse_date(&c[1], today, false)?;
        let b = parse_date(&c[2], today, false)?;
        let n = (b - a).num_days().abs();
        return Some(format!(
            "{n} days between {} and {}.",
            long(a.min(b)),
            long(a.max(b))
        ));
    }
    if let Some(c) = UNTIL.captures(q) {
        let d = parse_date(&c[1], today, true)?;
        let n = (d - today).num_days();
        if n < 0 {
            return None;
        }
        return Some(format!(
            "{n} days until {} (today is {}).",
            long(d),
            long(today)
        ));
    }
    if let Some(c) = WEEKDAY.captures(q) {
        let d = parse_date(&c[1], today, false)?;
        return Some(format!(
            "{} is a {}.",
            d.format("%B %-d, %Y"),
            d.format("%A")
        ));
    }
    if let Some(c) = OFFSET.captures(q) {
        // Only with an explicit direction, or "in N days" phrasing.
        let n: i64 = c[1].parse().ok()?;
        let unit = if c[2].starts_with("week") { 7 } else { 1 };
        let dir = c.get(3).map(|m| m.as_str()).unwrap_or("");
        if dir.is_empty() && !q.contains(" in ") && !q.starts_with("in ") {
            return None;
        }
        let back = dir == "ago" || dir == "before today";
        let delta = Duration::days(n * unit);
        let d = if back { today - delta } else { today + delta };
        let what = if back { "before" } else { "after" };
        return Some(format!(
            "{} — {n} {} {what} today ({}).",
            long(d),
            &c[2],
            long(today)
        ));
    }
    None
}

// --- text -------------------------------------------------------------------

static LETTERS_IN: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"how many (?:letters|characters) (?:are |is )?(?:there )?in (?:the word |the name |the string )?["'“]?([\p{L}\-']+)["'”]?"#,
    )
});
static LETTER_COUNT: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?:how many times does (?:the letter )?["']?(\p{L})["']? (?:appear|occur|show up) in|how many ["']?(\p{L})["']?(?:'s|s)? (?:are )?(?:there )?in) (?:the word )?["'“]?([\p{L}\-']+)["'”]?"#,
    )
});
static WORDS_IN: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:how many words (?:are )?(?:there )?in|count the words in) (?:this|the following|the) (?:sentence|text|phrase|line)\s*:\s*(.+)",
    )
});
static SORT: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:sort|order|arrange) (?:these |the following |the )?numbers(?: from)? (smallest to largest|lowest to highest|ascending|in ascending order|largest to smallest|highest to lowest|descending|in descending order)?\s*:?\s*([-\d.,\s]+)",
    )
});
static REVERSE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?:spell (?:the word )?["']?([\p{L}]+)["']? backwards|reverse (?:the word )?["']?([\p{L}]+)["']?)"#,
    )
});

fn text(q: &str) -> Option<String> {
    let lower = q.to_lowercase();
    let lower = lower.trim_end_matches(['?', '.', '!', ' ']);
    if let Some(c) = LETTER_COUNT.captures(lower) {
        let letter = c.get(1).or(c.get(2))?.as_str().chars().next()?;
        let word = &c[3];
        let n = word
            .chars()
            .filter(|ch| ch.to_lowercase().eq(letter.to_lowercase()))
            .count();
        let positions: Vec<String> = word
            .chars()
            .enumerate()
            .filter(|(_, ch)| ch.to_lowercase().eq(letter.to_lowercase()))
            .map(|(i, _)| (i + 1).to_string())
            .collect();
        let at = if positions.is_empty() {
            String::new()
        } else {
            format!(" (positions {})", positions.join(", "))
        };
        return Some(format!(
            "The letter \"{letter}\" appears {n} time{} in \"{word}\"{at}.",
            if n == 1 { "" } else { "s" }
        ));
    }
    if let Some(c) = LETTERS_IN.captures(lower) {
        let word = &c[1];
        let n = word.chars().filter(|ch| ch.is_alphabetic()).count();
        return Some(format!("\"{word}\" has {n} letters."));
    }
    if let Some(c) = WORDS_IN.captures(q.trim_end_matches(['?', ' '])) {
        let body = c[1].trim();
        let n = body.split_whitespace().count();
        return Some(format!("That text has {n} words."));
    }
    if let Some(c) = SORT.captures(lower) {
        let mut nums: Vec<f64> = c[2]
            .split([',', ' '])
            .filter(|t| !t.is_empty())
            .map(|t| t.parse::<f64>())
            .collect::<Result<_, _>>()
            .ok()?;
        if nums.len() < 2 {
            return None;
        }
        nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let desc = c.get(1).is_some_and(|m| {
            m.as_str().contains("largest to")
                || m.as_str().contains("highest to")
                || m.as_str().contains("desc")
        });
        if desc {
            nums.reverse();
        }
        let shown: Vec<String> = nums.iter().map(|v| show(*v, 10)).collect();
        return Some(shown.join(", "));
    }
    if let Some(c) = REVERSE.captures(lower) {
        let word = c.get(1).or(c.get(2))?.as_str();
        // Keep the user's own casing.
        let original = q
            .split(|ch: char| !ch.is_alphabetic())
            .find(|w| w.eq_ignore_ascii_case(word))
            .unwrap_or(word);
        let rev: String = original.chars().rev().collect();
        return Some(format!("\"{original}\" backwards is \"{rev}\"."));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 8).unwrap()
    }

    fn a(q: &str) -> String {
        answer(q, day()).unwrap_or_else(|| panic!("no exact answer for {q:?}"))
    }

    #[test]
    fn percentages() {
        assert_eq!(a("what is 15% of 2400?"), "15% of 2400 = 360");
        assert_eq!(a("15 percent of 2,400"), "15% of 2400 = 360");
        assert_eq!(a("increase 80 by 25%"), "80 increased by 25% = 100");
        assert_eq!(a("reduce 250 by 30%"), "250 decreased by 30% = 175");
        assert_eq!(a("what percent is 45 of 180"), "45 is 25% of 180");
        assert_eq!(a("45 is what percent of 180?"), "45 is 25% of 180");
    }

    #[test]
    fn units() {
        assert_eq!(a("convert 5 miles to km"), "5 miles = 8.05 km");
        assert_eq!(a("how many pounds is 70 kg"), "70 kg = 154.32 lb");
        assert_eq!(a("what is 100 F in celsius"), "100°F = 37.78°C");
        assert_eq!(a("6 feet in meters"), "6 ft = 1.83 m");
        assert_eq!(a("how many millilitres in 2.5 litres"), "2.5 L = 2500 mL");
        assert_eq!(a("how many grams are in 3 pounds"), "3 lb = 1360.78 g");
        assert_eq!(a("60 mph to km/h"), "60 mph = 96.56 km/h");
        // Different kinds do not convert.
        assert_eq!(answer("convert 5 kg to km", day()), None);
    }

    #[test]
    fn calendar() {
        assert_eq!(
            a("how many days until new year"),
            "54 days until Friday, January 1, 2027 (today is Sunday, November 8, 2026)."
        );
        assert_eq!(
            a("what day of the week is 25 December 2026"),
            "December 25, 2026 is a Friday."
        );
        assert_eq!(
            a("how many days between 1 March 2026 and 15 August 2026"),
            "167 days between Sunday, March 1, 2026 and Saturday, August 15, 2026."
        );
        assert!(a("what date is 100 days from today").starts_with("Tuesday, February 16, 2027"));
        assert!(a("what's the date in 2 weeks").starts_with("Sunday, November 22, 2026"));
        assert_eq!(
            a("what is today's date"),
            "Today is Sunday, November 8, 2026."
        );
        assert!(a("how many days until christmas").starts_with("47 days"));
        assert!(a("days until march 3").starts_with("115 days until Wednesday, March 3, 2027"));
    }

    #[test]
    fn text_tools() {
        assert_eq!(
            a("how many letters are in the word strawberry"),
            "\"strawberry\" has 10 letters."
        );
        assert_eq!(
            a("how many times does the letter r appear in strawberry"),
            "The letter \"r\" appears 3 times in \"strawberry\" (positions 3, 8, 9)."
        );
        assert!(a("how many r's in strawberry?").contains("3 times"));
        assert_eq!(
            a("how many words are in this sentence: the quick brown fox jumps over the lazy dog"),
            "That text has 9 words."
        );
        assert_eq!(
            a("sort these numbers from smallest to largest: 42, 7, 19, 3, 88"),
            "3, 7, 19, 42, 88"
        );
        assert_eq!(a("sort these numbers descending: 1, 3, 2"), "3, 2, 1");
        assert_eq!(
            a("spell the word harbor backwards"),
            "\"harbor\" backwards is \"robrah\"."
        );
    }

    #[test]
    fn word_problems_are_spotted_and_their_figures_checked() {
        let q = "I drive 420 km and my car uses 7 litres per 100 km. How many litres do I need?";
        assert!(looks_like_word_problem(q));
        assert!(uses_question_numbers("420 / 100 * 7", q));
        // An invented figure is refused.
        assert!(!uses_question_numbers("420 * 0.085", q));
        assert!(uses_question_numbers(
            "3 * 45 * 0.9",
            "3 shirts at 45 each with 10% off"
        ));
        // A divisor the question does not supply is refused.
        assert!(!uses_question_numbers(
            "85 * 38 / 3",
            "I earn 85 per hour and work 38 hours a week"
        ));
        assert!(uses_question_numbers(
            "48 - 48 / 3 - 5",
            "Sara has 48 apples and gives a third away, then eats 5"
        ));
        assert!(!looks_like_word_problem("how many continents are there"));
        assert!(!looks_like_word_problem("what is the capital of France"));
    }

    #[test]
    fn everything_else_goes_to_the_model() {
        for q in [
            "what is the capital of France",
            "who wrote Pride and Prejudice",
            "how many continents are there",
            "what day is the meeting",
            "how many days does it take to learn French",
            "convert this document to pdf",
            "how many people live in Riyadh",
            "what percent of people are left-handed",
            "sort these files by date",
            "in 3 words describe the sea",
        ] {
            assert_eq!(answer(q, day()), None, "{q}");
        }
    }
}
