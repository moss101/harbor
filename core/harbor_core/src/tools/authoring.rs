//! Verifiers for authored content (decision 0008). Each takes what a model
//! drafted and the material it was drafted from, and returns
//! `{ok, problems, …}` where every problem is a sentence the model can act
//! on: what is wrong, where, and what to do instead. A graph routes a
//! failed check back to the drafting node once (a bounded back-edge) with
//! those problems in its context, and stops after that — the executor's
//! schema retry fixes shape; these fix substance.
//!
//! - `workbook.verify_spec` — a table spec is well formed and every number
//!   in it occurs in the user's description;
//! - `deck.verify_outline` — slide count and structure, no template text,
//!   grounded numbers, and (for a summarised document) a verbatim quote
//!   on the page or paragraph each bullet cites;
//! - `document.verify_draft` — the template's sections, in order, no
//!   template text, grounded numbers;
//! - `email.verify_draft` / `email.render` — grounded names and figures, no
//!   attachment or "sent" claims, and the final text laid out by code;
//! - `text.verify_items` — named fields occur verbatim in the source and,
//!   optionally, next to the thing they are attributed to;
//! - `text.verify_citations` / `document.units` — the page/paragraph model
//!   a citation refers to, and the check that the quote is there.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::create::{
    normalize_outline, normalize_table_spec, parse_number_text, sentence_table,
    strip_letter_furniture, DeckOutline, DocumentSpec, WorkbookSpec,
};
use super::review::{number_key, numbers_in};
use super::{RiskClass, Tool, ToolContext, ToolError, ToolSpec};

const MB: usize = 1024 * 1024;

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// Shared checks

/// The numbers a source mentions, matched two ways so a faithful restating
/// is never flagged: by digits (`$540k` ~ `540`) and by value (`1,250.00` ~
/// `1250`, `12%` ~ `0.12`).
pub struct Grounding {
    keys: BTreeSet<String>,
    values: Vec<f64>,
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

impl Grounding {
    pub fn of(source: &str) -> Grounding {
        let mut keys = BTreeSet::new();
        let mut values = Vec::new();
        for span in numbers_in(source) {
            let k = number_key(&span);
            if !k.is_empty() {
                keys.insert(k);
            }
            if let Some((v, pct)) = parse_number_text(span.trim()) {
                values.push(v);
                if pct {
                    values.push(v / 100.0);
                }
            }
        }
        Grounding { keys, values }
    }

    pub fn has_value(&self, x: f64) -> bool {
        self.values
            .iter()
            .any(|v| close(*v, x) || close(*v, x * 100.0))
    }

    pub fn has_span(&self, span: &str) -> bool {
        let k = number_key(span);
        if k.is_empty() || self.keys.contains(&k) {
            return true;
        }
        match parse_number_text(span.trim()) {
            Some((v, pct)) => self.has_value(v) || (pct && self.has_value(v / 100.0)),
            None => false,
        }
    }

    /// Number spans in `text` the source never mentions (deduplicated).
    pub fn ungrounded(&self, text: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for n in numbers_in(text) {
            let n = n.trim().to_string();
            if !self.has_span(&n) && !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }
}

fn placeholder_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\b(?:lorem|ipsum|tbd|tbc|xxx+|placeholder|click to (?:add|edit)|insert (?:name|date|text|here))\b|\[(?:insert|your |your\]|name\]|date\]|company|todo|recipient|sender|client)[^\]]*\]|\{\{[^}]*\}\}|<<[^>]*>>",
        )
        .expect("placeholder regex")
    })
}

/// Template text left in a draft. Harbor's own honest gap markers
/// (`[needs input]`, `[needs figure]`) are not template text.
pub fn placeholder_in(text: &str) -> Option<String> {
    placeholder_re().find(text).map(|m| m.as_str().to_string())
}

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

const STOPWORDS: &[&str] = &[
    "with", "from", "that", "this", "will", "should", "would", "have", "about", "into", "also",
    "next", "their", "there", "they", "what", "when", "where", "which", "while", "your", "owner",
    "someone", "somebody", "could", "must", "need", "needs", "make", "sure", "take", "check",
];

/// Words of `anchor` specific enough to locate it in a source: four or
/// more letters in Latin script, three or more in others (Arabic stems are
/// short), stopwords excluded.
fn significant_words(anchor: &str) -> Vec<String> {
    anchor
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| {
            let n = w.chars().count();
            let latin = w.is_ascii();
            (latin && n >= 4) || (!latin && n >= 3)
        })
        .map(|w| w.to_lowercase())
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .collect()
}

/// Whether `value` is stated near `anchor` in `source`: on a line that
/// mentions the value, or on the line just before it (a reply — "Can
/// someone update the runbook?" / "Omar: I will."), at least one
/// significant word of the anchor occurs. Not the line after: in a
/// transcript the next line is usually someone else's turn ("Also, someone
/// should update the runbook — no owner yet."), and crediting the previous
/// speaker with it is exactly the misattribution this check exists for.
/// Conservative by design — a value it cannot attribute is nulled, never
/// guessed.
pub fn attributed(source: &str, value: &str, anchor: &str) -> bool {
    let words = significant_words(anchor);
    if words.is_empty() {
        return true;
    }
    let lines: Vec<String> = source.lines().map(|l| l.to_lowercase()).collect();
    let needle = value.to_lowercase();
    for (i, line) in lines.iter().enumerate() {
        if !line.contains(&needle) {
            continue;
        }
        let from = i.saturating_sub(1);
        let window = lines[from..=i].join("\n");
        if words.iter().any(|w| window.contains(w.as_str())) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Units and citations

/// A document as numbered units — pages of a PDF, paragraphs of a DOCX,
/// slides of a deck, lines of text — the frame a citation's `ref` names.
pub struct Units {
    pub label: &'static str,
    pub units: BTreeMap<i64, String>,
}

pub fn units_of(document: &Value) -> Units {
    let kind = document
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("text");
    let mut units = BTreeMap::new();
    let collect =
        |arr: Option<&Vec<Value>>, f: &dyn Fn(&Value) -> String| -> BTreeMap<i64, String> {
            arr.map(|a| {
                a.iter()
                    .filter_map(|u| Some((u.get("index")?.as_i64()?, f(u))))
                    .collect()
            })
            .unwrap_or_default()
        };
    let label = match kind {
        "pdf" => {
            units = collect(
                document.get("pages").and_then(Value::as_array),
                &|u: &Value| s(u, "text"),
            );
            "page"
        }
        "docx" => {
            units = collect(
                document.get("paragraphs").and_then(Value::as_array),
                &|u: &Value| s(u, "text"),
            );
            "paragraph"
        }
        "pptx" => {
            units = collect(
                document.get("slides").and_then(Value::as_array),
                &|u: &Value| {
                    let mut parts = vec![s(u, "title")];
                    if let Some(b) = u.get("bullets").and_then(Value::as_array) {
                        parts.extend(b.iter().filter_map(Value::as_str).map(str::to_string));
                    }
                    parts.push(s(u, "notes"));
                    parts.join("\n")
                },
            );
            "slide"
        }
        _ => {
            let text = document
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            for (i, line) in text.lines().enumerate() {
                units.insert(i as i64 + 1, line.to_string());
            }
            "line"
        }
    };
    Units { label, units }
}

/// Whitespace, case and typographic quotes/dashes normalised: PDFs carry
/// curly quotes and hyphenation the model cannot reproduce exactly, and a
/// sentence-initial capital is not a paraphrase.
fn normalise(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201b}' | '`' => '\'',
            '\u{201c}' | '\u{201d}' | '\u{201f}' => '"',
            '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            '\u{00a0}' => ' ',
            c => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub const MIN_QUOTE_CHARS: usize = 12;

/// `Ok(())` when `quote` occurs in unit `unit_ref`, otherwise the reason
/// in words a model can act on.
pub fn check_citation(units: &Units, unit_ref: i64, quote: &str) -> Result<(), String> {
    let label = units.label;
    let q = normalise(quote.trim().trim_matches('"').trim_matches('\u{201c}'));
    if q.chars().count() < MIN_QUOTE_CHARS {
        return Err(format!(
            "the quote is too short to check; copy at least a few words exactly as they appear on {label} {unit_ref}"
        ));
    }
    let Some(text) = units.units.get(&unit_ref) else {
        return Err(format!(
            "{label} {unit_ref} does not exist (the document has {} {label}s numbered {}–{})",
            units.units.len(),
            units.units.keys().next().copied().unwrap_or(0),
            units.units.keys().last().copied().unwrap_or(0)
        ));
    };
    if normalise(text).contains(&q) {
        return Ok(());
    }
    // Say where the quote actually is, when it is somewhere else.
    if let Some((other, _)) = units.units.iter().find(|(_, t)| normalise(t).contains(&q)) {
        return Err(format!(
            "the quote is on {label} {other}, not {label} {unit_ref}; cite {label} {other}"
        ));
    }
    Err(format!(
        "the quote does not appear on {label} {unit_ref}; copy a passage exactly as it is written there"
    ))
}

/// One citeable sentence of a document.
pub struct Sentence {
    pub n: i64,
    pub unit: i64,
    pub text: String,
}

/// Split each unit into sentences and number the citeable ones across the
/// whole document. A sentence ends at `.`, `!` or `?` followed by
/// whitespace and a capital, digit, quote or bracket; a line break also
/// ends one. Short lines without end punctuation (headings, a report
/// title) are kept as context, unnumbered. Returns the sentences and the
/// compact text a model reads (`Page 2` headers, `[n] sentence` lines).
pub fn sentences_of(units: &Units) -> (Vec<Sentence>, String) {
    let label = {
        let mut c = units.label.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect::<String>())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    let mut text = String::new();
    let mut n = 0i64;
    for (unit, body) in &units.units {
        let mut header_written = false;
        for line in joined_lines(body) {
            if line.is_empty() {
                continue;
            }
            if !header_written {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&format!("{label} {unit}\n"));
                header_written = true;
            }
            for sentence in split_sentences(&line) {
                let words = sentence
                    .split_whitespace()
                    .filter(|w| w.chars().any(char::is_alphanumeric))
                    .count();
                let ends = sentence.ends_with(['.', '!', '?']);
                if (ends && words >= 3) || words >= 10 {
                    n += 1;
                    text.push_str(&format!("[{n}] {sentence}\n"));
                    out.push(Sentence {
                        n,
                        unit: *unit,
                        text: sentence,
                    });
                } else {
                    text.push_str(&format!("{sentence}\n"));
                }
            }
        }
    }
    (out, text.trim_end().to_string())
}

/// Lines of a unit with wrapped sentences re-joined: a line that does not
/// end a sentence continues on the next when that one starts in lower
/// case (PDF text breaks lines wherever the page did). Headings, followed
/// by a capitalised line, stay separate.
fn joined_lines(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in body.lines() {
        let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            out.push(String::new());
            continue;
        }
        let continues = out
            .last()
            .map(|prev| {
                !prev.is_empty()
                    && !prev.ends_with(['.', '!', '?', ':', ';'])
                    && line.chars().next().map(char::is_lowercase).unwrap_or(false)
            })
            .unwrap_or(false);
        if continues {
            let prev = out.last_mut().expect("checked");
            prev.push(' ');
            prev.push_str(&line);
        } else {
            out.push(line);
        }
    }
    out
}

fn split_sentences(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, '.' | '!' | '?')
            && i + 2 < chars.len()
            && chars[i + 1].is_whitespace()
            && (chars[i + 2].is_uppercase()
                || chars[i + 2].is_ascii_digit()
                || matches!(chars[i + 2], '"' | '\u{201c}' | '(' | '['))
        {
            let s: String = chars[start..=i].iter().collect();
            out.push(s.trim().to_string());
            start = i + 1;
        }
        i += 1;
    }
    let rest: String = chars[start..].iter().collect();
    if !rest.trim().is_empty() {
        out.push(rest.trim().to_string());
    }
    out
}

pub struct DocumentSentences {
    spec: ToolSpec,
}

impl DocumentSentences {
    pub fn new() -> Self {
        DocumentSentences {
            spec: ToolSpec {
                id: "document.sentences".into(),
                description: "Number a document's citeable sentences, page by page (or paragraph, slide, line), as compact `[n] sentence` lines under unit headers. A model cites a sentence by its number; deck.verify_outline and deck.build look the sentence up, so the model never has to copy a quote.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "document": {"type": "object"},
                        "max_sentences": {"type": "integer", "minimum": 1, "maximum": 5000}
                    },
                    "required": ["document"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for DocumentSentences {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DocumentSentences {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let units = units_of(&args["document"]);
        let (sentences, text) = sentences_of(&units);
        let max = args
            .get("max_sentences")
            .and_then(Value::as_u64)
            .unwrap_or(1000) as usize;
        Ok(json!({
            "unit": units.label,
            "count": sentences.len(),
            "truncated": sentences.len() > max,
            "sentences": sentences.iter().take(max).map(|x| json!({"n": x.n, "unit": x.unit, "text": x.text})).collect::<Vec<_>>(),
            "numbered_text": text,
        }))
    }
}

// ---------------------------------------------------------------------------
// workbook.verify_spec

pub struct WorkbookVerifySpec {
    spec: ToolSpec,
}

impl WorkbookVerifySpec {
    pub fn new() -> Self {
        WorkbookVerifySpec {
            spec: ToolSpec {
                id: "workbook.verify_spec".into(),
                description: "Check a drafted table spec before anything is built: shape (typed columns, one cell per column, numeric cells that are numbers, totals and computed columns over numeric columns, a valid sheet name) and grounding (every number occurs in the user's description). Row labels the description does not mention are reported as suggestions, not errors.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "spec": {"type": "object"},
                        "source": {"type": "string", "maxLength": 2000000}
                    },
                    "required": ["spec", "source"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for WorkbookVerifySpec {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for WorkbookVerifySpec {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        // Check the table that would be built: structural slips are
        // repaired first (and reported), so the problems a model hears
        // are about substance it can fix.
        let (normalized, notes) = normalize_table_spec(&args["spec"]);
        let raw = &normalized;
        let source = s(args, "source");
        let mut problems: Vec<String> = Vec::new();
        let mut suggested = Vec::new();
        let mut checked = 0usize;
        match WorkbookSpec::from_value(raw) {
            Err(p) => problems.extend(p),
            Ok(spec) => {
                let g = Grounding::of(&source);
                let lower = source.to_lowercase();
                let label_col = spec
                    .columns
                    .iter()
                    .position(|(_, k)| *k == super::create::ColumnType::Text);
                let raw_rows = raw["rows"].as_array().cloned().unwrap_or_default();
                for (ri, row) in raw_rows.iter().enumerate() {
                    let cells = row.as_array().cloned().unwrap_or_default();
                    let label = label_col
                        .and_then(|c| cells.get(c))
                        .and_then(Value::as_str)
                        .map(|l| format!(" ({l})"))
                        .unwrap_or_default();
                    for (ci, cell) in cells.iter().enumerate() {
                        let Some((header, kind)) = spec.columns.get(ci) else {
                            continue;
                        };
                        if kind.is_numeric() {
                            let (grounded, shown) = match cell {
                                Value::Number(n) => (
                                    n.as_f64().map(|x| g.has_value(x)).unwrap_or(false),
                                    n.to_string(),
                                ),
                                Value::String(t) if !t.trim().is_empty() => {
                                    (g.has_span(t.trim()), t.trim().to_string())
                                }
                                _ => continue,
                            };
                            checked += 1;
                            if !grounded {
                                problems.push(format!(
                                    "Row {}{label}: {shown} under {header:?} does not appear in the description. Use only figures from the description, or leave the cell empty (null).",
                                    ri + 1
                                ));
                            }
                        } else if let Some(t) = cell.as_str() {
                            if let Some(m) = placeholder_in(t) {
                                problems.push(format!(
                                    "Row {}{label}: {m:?} is template text; use a value from the description or leave the cell empty.",
                                    ri + 1
                                ));
                            }
                            for n in g.ungrounded(t) {
                                problems.push(format!(
                                    "Row {}{label}: {n} under {header:?} does not appear in the description.",
                                    ri + 1
                                ));
                            }
                            if Some(ci) == label_col
                                && !t.trim().is_empty()
                                && !lower.contains(&t.trim().to_lowercase())
                            {
                                suggested.push(t.trim().to_string());
                            }
                        }
                    }
                }
                for text in std::iter::once(spec.title.as_str())
                    .chain(spec.columns.iter().map(|(h, _)| h.as_str()))
                {
                    if let Some(m) = placeholder_in(text) {
                        problems.push(format!("{text:?} contains template text {m:?}."));
                    }
                }
            }
        }
        Ok(json!({
            "ok": problems.is_empty(),
            "problems": problems,
            "suggested_labels": suggested,
            "numbers_checked": checked,
            "normalized": notes,
        }))
    }
}

// ---------------------------------------------------------------------------
// deck.verify_outline

pub struct DeckVerifyOutline {
    spec: ToolSpec,
}

impl DeckVerifyOutline {
    pub fn new() -> Self {
        DeckVerifyOutline {
            spec: ToolSpec {
                id: "deck.verify_outline".into(),
                description: "Check a drafted deck outline before it is built: slide count within bounds, one distinct title per slide, bullet counts and lengths, no template text, every number grounded in the source, and — when a document is supplied — every bullet citing a page/paragraph whose text contains its quote verbatim.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "outline": {"type": "object"},
                        "source": {"type": "string", "maxLength": 2000000},
                        "slides": {"type": ["integer", "null"], "minimum": 1, "maximum": 30},
                        "min_slides": {"type": "integer", "minimum": 1, "maximum": 30},
                        "max_slides": {"type": "integer", "minimum": 1, "maximum": 30},
                        "max_bullets": {"type": "integer", "minimum": 1, "maximum": 10},
                        "max_bullet_words": {"type": "integer", "minimum": 3, "maximum": 60},
                        "document": {"type": "object"},
                        "sentences": {"type": "object"}
                    },
                    "required": ["outline", "source"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for DeckVerifyOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DeckVerifyOutline {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let source = s(args, "source");
        let int = |k: &str, d: i64| args.get(k).and_then(Value::as_i64).unwrap_or(d);
        // An exact count (the user asked for five slides) wins over the
        // graph's default range.
        let exact = args.get("slides").and_then(Value::as_i64);
        let min = exact.unwrap_or_else(|| int("min_slides", 1)).max(1) as usize;
        let max = exact
            .unwrap_or_else(|| int("max_slides", 12))
            .max(min as i64) as usize;
        let max_bullets = int("max_bullets", 5) as usize;
        let max_words = int("max_bullet_words", 25) as usize;
        // With numbered sentences, a bullet cites a sentence number and its
        // figures must be in that sentence; with only a document, a bullet
        // carries a page and a quote that must be on that page.
        let sentences = args.get("sentences").map(sentence_table);
        let units = if sentences.is_some() {
            None
        } else {
            args.get("document").map(units_of)
        };
        let mut problems: Vec<String> = Vec::new();
        // Advisory: worth one repair round, never a reason to withhold the
        // deck (repetition, length).
        let mut warnings: Vec<String> = Vec::new();
        let mut cited_by: BTreeMap<i64, String> = BTreeMap::new();
        let mut seen_bullets: BTreeMap<String, String> = BTreeMap::new();
        let (normalized, notes) = normalize_outline(&args["outline"]);
        let mut bullets_seen = 0usize;
        let mut cites_checked = 0usize;
        let mut cites_ok = 0usize;
        match DeckOutline::from_value(&normalized) {
            Err(p) => problems.extend(p),
            Ok(o) => {
                let g = Grounding::of(&source);
                let n = o.slides.len();
                if n < min || n > max {
                    problems.push(if min == max {
                        format!("The outline has {n} slides; it must have exactly {min}.")
                    } else {
                        format!("The outline has {n} slides; it must have between {min} and {max}.")
                    });
                }
                for text in std::iter::once(o.title.as_str()).chain(o.subtitle.as_deref()) {
                    for num in g.ungrounded(text) {
                        problems.push(format!(
                            "The deck title or subtitle uses {num}, which does not appear in the source; remove it."
                        ));
                    }
                    if let Some(m) = placeholder_in(text) {
                        problems.push(format!(
                            "The deck title still contains template text {m:?}."
                        ));
                    }
                }
                let mut titles: BTreeMap<String, usize> = BTreeMap::new();
                for (i, slide) in o.slides.iter().enumerate() {
                    let at = i + 1;
                    if let Some(prev) = titles.insert(slide.title.to_lowercase(), at) {
                        warnings.push(format!(
                            "Slides {prev} and {at} have the same title {:?}; give each slide its own message.",
                            slide.title
                        ));
                    }
                    if words(&slide.title) > 12 {
                        warnings.push(format!(
                            "Slide {at} title is {} words; keep titles under 12 words.",
                            words(&slide.title)
                        ));
                    }
                    if slide.bullets.is_empty() {
                        problems.push(format!("Slide {at} has no bullets."));
                    }
                    if slide.bullets.len() > max_bullets {
                        warnings.push(format!(
                            "Slide {at} has {} bullets; keep it to {max_bullets}.",
                            slide.bullets.len()
                        ));
                    }
                    let mut texts: Vec<&str> = vec![slide.title.as_str()];
                    texts.extend(slide.notes.as_deref());
                    for (j, b) in slide.bullets.iter().enumerate() {
                        bullets_seen += 1;
                        texts.push(b.text.as_str());
                        let here = format!("slide {at}, bullet {}", j + 1);
                        if let Some(prev) = seen_bullets.insert(b.text.to_lowercase(), here.clone())
                        {
                            warnings.push(format!(
                                "{} repeats {prev}; each point should appear once.",
                                capitalise(&here)
                            ));
                        }
                        if let Some(table) = &sentences {
                            cites_checked += 1;
                            match b.source {
                                None => problems.push(format!(
                                    "{} has no source; set source to the number in brackets of the sentence it comes from.",
                                    capitalise(&here)
                                )),
                                Some(n) => match table.get(&n) {
                                    None => problems.push(format!(
                                        "{}: sentence [{n}] does not exist; cite a numbered sentence from the document.",
                                        capitalise(&here)
                                    )),
                                    Some((_, sentence)) => {
                                        let local = Grounding::of(sentence);
                                        let stray = local.ungrounded(&b.text);
                                        if stray.is_empty() {
                                            cites_ok += 1;
                                        } else {
                                            problems.push(format!(
                                                "{}: {} is not in sentence [{n}]; cite the sentence that states it.",
                                                capitalise(&here),
                                                stray.join(", ")
                                            ));
                                        }
                                        if let Some(prev) = cited_by.insert(n, here.clone()) {
                                            warnings.push(format!(
                                                "Sentence [{n}] is cited by {prev} and {here}; use each sentence for one point only."
                                            ));
                                        }
                                    }
                                },
                            }
                        }
                        let w = words(&b.text);
                        if w > max_words {
                            warnings.push(format!(
                                "Slide {at}, bullet {}: {w} words; shorten it to at most {max_words}.",
                                j + 1
                            ));
                        }
                        if let Some(u) = &units {
                            cites_checked += 1;
                            match &b.cite {
                                None => problems.push(format!(
                                    "Slide {at}, bullet {} has no source; add ref (the {} number) and quote (words copied exactly from that {}).",
                                    j + 1,
                                    u.label,
                                    u.label
                                )),
                                Some((r, q)) => match check_citation(u, *r, q) {
                                    Ok(()) => cites_ok += 1,
                                    Err(why) => problems.push(format!(
                                        "Slide {at}, bullet {}: {why}.",
                                        j + 1
                                    )),
                                },
                            }
                        }
                    }
                    for text in texts {
                        if let Some(m) = placeholder_in(text) {
                            problems.push(format!(
                                "Slide {at} still contains template text {m:?}; replace it or write [needs input]."
                            ));
                        }
                        for num in g.ungrounded(text) {
                            problems.push(format!(
                                "Slide {at}: {num} does not appear in the source; remove it or use a figure from the source."
                            ));
                        }
                    }
                }
            }
        }
        Ok(json!({
            "ok": problems.is_empty(),
            "problems": problems,
            "warnings": warnings,
            "normalized": notes,
            "bullets_checked": bullets_seen,
            "citations_checked": cites_checked,
            "citations_verified": cites_ok,
        }))
    }
}

// ---------------------------------------------------------------------------
// document.verify_draft

pub struct DocumentVerifyDraft {
    spec: ToolSpec,
}

impl DocumentVerifyDraft {
    pub fn new() -> Self {
        DocumentVerifyDraft {
            spec: ToolSpec {
                id: "document.verify_draft".into(),
                description: "Check a drafted document against its template before it is built: every required section present and in order with no extras, no empty section, no template text, paragraphs of readable length, and every number grounded in the brief. Counts the [needs input] markers left for the author.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "draft": {"type": "object"},
                        "required_headings": {"type": "array", "maxItems": 20, "items": {"type": "string", "minLength": 1, "maxLength": 80}},
                        "source": {"type": "string", "maxLength": 2000000},
                        "max_paragraph_words": {"type": "integer", "minimum": 20, "maximum": 1000},
                        "layout": {"enum": ["headed", "letter"]}
                    },
                    "required": ["draft", "source"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for DocumentVerifyDraft {
    fn default() -> Self {
        Self::new()
    }
}

fn heading_key(h: &str) -> String {
    h.trim().trim_end_matches(':').trim().to_lowercase()
}

impl Tool for DocumentVerifyDraft {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let source = s(args, "source");
        let max_words = args
            .get("max_paragraph_words")
            .and_then(Value::as_u64)
            .unwrap_or(180) as usize;
        let required: Vec<String> = args
            .get("required_headings")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let letter = args.get("layout").and_then(Value::as_str) == Some("letter");
        let mut problems: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut needs_input = 0usize;
        let mut word_count = 0usize;
        match DocumentSpec::from_value(&args["draft"]) {
            Err(p) => problems.extend(p),
            Ok(mut doc) => {
                // A letter's salutation and sign-off are written by Harbor
                // from the inputs; check the body as it will be printed.
                if letter {
                    for s in &mut doc.sections {
                        s.paragraphs = s
                            .paragraphs
                            .iter()
                            .map(|p| strip_letter_furniture(p))
                            .filter(|p| !p.is_empty())
                            .collect();
                    }
                }
                // The same sentence in two sections says it twice.
                let mut sentence_home: BTreeMap<String, String> = BTreeMap::new();
                for s in &doc.sections {
                    for p in &s.paragraphs {
                        for line in p.lines() {
                            for sentence in split_sentences(line) {
                                if sentence.split_whitespace().count() < 6 {
                                    continue;
                                }
                                let key = normalise(&sentence);
                                match sentence_home.get(&key) {
                                    Some(home) if *home != s.heading => warnings.push(format!(
                                        "Section {:?} repeats a sentence from section {home:?}; say it once.",
                                        s.heading
                                    )),
                                    Some(_) => {}
                                    None => {
                                        sentence_home.insert(key, s.heading.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                let g = Grounding::of(&source);
                if !required.is_empty() {
                    let have: Vec<String> = doc
                        .sections
                        .iter()
                        .map(|s| heading_key(&s.heading))
                        .collect();
                    let want: Vec<String> = required.iter().map(|h| heading_key(h)).collect();
                    for (i, w) in want.iter().enumerate() {
                        if !have.contains(w) {
                            problems.push(format!(
                                "Section {:?} is missing; add it (write [needs input] if the brief has nothing for it).",
                                required[i]
                            ));
                        }
                    }
                    for s in &doc.sections {
                        if !want.contains(&heading_key(&s.heading)) {
                            problems.push(format!(
                                "Section {:?} is not part of this template; use exactly these headings: {}.",
                                s.heading,
                                required.join(", ")
                            ));
                        }
                    }
                    let order: Vec<&String> = have.iter().filter(|h| want.contains(h)).collect();
                    let expected: Vec<&String> = want.iter().filter(|w| have.contains(w)).collect();
                    if order != expected {
                        problems.push(format!(
                            "Put the sections in this order: {}.",
                            required.join(", ")
                        ));
                    }
                }
                for text in std::iter::once(doc.title.as_str()) {
                    if let Some(m) = placeholder_in(text) {
                        problems.push(format!("The title still contains template text {m:?}."));
                    }
                    for n in g.ungrounded(text) {
                        problems.push(format!(
                            "The title uses {n}, which does not appear in the brief."
                        ));
                    }
                }
                for s in &doc.sections {
                    if s.paragraphs.is_empty() {
                        problems.push(format!(
                            "Section {:?} has no text; write [needs input] if the brief has nothing for it.",
                            s.heading
                        ));
                    }
                    for (k, p) in s.paragraphs.iter().enumerate() {
                        let w = words(p);
                        word_count += w;
                        needs_input += p.matches("[needs input]").count();
                        if w > max_words {
                            warnings.push(format!(
                                "Section {:?}, paragraph {}: {w} words; shorten it to under {max_words}.",
                                s.heading,
                                k + 1
                            ));
                        }
                        if let Some(m) = placeholder_in(p) {
                            problems.push(format!(
                                "Section {:?} still contains template text {m:?}; replace it or write [needs input].",
                                s.heading
                            ));
                        }
                        for n in g.ungrounded(p) {
                            problems.push(format!(
                                "Section {:?}: {n} does not appear in the brief; remove it or write [needs input].",
                                s.heading
                            ));
                        }
                    }
                }
            }
        }
        warnings.dedup();
        Ok(json!({
            "ok": problems.is_empty(),
            "problems": problems,
            "warnings": warnings,
            "needs_input": needs_input,
            "word_count": word_count,
        }))
    }
}

// ---------------------------------------------------------------------------
// email.verify_draft / email.render

fn attachment_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(attached|attachment|attachments|enclosed)\b")
            .expect("attachment regex")
    })
}

fn sent_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\b(i have sent|i've sent|i sent|we have sent|we've sent|we sent|has been sent|have been sent|was sent|were sent)\b",
        )
        .expect("sent regex")
    })
}

const GREETING_WORDS: &[&str] = &[
    "hi",
    "hello",
    "dear",
    "hey",
    "good",
    "morning",
    "afternoon",
    "evening",
    "greetings",
    "mr",
    "mrs",
    "ms",
    "dr",
    "prof",
    "sir",
    "madam",
    "team",
    "all",
    "everyone",
    "folks",
    "colleagues",
    "and",
];

/// Names a greeting addresses ("Hi Amina," → Amina). Latin script: the
/// capitalised words that are not greeting words or titles. Other scripts:
/// the words after the first.
fn greeting_names(greeting: &str) -> Vec<String> {
    let tokens: Vec<&str> = greeting
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.iter().any(|t| !t.is_ascii()) {
        return tokens
            .iter()
            .skip(1)
            .filter(|t| t.chars().count() >= 2)
            .map(|t| t.to_string())
            .collect();
    }
    tokens
        .iter()
        .filter(|t| t.chars().next().map(char::is_uppercase).unwrap_or(false))
        .filter(|t| !GREETING_WORDS.contains(&t.to_lowercase().as_str()))
        .map(|t| t.to_string())
        .collect()
}

pub struct EmailVerifyDraft {
    spec: ToolSpec,
}

impl EmailVerifyDraft {
    pub fn new() -> Self {
        EmailVerifyDraft {
            spec: ToolSpec {
                id: "email.verify_draft".into(),
                description: "Check a drafted email before it is shown: a subject and a body of readable length, every name in the greeting and every figure or date grounded in the thread and notes, no mention of attachments when nothing is attached, no claim that anything was sent unless the notes say so, no template text. Harbor drafts only; nothing is ever sent.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "draft": {"type": "object"},
                        "source": {"type": "string", "maxLength": 2000000},
                        "sources": {"type": "array", "maxItems": 8, "items": {"type": ["string", "null"], "maxLength": 2000000}},
                        "attachments": {"type": "boolean"}
                    },
                    "required": ["draft"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for EmailVerifyDraft {
    fn default() -> Self {
        Self::new()
    }
}

fn paragraphs_of(draft: &Value) -> Vec<String> {
    draft
        .get("paragraphs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

impl Tool for EmailVerifyDraft {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let draft = &args["draft"];
        // The thread and the user's notes are separate inputs; either may
        // be absent (a fresh email has no thread).
        let mut source = s(args, "source");
        for extra in args
            .get("sources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            source.push('\n');
            source.push_str(extra);
        }
        let attachments = args
            .get("attachments")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let subject = s(draft, "subject").trim().to_string();
        let greeting = s(draft, "greeting").trim().to_string();
        let paragraphs = paragraphs_of(draft);
        let mut problems = Vec::new();
        if subject.is_empty() {
            problems.push("The subject is empty; write a short subject.".to_string());
        } else if subject.chars().count() > 120 {
            problems.push("The subject is over 120 characters; shorten it.".to_string());
        }
        if paragraphs.is_empty() {
            problems.push("The draft has no body.".to_string());
        } else if paragraphs.len() > 6 {
            problems.push(format!(
                "The body has {} paragraphs; keep it to 6 or fewer.",
                paragraphs.len()
            ));
        }
        let lower_source = source.to_lowercase();
        for name in greeting_names(&greeting) {
            if !lower_source.contains(&name.to_lowercase()) {
                problems.push(format!(
                    "The greeting names {name:?}, but the thread and notes never mention {name}; greet someone who is named there, or use a neutral greeting."
                ));
            }
        }
        let g = Grounding::of(&source);
        let all: Vec<&str> = std::iter::once(subject.as_str())
            .chain(std::iter::once(greeting.as_str()))
            .chain(paragraphs.iter().map(String::as_str))
            .collect();
        let mut ungrounded = Vec::new();
        for text in &all {
            for n in g.ungrounded(text) {
                if !ungrounded.contains(&n) {
                    ungrounded.push(n);
                }
            }
            if let Some(m) = placeholder_in(text) {
                problems.push(format!(
                    "The draft still contains template text {m:?}; write the real value or leave it out."
                ));
            }
        }
        for n in &ungrounded {
            problems.push(format!(
                "{n} does not appear in the thread or notes; do not introduce figures, dates or amounts."
            ));
        }
        let body = all.join("\n");
        if !attachments {
            if let Some(m) = attachment_re().find(&body) {
                problems.push(format!(
                    "The draft mentions {:?}, but nothing is attached; remove that sentence.",
                    m.as_str()
                ));
            }
        }
        if let Some(m) = sent_re().find(&body) {
            if !lower_source.contains(&m.as_str().to_lowercase()) {
                problems.push(format!(
                    "The draft says {:?}; do not claim anything was sent unless the notes say so.",
                    m.as_str()
                ));
            }
        }
        Ok(json!({
            "ok": problems.is_empty(),
            "problems": problems,
            "ungrounded_numbers": ungrounded,
        }))
    }
}

pub struct EmailRender {
    spec: ToolSpec,
}

impl EmailRender {
    pub fn new() -> Self {
        EmailRender {
            spec: ToolSpec {
                id: "email.render".into(),
                description: "Lay out a verified email draft as the final text (subject line, greeting, paragraphs, sign-off and the sender's name). Formatting only; nothing is sent.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "draft": {"type": "object"},
                        "sender_name": {"type": ["string", "null"], "maxLength": 120}
                    },
                    "required": ["draft"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 2_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for EmailRender {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for EmailRender {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let draft = &args["draft"];
        let subject = s(draft, "subject").trim().to_string();
        let greeting = s(draft, "greeting").trim().to_string();
        let mut sign_off = s(draft, "sign_off").trim().to_string();
        if sign_off.is_empty() {
            sign_off = "Best regards".into();
        }
        if !sign_off.ends_with([',', '.', '!', '،']) {
            sign_off.push(',');
        }
        let mut parts = Vec::new();
        if !greeting.is_empty() {
            parts.push(greeting);
        }
        parts.extend(paragraphs_of(draft));
        let mut closing = sign_off;
        if let Some(name) = args
            .get("sender_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            closing.push('\n');
            closing.push_str(name);
        }
        parts.push(closing);
        let body = parts.join("\n\n");
        Ok(json!({
            "subject": subject,
            "body": body,
            "text": format!("Subject: {subject}\n\n{body}"),
            "sent": false,
        }))
    }
}

// ---------------------------------------------------------------------------
// text.verify_items

pub struct TextVerifyItems {
    spec: ToolSpec,
}

impl TextVerifyItems {
    pub fn new() -> Self {
        TextVerifyItems {
            spec: ToolSpec {
                id: "text.verify_items".into(),
                description: "Verify named string fields of every item against the source in one call: a value that does not occur verbatim is nulled, and with `anchor_field` a value that is not stated near the item's anchor text (same line or the line before or after) is nulled too. Returns the verified items, what was dropped and why, and a problem sentence per drop.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "source": {"type": "string", "maxLength": 2000000},
                        "items": {"type": ["array", "null"], "maxItems": 200, "items": {"type": "object"}},
                        "fields": {"type": "array", "minItems": 1, "maxItems": 16, "items": {"type": "string", "minLength": 1}},
                        "anchor_field": {"type": "string", "minLength": 1},
                        "label": {"type": "string", "maxLength": 40}
                    },
                    "required": ["source", "items", "fields"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for TextVerifyItems {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for TextVerifyItems {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let source = s(args, "source");
        let label = args
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("item")
            .to_string();
        let anchor = args
            .get("anchor_field")
            .and_then(Value::as_str)
            .map(str::to_string);
        let fields: Vec<String> = args["fields"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut results = Vec::new();
        let mut problems = Vec::new();
        for (i, item) in args["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let mut verified = item.clone();
            let mut dropped = Vec::new();
            let anchor_text = anchor
                .as_ref()
                .and_then(|a| item.get(a))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for f in &fields {
                let Some(Value::String(v)) = item.get(f) else {
                    continue;
                };
                if v.trim().is_empty() {
                    continue;
                }
                let reason = if !source.contains(v.as_str()) {
                    problems.push(format!(
                        "{} {} {f} {v:?} does not occur in the source. Copy it exactly as it is written there, or use null.",
                        capitalise(&label),
                        i + 1
                    ));
                    Some("not_in_source")
                } else if anchor.is_some() && !attributed(&source, v, &anchor_text) {
                    problems.push(format!(
                        "{} {} {f} {v:?} is not stated next to {anchor_text:?} in the source. Use null unless the source says so.",
                        capitalise(&label),
                        i + 1
                    ));
                    Some("not_attributed")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    dropped.push(json!({"field": f, "value": v, "reason": reason}));
                    if let Some(obj) = verified.as_object_mut() {
                        obj.insert(f.clone(), Value::Null);
                    }
                }
            }
            results.push(json!({"item": verified, "dropped": dropped}));
        }
        Ok(json!({
            "ok": problems.is_empty(),
            "results": results,
            "problems": problems,
        }))
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// text.verify_citations / document.units

pub struct TextVerifyCitations {
    spec: ToolSpec,
}

impl TextVerifyCitations {
    pub fn new() -> Self {
        TextVerifyCitations {
            spec: ToolSpec {
                id: "text.verify_citations".into(),
                description: "Check citations against a document read by artifact.read: each citation names a unit (PDF page, DOCX paragraph, slide or text line) and a quote, and passes only when that unit's text contains the quote (whitespace, case and typographic quotes normalised). Reports where a misplaced quote actually is.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "document": {"type": "object"},
                        "citations": {"type": "array", "maxItems": 200, "items": {"type": "object"}}
                    },
                    "required": ["document", "citations"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: MB,
            },
        }
    }
}

impl Default for TextVerifyCitations {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for TextVerifyCitations {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let units = units_of(&args["document"]);
        let mut results = Vec::new();
        let mut problems = Vec::new();
        for (i, c) in args["citations"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let r = c.get("ref").and_then(Value::as_i64);
            let q = c.get("quote").and_then(Value::as_str).unwrap_or("");
            let outcome = match r {
                Some(r) => check_citation(&units, r, q),
                None => Err(format!("citation {} names no {}", i + 1, units.label)),
            };
            match &outcome {
                Ok(()) => results.push(json!({"ref": r, "quote": q, "verified": true})),
                Err(why) => {
                    problems.push(format!("Citation {}: {why}.", i + 1));
                    results.push(json!({"ref": r, "quote": q, "verified": false, "reason": why}));
                }
            }
        }
        Ok(json!({
            "ok": problems.is_empty(),
            "unit": units.label,
            "results": results,
            "problems": problems,
        }))
    }
}

pub struct DocumentUnits {
    spec: ToolSpec,
}

impl DocumentUnits {
    pub fn new() -> Self {
        DocumentUnits {
            spec: ToolSpec {
                id: "document.units".into(),
                description: "Number a document's units for citation: PDF pages, DOCX paragraphs, slides or text lines, as compact `[n] text` lines (empty units skipped, numbering kept) with the unit label. The numbers are the ones text.verify_citations and deck.verify_outline check against.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {"document": {"type": "object"}},
                    "required": ["document"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 5_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for DocumentUnits {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DocumentUnits {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let units = units_of(&args["document"]);
        let mut lines = Vec::new();
        for (n, text) in &units.units {
            let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !t.is_empty() {
                lines.push(format!("[{n}] {t}"));
            }
        }
        Ok(json!({
            "unit": units.label,
            "count": units.units.len(),
            "numbered_text": lines.join("\n"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{MemoryArtifacts, ToolRegistry};
    use std::sync::atomic::AtomicBool;

    fn call(tool: &str, args: Value) -> Value {
        let artifacts = MemoryArtifacts::new();
        let cancel = AtomicBool::new(false);
        let host = json!({});
        let ctx = ToolContext::new(&artifacts, &host, &cancel);
        let allow: BTreeSet<String> = [tool.to_string()].into_iter().collect();
        ToolRegistry::builtin()
            .call(&ctx, tool, &args, &allow)
            .unwrap_or_else(|e| panic!("{tool}: {e}"))
            .output
    }

    #[test]
    fn grounding_matches_restatements_and_flags_inventions() {
        let g = Grounding::of("Rent is $1,200 a month; groceries about 400. Crash-free at 99.4%.");
        assert!(g.has_span("1200"));
        assert!(g.has_span("$1,200.00") || g.has_value(1200.0));
        assert!(g.has_value(0.994), "percent restated as a fraction");
        assert!(g.has_span("400"));
        assert_eq!(
            g.ungrounded("Crash-free at 99.7%, rent 1,200"),
            vec!["99.7%"]
        );
        assert_eq!(
            placeholder_in("Dear [Your name],"),
            Some("[Your name]".into())
        );
        assert_eq!(placeholder_in("The budget is [needs input]."), None);
        assert_eq!(placeholder_in("Lorem ipsum"), Some("Lorem".into()));
    }

    #[test]
    fn workbook_spec_verification_grounds_numbers_and_suggests_labels() {
        let out = call(
            "workbook.verify_spec",
            json!({
                "source": "Monthly budget: rent 1200, groceries 400, internet 60.",
                "spec": {
                    "title": "Monthly budget", "sheet": "Budget",
                    "columns": [{"header": "Item", "type": "text"}, {"header": "Amount", "type": "currency"}],
                    "rows": [["Rent", 1200], ["Groceries", "400"], ["Gym", 45], ["Internet", null]],
                    "total_columns": ["Amount"]
                }
            }),
        );
        assert_eq!(out["ok"], false);
        let problems = out["problems"].as_array().unwrap();
        assert_eq!(problems.len(), 1, "{out}");
        assert!(
            problems[0].as_str().unwrap().contains("Row 3 (Gym): 45"),
            "{out}"
        );
        assert_eq!(out["suggested_labels"], json!(["Gym"]));
        let ok = call(
            "workbook.verify_spec",
            json!({
                "source": "Track job applications: company, role, status, date applied.",
                "spec": {"title": "Job applications", "sheet": "Applications",
                         "columns": [{"header": "Company", "type": "text"}, {"header": "Status", "type": "text"}],
                         "rows": []}
            }),
        );
        assert_eq!(ok["ok"], true, "{ok}");
    }

    #[test]
    fn deck_outline_checks_count_structure_numbers_and_citations() {
        let document = json!({
            "kind": "pdf",
            "pages": [
                {"index": 1, "text": "Annual report 2025. Revenue grew 12% to $4.2m."},
                {"index": 2, "text": "Churn fell to 3.1% after the onboarding redesign."}
            ]
        });
        let out = call(
            "deck.verify_outline",
            json!({
                "source": "Annual report 2025. Revenue grew 12% to $4.2m.\nChurn fell to 3.1% after the onboarding redesign.",
                "document": document,
                "min_slides": 2, "max_slides": 2,
                "outline": {"title": "2025 in review", "slides": [
                    {"title": "Growth", "bullets": [
                        {"text": "Revenue up 12%", "ref": 1, "quote": "Revenue grew 12% to $4.2m"},
                        {"text": "Churn down to 3.1%", "ref": 1, "quote": "Churn fell to 3.1%"}
                    ]},
                    {"title": "Growth", "bullets": [{"text": "Margin 40%", "ref": 2, "quote": "margin"}]}
                ]}
            }),
        );
        assert_eq!(out["ok"], false);
        let p: Vec<String> = out["problems"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        let joined = p.join("\n");
        assert!(
            joined.contains("the quote is on page 2, not page 1"),
            "{joined}"
        );
        assert!(out["warnings"].to_string().contains("same title"), "{out}");
        assert!(joined.contains("too short to check"), "{joined}");
        assert!(joined.contains("40%"), "{joined}");
        assert_eq!(out["citations_verified"], 1);
        let units = call("document.units", json!({"document": document}));
        assert_eq!(units["unit"], "page");
        assert!(units["numbered_text"]
            .as_str()
            .unwrap()
            .starts_with("[1] Annual report"));
    }

    #[test]
    fn sentence_cited_bullets_must_state_what_their_sentence_says() {
        let sentences = json!({
            "unit": "page",
            "sentences": [
                {"n": 1, "unit": 1, "text": "Patient visits grew 14% to 48,200 across the six clinics."},
                {"n": 2, "unit": 2, "text": "Operating revenue was AED 6.4m, 9% above plan."},
                {"n": 3, "unit": 2, "text": "Staff costs rose because of two new night shifts."}
            ]
        });
        let source = "Q3. Patient visits grew 14% to 48,200 across the six clinics. Operating revenue was AED 6.4m, 9% above plan. Staff costs rose because of two new night shifts.";
        let out = call(
            "deck.verify_outline",
            json!({
                "source": source,
                "sentences": sentences,
                "slides": 2,
                "outline": {"title": "Q3", "slides": [
                    {"title": "Visits", "bullets": [{"text": "Visits up 14%", "source": 1}, {"text": "Costs up 12%", "source": 3}]},
                    {"title": "Money", "bullets": [{"text": "Revenue AED 6.4m", "source": 2}, {"text": "Beat plan", "source": 7}, {"text": "No source"}]}
                ]}
            }),
        );
        let joined = out["problems"].to_string();
        assert!(joined.contains("12% is not in sentence [3]"), "{joined}");
        assert!(joined.contains("sentence [7] does not exist"), "{joined}");
        assert!(joined.contains("bullet 3 has no source"), "{joined}");
        assert_eq!(out["citations_verified"], 2, "{out}");
        // Repeats are removed, not argued about: a bullet restating an
        // earlier one, or citing an already-cited sentence, is dropped and
        // a slide left empty goes with it.
        let dup = call(
            "deck.verify_outline",
            json!({
                "source": source,
                "sentences": sentences,
                "slides": 2,
                "outline": {"title": "Q3", "slides": [
                    {"title": "Visits", "bullets": [{"text": "Visits up 14%", "source": 1}]},
                    {"title": "Money", "bullets": [{"text": "Revenue AED 6.4m", "source": 2}]},
                    {"title": "Visits", "bullets": [{"text": "Visits up 14%", "source": 1}]}
                ]}
            }),
        );
        assert_eq!(dup["ok"], true, "{dup}");
        assert!(
            dup["normalized"]
                .to_string()
                .contains("slide 3 only repeated earlier slides"),
            "{dup}"
        );
    }

    #[test]
    fn letters_are_checked_as_printed_and_repetition_is_advisory() {
        let draft = json!({"title": "Lease renewal", "sections": [
            {"heading": "Opening", "paragraphs": ["Dear Sir/Madam,\n\nWe want to renew the lease on Unit 4 for two more years.\n\nYours sincerely,\n[Your Name]"]},
            {"heading": "Body", "paragraphs": ["We want to renew the lease on Unit 4 for two more years."]},
            {"heading": "Closing", "paragraphs": ["Please send the contract."]}
        ]});
        let letter = call(
            "document.verify_draft",
            json!({"source": "Renew the lease on Unit 4 for two more years.", "layout": "letter",
                   "required_headings": ["Opening", "Body", "Closing"], "draft": draft}),
        );
        let joined = letter["problems"].to_string();
        assert!(
            !joined.contains("Your Name"),
            "the sign-off is Harbor's: {joined}"
        );
        assert_eq!(letter["ok"], true, "{letter}");
        assert!(
            letter["warnings"]
                .to_string()
                .contains("repeats a sentence from section"),
            "{letter}"
        );
        let headed = call(
            "document.verify_draft",
            json!({"source": "Renew the lease on Unit 4 for two more years.",
                   "required_headings": ["Opening", "Body", "Closing"], "draft": draft}),
        );
        assert!(
            headed["problems"].to_string().contains("Your Name"),
            "{headed}"
        );
    }

    #[test]
    fn document_draft_follows_the_template() {
        let out = call(
            "document.verify_draft",
            json!({
                "source": "Proposal: move the archive to local storage. Budget $42,000. Two engineers for six weeks.",
                "required_headings": ["Summary", "Budget", "Next steps"],
                "draft": {"title": "Archive proposal", "sections": [
                    {"heading": "Budget", "paragraphs": ["The move costs $42,000 and $5,000 for hardware."]},
                    {"heading": "Summary", "paragraphs": ["Move the archive. [needs input]"]},
                    {"heading": "Risks", "paragraphs": ["TBD"]}
                ]}
            }),
        );
        let joined = out["problems"].to_string();
        assert!(joined.contains("Next steps"), "{joined}");
        assert!(joined.contains("not part of this template"), "{joined}");
        assert!(joined.contains("in this order"), "{joined}");
        assert!(joined.contains("5,000"), "{joined}");
        assert!(joined.contains("TBD"), "{joined}");
        assert_eq!(out["needs_input"], 1);
    }

    #[test]
    fn email_checks_names_figures_attachments_and_sent_claims_then_renders() {
        let source = "From: Amina Khan\nCan you confirm the delivery date for order 4471? We need it before 3 October.\nNotes: confirm 30 September delivery.";
        let bad = call(
            "email.verify_draft",
            json!({"source": source, "draft": {
                "subject": "Re: order 4471",
                "greeting": "Hi Omar,",
                "paragraphs": ["Delivery is confirmed for 30 September; I have sent the invoice.", "Please see the attached schedule and the 10% discount."],
                "sign_off": "Best regards"
            }}),
        );
        let joined = bad["problems"].to_string();
        assert!(joined.contains("\\\"Omar\\\""), "{joined}");
        assert!(joined.contains("10%"), "{joined}");
        assert!(joined.contains("attached"), "{joined}");
        assert!(joined.contains("I have sent"), "{joined}");
        let good_draft = json!({
            "subject": "Re: order 4471",
            "greeting": "Hi Amina,",
            "paragraphs": ["Delivery of order 4471 is confirmed for 30 September, before your 3 October deadline."],
            "sign_off": "Best regards"
        });
        let good = call(
            "email.verify_draft",
            json!({"source": source, "draft": good_draft}),
        );
        assert_eq!(good["ok"], true, "{good}");
        let rendered = call(
            "email.render",
            json!({"draft": good_draft, "sender_name": "Omar Haddad"}),
        );
        assert_eq!(
            rendered["text"],
            "Subject: Re: order 4471\n\nHi Amina,\n\nDelivery of order 4471 is confirmed for 30 September, before your 3 October deadline.\n\nBest regards,\nOmar Haddad"
        );
        assert_eq!(rendered["sent"], false);
    }

    #[test]
    fn items_are_verified_verbatim_and_by_attribution() {
        let transcript = "Sara: Let's decide on the archive migration.\nOmar: Agreed. I can write the migration plan by Friday 26 September.\nSara: Also, someone should update the runbook — no owner yet.";
        let out = call(
            "text.verify_items",
            json!({
                "source": transcript,
                "items": [
                    {"text": "Write the migration plan", "owner": "Omar", "due": "2023-09-26"},
                    {"text": "Update the runbook", "owner": "Omar", "due": null}
                ],
                "fields": ["owner", "due"],
                "anchor_field": "text",
                "label": "action"
            }),
        );
        assert_eq!(out["ok"], false);
        assert_eq!(out["results"][0]["item"]["owner"], "Omar");
        assert!(out["results"][0]["item"]["due"].is_null());
        assert_eq!(out["results"][0]["dropped"][0]["reason"], "not_in_source");
        assert!(out["results"][1]["item"]["owner"].is_null());
        assert_eq!(out["results"][1]["dropped"][0]["reason"], "not_attributed");
        let joined = out["problems"].to_string();
        assert!(joined.contains("Action 1 due"), "{joined}");
        assert!(joined.contains("Action 2 owner"), "{joined}");
        // Arabic: short stems still anchor an owner to the action.
        let ar = "عمر: موافق. سأكتب خطة النقل بحلول الجمعة 26 سبتمبر.";
        assert!(attributed(ar, "عمر", "كتابة خطة النقل"));
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use crate::tools::{MemoryArtifacts, ToolRegistry};
    use std::sync::atomic::AtomicBool;

    /// The report-to-slides fixture keeps its figures on known pages, and
    /// the numbering document.units gives is the numbering citations use.
    #[test]
    fn quarterly_report_pages_are_numbered_for_citation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(root.join("fixtures/office/quarterly_report.pdf")).unwrap();
        let artifacts = MemoryArtifacts::new().with("r", "quarterly_report.pdf", bytes);
        let cancel = AtomicBool::new(false);
        let host = json!({});
        let ctx = ToolContext::new(&artifacts, &host, &cancel);
        let allow: BTreeSet<String> =
            ["artifact.read".to_string(), "document.units".to_string()].into();
        let r = ToolRegistry::builtin();
        let doc = r
            .call(&ctx, "artifact.read", &json!({"artifact_id": "r"}), &allow)
            .unwrap()
            .output;
        assert_eq!(doc["page_count"], 3, "{doc}");
        let units = r
            .call(&ctx, "document.units", &json!({"document": doc}), &allow)
            .unwrap()
            .output;
        println!("{}", units["numbered_text"].as_str().unwrap());
        let u = units_of(&doc);
        assert!(check_citation(&u, 1, "Patient visits grew 14% to 48,200").is_ok());
        assert!(check_citation(&u, 2, "Operating revenue was AED 6.4m, 9% above plan").is_ok());
        assert!(check_citation(&u, 3, "Nurse turnover reached 11%").is_ok());
        assert!(check_citation(&u, 1, "Nurse turnover reached 11%")
            .unwrap_err()
            .contains("page 3"));
        let allow: BTreeSet<String> = ["document.sentences".to_string()].into();
        let sentences = r
            .call(
                &ctx,
                "document.sentences",
                &json!({"document": doc}),
                &allow,
            )
            .unwrap()
            .output;
        println!("{}", sentences["numbered_text"].as_str().unwrap());
        // Headings and the report title are context, not citeable; every
        // statement is numbered with its page.
        assert_eq!(sentences["count"], 9, "{sentences}");
        let wrapped = sentences_of(&Units {
            label: "page",
            units: [(
                1,
                "Two clinics still run on the old\nbooking system.\nFinances\nRevenue rose by 9%."
                    .to_string(),
            )]
            .into(),
        });
        assert_eq!(
            wrapped.0[0].text,
            "Two clinics still run on the old booking system."
        );
        assert_eq!(wrapped.0.len(), 2);
        assert_eq!(sentences["sentences"][0]["unit"], 1);
        assert!(sentences["sentences"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Patient visits grew 14%"));
        assert_eq!(sentences["sentences"][8]["unit"], 3);
        assert!(sentences["numbered_text"]
            .as_str()
            .unwrap()
            .contains("Page 2\nFinances\n[4] Operating revenue"));
    }
}
