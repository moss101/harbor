//! Deterministic review tools (production plan B3). Every check that a
//! small model gets wrong is done here, in code, against the artifact IR:
//! the model only ranks, explains and drafts replacements, and every
//! model output is verified below it. Each tool reports `not_checked`
//! entries for the principles the IR cannot see, so the user is never told
//! a deck, workbook or document is clean on a dimension nobody looked at.
//!
//! - `deck.inspect` — placeholder remnants, unsourced numeric claims,
//!   repeated slide structure, overloaded/empty slides, slide kinds.
//! - `workbook.conventions` — hard-coded constants inside formula rows and
//!   columns (with a shifted neighbour formula that is verified through
//!   the pinned engine), links to other files.
//! - `docx.inspect` — heading hierarchy, direct-formatting overrides
//!   (style contamination), mixed-direction paragraphs, margins and heading
//!   size ratios from styles.xml when present.
//! - `text.verify_numbers` — every number in a text occurs in the source
//!   (the "no invented metrics" check).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use harbor_artifacts::{DocxDocument, PptxDeck, WorkbookDoc};
use harbor_formula::value::CellValue;

use super::builtin::{cell_content_hash, cell_target_id, detect_kind, ArtifactKind};
use super::{RiskClass, Tool, ToolContext, ToolError, ToolSpec};

const MB: usize = 1024 * 1024;

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn artifact(
    ctx: &ToolContext<'_>,
    tool: &str,
    args: &Value,
) -> Result<(String, super::ArtifactBytes), ToolError> {
    let id =
        s(args, "artifact_id").ok_or_else(|| ToolError::failed(tool, "artifact_id missing"))?;
    let a = ctx.artifacts.get(&id).ok_or_else(|| {
        ToolError::Unavailable(
            tool.into(),
            format!("artifact {id} is not attached to this run"),
        )
    })?;
    Ok((id, a))
}

fn artifact_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "max_findings": {"type": "integer", "minimum": 1, "maximum": 10000}
        },
        "required": ["artifact_id"],
        "additionalProperties": false
    })
}

/// Numbers as a reader sees them: `12%`, `$540k`, `1,250.00`, `2026`,
/// `3.5x`, `18`. Returns the matched spans verbatim.
pub fn numbers_in(text: &str) -> Vec<String> {
    let re = number_re();
    re.find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|n| n.chars().any(|c| c.is_ascii_digit()))
        .collect()
}

fn number_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"[$€£]?\d(?:[\d,]*\d)?(?:\.\d+)?\s?(?:%|x|k|K|m|M|bn|B)?")
            .expect("number regex")
    })
}

/// Digits-only key of a number span so `$540k` in the output matches
/// `540k` or `540,000`-free prose in the source only when the digits agree.
fn number_key(span: &str) -> String {
    span.chars().filter(|c| c.is_ascii_digit()).collect()
}

// ---------------------------------------------------------------------------
// text.verify_numbers

pub struct TextVerifyNumbers {
    spec: ToolSpec,
}

impl TextVerifyNumbers {
    pub fn new() -> Self {
        TextVerifyNumbers {
            spec: ToolSpec {
                id: "text.verify_numbers".into(),
                description: "Report every number in `texts` whose digits do not occur in `source`; the deterministic 'no invented metrics' check below a drafting model.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "source": {"type": "string", "maxLength": 2000000},
                        "texts": {"type": "array", "maxItems": 200, "items": {"type": "string", "maxLength": 20000}}
                    },
                    "required": ["source", "texts"],
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

impl Default for TextVerifyNumbers {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for TextVerifyNumbers {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let source = s(args, "source").unwrap_or_default();
        let source_keys: BTreeSet<String> = numbers_in(&source)
            .iter()
            .map(|n| number_key(n))
            .filter(|k| !k.is_empty())
            .collect();
        let mut checked = 0usize;
        let mut missing = Vec::new();
        for (i, t) in args["texts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let Some(text) = t.as_str() else { continue };
            for n in numbers_in(text) {
                let key = number_key(&n);
                if key.is_empty() {
                    continue;
                }
                checked += 1;
                if !source_keys.contains(&key) {
                    missing.push(json!({"index": i, "number": n.trim()}));
                }
            }
        }
        Ok(json!({
            "grounded": missing.is_empty(),
            "checked": checked,
            "missing": missing,
        }))
    }
}

// ---------------------------------------------------------------------------
// deck.inspect

pub struct DeckInspect {
    spec: ToolSpec,
    placeholder_re: regex::Regex,
    source_re: regex::Regex,
}

impl DeckInspect {
    pub fn new() -> Self {
        DeckInspect {
            spec: ToolSpec {
                id: "deck.inspect".into(),
                description: "Deterministic deck QA over the presentation IR: slide kinds, placeholder remnants, numeric claims without a source in the notes, consecutive slides with identical structure, overloaded and empty slides. Reports what the IR cannot check.".into(),
                args_schema: artifact_args_schema(),
                risk: RiskClass::Read,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 30_000,
                max_output_bytes: 8 * MB,
            },
            placeholder_re: regex::Regex::new(
                r"(?i)\b(lorem|ipsum|xxx+|sample text|click to add|click to edit|placeholder|tbd|todo|\[insert[^\]]*\]|title layout|content layout)\b",
            )
            .expect("placeholder regex"),
            source_re: regex::Regex::new(r"(?i)(source|src|according to|per |https?://|www\.|©|report|survey|data:)")
                .expect("source regex"),
        }
    }

    fn classify(
        &self,
        index: usize,
        total: usize,
        title: &str,
        bullets: &[String],
    ) -> &'static str {
        let t = title.to_ascii_lowercase();
        if index == 1 {
            return "cover";
        }
        if t.contains("agenda") || t.contains("contents") || t.contains("outline") {
            return "agenda";
        }
        if t.contains("summary")
            || t.contains("recap")
            || t.contains("next steps")
            || t.contains("conclusion")
            || t.contains("thank")
            || t.contains("q&a")
            || (index == total && bullets.len() <= 3)
        {
            return "summary";
        }
        if bullets.is_empty() || (bullets.len() == 1 && bullets[0].split_whitespace().count() <= 4)
        {
            return "section_divider";
        }
        "content"
    }
}

impl Default for DeckInspect {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DeckInspect {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let (id, a) = artifact(ctx, &tool, args)?;
        if detect_kind(&a.bytes) != ArtifactKind::Pptx {
            return Err(ToolError::failed(
                &tool,
                format!("artifact {id} is not a PPTX deck"),
            ));
        }
        let max_findings = args
            .get("max_findings")
            .and_then(Value::as_u64)
            .unwrap_or(500) as usize;
        let deck = PptxDeck::from_pptx_bytes(&a.bytes)
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let total = deck.slides.len();
        let mut slides = Vec::new();
        let mut findings = Vec::new();
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut all_text = Vec::new();
        let mut previous: Option<(usize, usize, bool, bool)> = None;
        for (i, sl) in deck.slides.iter().enumerate() {
            let index = i + 1;
            let kind = self.classify(index, total, &sl.title, &sl.bullets);
            let mut slide_findings: Vec<Value> = Vec::new();
            let mut push =
                |code: &str, severity: &str, detail: String, evidence: Option<String>| {
                    *counts.entry(code.to_string()).or_insert(0) += 1;
                    let f = json!({
                        "slide": index,
                        "code": code,
                        "severity": severity,
                        "detail": detail,
                        "evidence": evidence,
                    });
                    slide_findings.push(f);
                };
            let notes = sl.notes.clone().unwrap_or_default();
            let mut texts: Vec<(&str, String)> = vec![("title", sl.title.clone())];
            for b in &sl.bullets {
                texts.push(("bullet", b.clone()));
            }
            texts.push(("notes", notes.clone()));
            for (where_, text) in &texts {
                if let Some(m) = self.placeholder_re.find(text) {
                    push(
                        "placeholder_text",
                        "high",
                        format!("{where_} still contains template text \"{}\"", m.as_str()),
                        Some(text.clone()),
                    );
                }
            }
            all_text.push(sl.title.clone());
            all_text.extend(sl.bullets.iter().cloned());
            if !notes.is_empty() {
                all_text.push(notes.clone());
            }
            // Numeric claims on the slide need a source in the notes.
            let notes_has_source = self.source_re.is_match(&notes);
            let claims: Vec<String> = sl
                .bullets
                .iter()
                .chain(std::iter::once(&sl.title))
                .flat_map(|t| numbers_in(t))
                .map(|n| n.trim().to_string())
                .filter(|n| number_key(n).len() >= 2 || n.contains('%') || n.contains('$'))
                .collect();
            if !claims.is_empty() && !notes_has_source && kind != "cover" {
                push(
                    "unsourced_number",
                    "medium",
                    format!(
                        "{} numeric claim(s) without a source line in the notes: {}",
                        claims.len(),
                        claims.join(", ")
                    ),
                    None,
                );
            }
            if kind == "content" {
                if sl.bullets.len() > 7 {
                    push(
                        "overloaded_slide",
                        "low",
                        format!("{} bullets on one slide (keep to seven)", sl.bullets.len()),
                        None,
                    );
                }
                if sl.title.split_whitespace().count() > 12 {
                    push(
                        "long_title",
                        "low",
                        format!("title is {} words", sl.title.split_whitespace().count()),
                        Some(sl.title.clone()),
                    );
                }
                let shape = (
                    sl.bullets.len(),
                    sl.title.split_whitespace().count().min(3),
                    sl.chart.is_some(),
                    sl.image.is_some(),
                );
                if let Some((prev_index, prev_bullets, prev_chart, prev_image)) = previous {
                    if prev_bullets == shape.0
                        && prev_chart == shape.2
                        && prev_image == shape.3
                        && shape.0 >= 3
                    {
                        push(
                            "repeated_structure",
                            "low",
                            format!(
                                "same structure as slide {prev_index} ({} bullets, chart={}, image={})",
                                shape.0, shape.2, shape.3
                            ),
                            None,
                        );
                    }
                }
                previous = Some((index, shape.0, shape.2, shape.3));
            } else {
                previous = None;
            }
            if sl.title.trim().is_empty() && sl.bullets.is_empty() {
                push("empty_slide", "medium", "no title and no text".into(), None);
            }
            if kind != "cover" && sl.title.trim().is_empty() {
                push("missing_title", "medium", "slide has no title".into(), None);
            }
            findings.extend(slide_findings.iter().cloned());
            slides.push(json!({
                "index": index,
                "kind": kind,
                "title": sl.title,
                "bullet_count": sl.bullets.len(),
                "has_notes": !notes.is_empty(),
                "has_chart": sl.chart.is_some(),
                "has_image": sl.image.is_some(),
                "findings": slide_findings,
            }));
        }
        let truncated = findings.len() > max_findings;
        findings.truncate(max_findings);
        // Text-level fixes are only ever for template remnants; the
        // candidate list carries the exact slide text so a model has
        // nothing to invent but the replacement.
        let fix_candidates: Vec<Value> = findings
            .iter()
            .filter(|f| f["code"] == "placeholder_text")
            .enumerate()
            .map(|(i, f)| {
                json!({
                    "candidate": i + 1,
                    "slide": f["slide"],
                    "original": f["evidence"],
                    "problem": f["detail"],
                })
            })
            .collect();
        let needs_sources: Vec<Value> = findings
            .iter()
            .filter(|f| f["code"] == "unsourced_number")
            .map(|f| f["slide"].clone())
            .collect();
        Ok(json!({
            "artifact_id": id,
            "content_hash": harbor_canonical::sha256_hex(&a.bytes),
            "fix_candidates": fix_candidates,
            "needs_sources": needs_sources,
            "slide_count": total,
            "slides": slides,
            "findings": findings,
            "finding_count": findings.len(),
            "counts_by_code": counts,
            "truncated": truncated,
            "text": all_text.join("\n"),
            "not_checked": [
                {"check": "page_numbers", "reason": "slide footers are not in the presentation IR"},
                {"check": "text_sizes_and_hierarchy", "reason": "run sizes are not in the presentation IR"},
                {"check": "contrast", "reason": "colours are not in the presentation IR"},
                {"check": "alignment", "reason": "paragraph alignment is not in the presentation IR"},
                {"check": "layout_reuse", "reason": "master layout ids are not in the IR; repeated_structure compares text structure only"}
            ]
        }))
    }
}

// ---------------------------------------------------------------------------
// workbook.conventions

pub struct WorkbookConventions {
    spec: ToolSpec,
    ref_re: regex::Regex,
    external_re: regex::Regex,
}

impl WorkbookConventions {
    pub fn new() -> Self {
        WorkbookConventions {
            spec: ToolSpec {
                id: "workbook.conventions".into(),
                description: "Formula-first audit of an attached workbook: numeric constants typed inside formula rows or columns (with the neighbouring formula shifted to the cell and verified through the pinned engine) and links to other files. Cell colours and number formats are reported as not checked.".into(),
                args_schema: artifact_args_schema(),
                risk: RiskClass::Read,
                requires: vec!["artifact.engine".into(), "formula.qualified".into()],
                timeout_ms: 120_000,
                max_output_bytes: 8 * MB,
            },
            ref_re: regex::Regex::new(r"(\$?)([A-Z]{1,3})(\$?)(\d{1,7})").expect("ref regex"),
            external_re: regex::Regex::new(r"\[([^\]]+)\]").expect("external regex"),
        }
    }

    /// Shift the relative references of `formula` by `d_col`/`d_row`
    /// (absolute parts keep their coordinate). Returns None when a shift
    /// would leave the sheet.
    fn shift_formula(&self, formula: &str, d_col: i64, d_row: i64) -> Option<String> {
        let mut out = String::new();
        let mut last = 0;
        for cap in self.ref_re.captures_iter(formula) {
            let m = cap.get(0).unwrap();
            // Skip matches inside quoted sheet names or function names
            // followed by '(' (e.g. LOG10( ) — a function, not a ref.
            let after = formula[m.end()..].chars().next();
            if after == Some('(') {
                continue;
            }
            out.push_str(&formula[last..m.start()]);
            let abs_col = &cap[1] == "$";
            let abs_row = &cap[3] == "$";
            let col = harbor_artifacts::workbook::col_number(&cap[2]) as i64;
            let row: i64 = cap[4].parse().ok()?;
            let new_col = if abs_col { col } else { col + d_col };
            let new_row = if abs_row { row } else { row + d_row };
            if new_col < 1 || new_row < 1 {
                return None;
            }
            out.push_str(&format!(
                "{}{}{}{}",
                if abs_col { "$" } else { "" },
                harbor_artifacts::workbook::col_letter(new_col as u32),
                if abs_row { "$" } else { "" },
                new_row
            ));
            last = m.end();
        }
        out.push_str(&formula[last..]);
        Some(out)
    }
}

impl Default for WorkbookConventions {
    fn default() -> Self {
        Self::new()
    }
}

fn number_of(v: Option<&CellValue>) -> Option<f64> {
    match v {
        Some(CellValue::Number(n)) => Some(*n),
        Some(CellValue::Text(t)) => t.trim().replace(',', "").parse::<f64>().ok(),
        _ => None,
    }
}

impl Tool for WorkbookConventions {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let (id, a) = artifact(ctx, &tool, args)?;
        if detect_kind(&a.bytes) != ArtifactKind::Xlsx {
            return Err(ToolError::failed(
                &tool,
                format!("artifact {id} is not an XLSX workbook"),
            ));
        }
        let max_findings = args
            .get("max_findings")
            .and_then(Value::as_u64)
            .unwrap_or(500) as usize;
        let wb =
            WorkbookDoc::load(&a.bytes).map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        // A private engine copy to verify candidate formulas.
        let mut calc = harbor_formula::engine::HarborWorkbook::new();
        for (name, data) in wb.sheets_snapshot() {
            calc.add_sheet(name);
            for ((c, r), cell) in &data.cells {
                if let Some(cv) = &cell.cached {
                    if cell.formula.is_none() {
                        calc.set_value(name, *r, *c, cv.clone());
                    }
                }
            }
        }
        for (name, data) in wb.sheets_snapshot() {
            for ((c, r), cell) in &data.cells {
                if let Some(f) = &cell.formula {
                    calc.set_formula(name, *r, *c, f);
                }
            }
        }
        let mut findings = Vec::new();
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut sheets = Vec::new();
        for (name, data) in wb.sheets_snapshot() {
            let mut formula_cells = 0usize;
            let mut constant_cells = 0usize;
            // Group by row and by column.
            let mut rows: BTreeMap<u32, Vec<(u32, &harbor_artifacts::workbook::SheetCell)>> =
                BTreeMap::new();
            let mut cols: BTreeMap<u32, Vec<(u32, &harbor_artifacts::workbook::SheetCell)>> =
                BTreeMap::new();
            for ((c, r), cell) in &data.cells {
                if cell.formula.is_some() {
                    formula_cells += 1;
                } else if number_of(cell.cached.as_ref()).is_some() {
                    constant_cells += 1;
                }
                rows.entry(*r).or_default().push((*c, cell));
                cols.entry(*c).or_default().push((*r, cell));
            }
            let mut seen: BTreeSet<(u32, u32)> = BTreeSet::new();
            // External links.
            for ((c, r), cell) in &data.cells {
                if let Some(f) = &cell.formula {
                    if let Some(m) = self.external_re.captures(f) {
                        let address =
                            format!("{}{}", harbor_artifacts::workbook::col_letter(*c), r);
                        *counts.entry("external_link".into()).or_insert(0) += 1;
                        findings.push(json!({
                            "sheet": name,
                            "address": address,
                            "target_id": cell_target_id(name, &address),
                            "content_hash": cell_content_hash(cell.formula.as_deref(), cell.cached.as_ref()),
                            "code": "external_link",
                            "severity": "medium",
                            "formula": f,
                            "detail": format!("depends on another file: {}", &m[1]),
                            "linked_file": &m[1],
                        }));
                    }
                }
            }
            // Constants inside formula rows / columns.
            let mut check_line =
                |line_kind: &str,
                 key: u32,
                 cells: &Vec<(u32, &harbor_artifacts::workbook::SheetCell)>,
                 findings: &mut Vec<Value>,
                 counts: &mut BTreeMap<String, u64>| {
                    let formulas: Vec<&(u32, &harbor_artifacts::workbook::SheetCell)> =
                        cells.iter().filter(|(_, c)| c.formula.is_some()).collect();
                    if formulas.len() < 2 {
                        return;
                    }
                    let min_f = formulas.iter().map(|(k, _)| *k).min().unwrap();
                    let max_f = formulas.iter().map(|(k, _)| *k).max().unwrap();
                    for (k, cell) in cells {
                        if cell.formula.is_some() || *k < min_f || *k > max_f {
                            continue;
                        }
                        let Some(current) = number_of(cell.cached.as_ref()) else {
                            continue;
                        };
                        let (col, row) = if line_kind == "row" {
                            (*k, key)
                        } else {
                            (key, *k)
                        };
                        if !seen.insert((col, row)) {
                            continue;
                        }
                        // Nearest formula neighbour on the line.
                        let neighbour = formulas
                            .iter()
                            .min_by_key(|(nk, _)| (*nk as i64 - *k as i64).abs())
                            .unwrap();
                        let (d_col, d_row) = if line_kind == "row" {
                            (*k as i64 - neighbour.0 as i64, 0)
                        } else {
                            (0, *k as i64 - neighbour.0 as i64)
                        };
                        let candidate = self.shift_formula(
                            neighbour.1.formula.as_deref().unwrap(),
                            d_col,
                            d_row,
                        );
                        let address =
                            format!("{}{}", harbor_artifacts::workbook::col_letter(col), row);
                        let neighbour_address = if line_kind == "row" {
                            format!(
                                "{}{}",
                                harbor_artifacts::workbook::col_letter(neighbour.0),
                                key
                            )
                        } else {
                            format!(
                                "{}{}",
                                harbor_artifacts::workbook::col_letter(key),
                                neighbour.0
                            )
                        };
                        // Verify the candidate through the engine on a private copy.
                        let (reproduces, computed) = match &candidate {
                            Some(f) => {
                                calc.set_formula(name, row, col, f);
                                let v = calc.evaluate_cell(name, row, col);
                                calc.set_value(
                                    name,
                                    row,
                                    col,
                                    cell.cached.clone().unwrap_or(CellValue::Blank),
                                );
                                let computed = number_of(Some(&v));
                                let ok = computed
                                    .map(|x| {
                                        (x - current).abs() <= 1e-9_f64.max(current.abs() * 1e-9)
                                    })
                                    .unwrap_or(false);
                                (ok, computed)
                            }
                            None => (false, None),
                        };
                        let code = if reproduces {
                            "hardcoded_in_formula_line"
                        } else {
                            "constant_in_formula_line"
                        };
                        *counts.entry(code.into()).or_insert(0) += 1;
                        findings.push(json!({
                        "sheet": name,
                        "address": address,
                        "target_id": cell_target_id(name, &address),
                        "content_hash": cell_content_hash(cell.formula.as_deref(), cell.cached.as_ref()),
                        "code": code,
                        "severity": if reproduces { "high" } else { "low" },
                        "line": line_kind,
                        "current_value": current,
                        "neighbour": neighbour_address,
                        "neighbour_formula": neighbour.1.formula,
                        "suggested_formula": candidate.as_ref().map(|f| format!("={}", f.trim_start_matches('='))),
                        "reproduces_value": reproduces,
                        "computed_value": computed,
                        "detail": if reproduces {
                            format!("{address} is typed as {current} inside a {line_kind} of formulas; the shifted neighbour formula {} reproduces it", candidate.clone().unwrap_or_default())
                        } else {
                            format!("{address} is a constant inside a {line_kind} of formulas; no verified formula reproduces {current} (likely an input — confirm and colour as such)")
                        },
                    }));
                    }
                };
            for (r, cells) in &rows {
                check_line("row", *r, cells, &mut findings, &mut counts);
            }
            for (c, cells) in &cols {
                check_line("column", *c, cells, &mut findings, &mut counts);
            }
            sheets.push(json!({
                "name": name,
                "cells": data.cells.len(),
                "formula_cells": formula_cells,
                "constant_cells": constant_cells,
            }));
        }
        let truncated = findings.len() > max_findings;
        findings.truncate(max_findings);
        // The fix/review split is mechanical: only an engine-verified
        // formula may be applied; everything else is for the user.
        let decisions: Vec<Value> = findings
            .iter()
            .map(|f| {
                let code = f["code"].as_str().unwrap_or_default();
                let (action, reason) = match code {
                    "hardcoded_in_formula_line" => (
                        "fix",
                        format!(
                            "{} reproduces the typed value {} through the engine",
                            f["suggested_formula"].as_str().unwrap_or_default(),
                            f["current_value"]
                        ),
                    ),
                    "external_link" => (
                        "review",
                        format!(
                            "fragile dependency on {}; confirm the file is available or replace the link with a value",
                            f["linked_file"].as_str().unwrap_or_default()
                        ),
                    ),
                    _ => (
                        "review",
                        "no formula reproduces this constant; confirm it is an input and colour it as one".to_string(),
                    ),
                };
                json!({"sheet": f["sheet"], "address": f["address"], "action": action, "reason": reason})
            })
            .collect();
        let fixes = decisions.iter().filter(|d| d["action"] == "fix").count();
        let summary = format!(
            "{} cell(s) typed inside formula lines have an engine-verified formula and are proposed as fixes; {} finding(s) are left for review ({} constant(s) without a reproducing formula, {} link(s) to other files). Colour roles and number formats were not checked: cell styles are not in the workbook IR.",
            fixes,
            decisions.len() - fixes,
            counts.get("constant_in_formula_line").copied().unwrap_or(0),
            counts.get("external_link").copied().unwrap_or(0)
        );
        Ok(json!({
            "artifact_id": id,
            "content_hash": harbor_canonical::sha256_hex(&a.bytes),
            "decisions": decisions,
            "summary": summary,
            "engine": {
                "family": harbor_formula::engine::engine_identity().family,
                "version": harbor_formula::engine::engine_identity().version,
            },
            "sheets": sheets,
            "findings": findings,
            "finding_count": findings.len(),
            "counts_by_code": counts,
            "truncated": truncated,
            "not_checked": [
                {"check": "color_roles", "reason": "font colours and fills are not in the workbook IR"},
                {"check": "number_formats", "reason": "cell number formats are not in the workbook IR"},
            ]
        }))
    }
}

// ---------------------------------------------------------------------------
// docx.inspect

pub struct DocxInspect {
    spec: ToolSpec,
}

impl DocxInspect {
    pub fn new() -> Self {
        DocxInspect {
            spec: ToolSpec {
                id: "docx.inspect".into(),
                description: "Deterministic style review of an attached DOCX: heading hierarchy (skipped levels, empty headings), direct-formatting overrides per paragraph (style contamination), mixed-direction paragraphs, page margins, and heading size ratios from styles.xml when present.".into(),
                args_schema: artifact_args_schema(),
                risk: RiskClass::Read,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 30_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for DocxInspect {
    fn default() -> Self {
        Self::new()
    }
}

fn heading_level(style: Option<&str>) -> Option<u32> {
    let s = style?.to_ascii_lowercase().replace(' ', "");
    let rest = s.strip_prefix("heading")?;
    rest.parse::<u32>().ok().filter(|l| (1..=9).contains(l))
}

fn is_arabic(ch: char) -> bool {
    let cp = ch as u32;
    (0x0600..=0x06FF).contains(&cp)
        || (0x0750..=0x077F).contains(&cp)
        || (0xFB50..=0xFDFF).contains(&cp)
        || (0xFE70..=0xFEFF).contains(&cp)
}

fn read_part(bytes: &[u8], name: &str) -> Option<String> {
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut f = archive.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

impl Tool for DocxInspect {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let (id, a) = artifact(ctx, &tool, args)?;
        if detect_kind(&a.bytes) != ArtifactKind::Docx {
            return Err(ToolError::failed(
                &tool,
                format!("artifact {id} is not a DOCX document"),
            ));
        }
        let max_findings = args
            .get("max_findings")
            .and_then(Value::as_u64)
            .unwrap_or(500) as usize;
        let doc =
            DocxDocument::load(&a.bytes).map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let xml = read_part(&a.bytes, "word/document.xml")
            .ok_or_else(|| ToolError::failed(&tool, "missing word/document.xml"))?;
        let tree = roxmltree::Document::parse(&xml)
            .map_err(|e| ToolError::failed(&tool, format!("document.xml: {e}")))?;
        let mut findings = Vec::new();
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut push = |code: &str, severity: &str, location: String, detail: String| {
            *counts.entry(code.to_string()).or_insert(0) += 1;
            findings.push(json!({
                "code": code,
                "severity": severity,
                "location": location,
                "detail": detail,
            }));
        };

        // Paragraph-level scan in document order (matches DocxDocument
        // indices: every <w:p> in the body).
        let body = tree
            .descendants()
            .find(|n| n.has_tag_name("body"))
            .ok_or_else(|| ToolError::failed(&tool, "document.xml has no body"))?;
        let paragraphs: Vec<roxmltree::Node> =
            body.descendants().filter(|n| n.has_tag_name("p")).collect();
        let mut structure = Vec::new();
        let mut last_level: Option<u32> = None;
        let mut contaminated = 0usize;
        let mut mixed = 0usize;
        for (i, p) in paragraphs.iter().enumerate() {
            let index = (i + 1) as u32;
            let para = doc.paragraphs.get(i);
            let text = para.map(|p| p.text.clone()).unwrap_or_default();
            let style = para.and_then(|p| p.style.clone());
            let level = heading_level(style.as_deref());
            let ppr = p.children().find(|n| n.has_tag_name("pPr"));
            let mut para_overrides: BTreeSet<String> = BTreeSet::new();
            if let Some(ppr) = ppr {
                for child in ppr.children() {
                    match child.tag_name().name() {
                        "spacing" => {
                            para_overrides.insert("spacing".into());
                        }
                        "ind" => {
                            para_overrides.insert("indent".into());
                        }
                        "jc" => {
                            para_overrides.insert("alignment".into());
                        }
                        "rPr" => {}
                        _ => {}
                    }
                }
            }
            let bidi = ppr
                .map(|n| n.children().any(|c| c.has_tag_name("bidi")))
                .unwrap_or(false);
            let mut run_overrides: BTreeSet<String> = BTreeSet::new();
            for r in p.children().filter(|n| n.has_tag_name("r")) {
                if let Some(rpr) = r.children().find(|n| n.has_tag_name("rPr")) {
                    for c in rpr.children() {
                        let name = match c.tag_name().name() {
                            "rFonts" => "font",
                            "color" => "color",
                            "sz" | "szCs" => "size",
                            "b" | "bCs" => "bold",
                            "i" | "iCs" => "italic",
                            "u" => "underline",
                            "highlight" | "shd" => "highlight",
                            "rStyle" => "",
                            _ => "",
                        };
                        if !name.is_empty() {
                            run_overrides.insert(name.into());
                        }
                    }
                }
            }
            let location = format!("¶ {index}");
            if !run_overrides.is_empty() || !para_overrides.is_empty() {
                // Emphasis inside a paragraph (bold/italic on one run) is
                // ordinary; font, size, colour, spacing, indent, alignment
                // overrides break the template.
                let structural: Vec<String> = run_overrides
                    .iter()
                    .filter(|o| matches!(o.as_str(), "font" | "size" | "color" | "highlight"))
                    .cloned()
                    .chain(para_overrides.iter().cloned())
                    .collect();
                if !structural.is_empty() {
                    contaminated += 1;
                    push(
                        "direct_formatting",
                        "medium",
                        location.clone(),
                        format!(
                            "direct {} override(s) on top of style {}",
                            structural.join(", "),
                            style.clone().unwrap_or_else(|| "Normal".into())
                        ),
                    );
                }
            }
            // Hierarchy.
            if let Some(l) = level {
                if text.trim().is_empty() {
                    push(
                        "empty_heading",
                        "medium",
                        location.clone(),
                        format!("Heading {l} has no text"),
                    );
                }
                if let Some(prev) = last_level {
                    if l > prev + 1 {
                        push(
                            "heading_skip",
                            "medium",
                            location.clone(),
                            format!(
                                "Heading {l} follows Heading {prev}; level {} was skipped",
                                prev + 1
                            ),
                        );
                    }
                } else if l > 1
                    && !doc.paragraphs.iter().any(|p| {
                        p.style
                            .as_deref()
                            .map(|s| s.eq_ignore_ascii_case("Title"))
                            .unwrap_or(false)
                    })
                {
                    push(
                        "heading_skip",
                        "low",
                        location.clone(),
                        format!("first heading is Heading {l}; the document starts below level 1"),
                    );
                }
                last_level = Some(l);
            }
            // Direction.
            let arabic = text.chars().filter(|c| is_arabic(*c)).count();
            let latin = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
            let digits = text.chars().filter(|c| c.is_ascii_digit()).count();
            if arabic > 0 && (latin > 0 || digits > 0) {
                mixed += 1;
                push(
                    "mixed_direction",
                    "low",
                    location.clone(),
                    format!(
                        "Arabic text with {} Latin letter(s) and {} digit(s); identifiers and numbers need LTR isolation ({})",
                        latin,
                        digits,
                        if bidi { "paragraph is RTL" } else { "paragraph is not marked RTL" }
                    ),
                );
            } else if arabic > 0 && !bidi {
                push(
                    "rtl_not_marked",
                    "medium",
                    location.clone(),
                    "Arabic paragraph is not marked right-to-left (w:bidi)".into(),
                );
            }
            structure.push(json!({
                "index": index,
                "style": style,
                "heading_level": level,
                "chars": text.chars().count(),
                "run_overrides": run_overrides,
                "paragraph_overrides": para_overrides,
                "rtl": bidi,
            }));
        }
        // Heading level counts and consecutive same-level headings with no body.
        let mut heading_counts: BTreeMap<String, u64> = BTreeMap::new();
        for p in &doc.paragraphs {
            if let Some(l) = heading_level(p.style.as_deref()) {
                *heading_counts.entry(format!("h{l}")).or_insert(0) += 1;
            }
        }
        let mut prev_heading: Option<(u32, u32)> = None;
        for p in &doc.paragraphs {
            match heading_level(p.style.as_deref()) {
                Some(l) => {
                    if let Some((pi, pl)) = prev_heading {
                        if pl == l {
                            push(
                                "heading_without_body",
                                "low",
                                format!("¶ {pi}"),
                                format!("Heading {pl} is followed directly by another Heading {l}"),
                            );
                        }
                    }
                    prev_heading = Some((p.index, l));
                }
                None => {
                    if !p.text.trim().is_empty() {
                        prev_heading = None;
                    }
                }
            }
        }
        // Margins (sectPr/pgMar, twips): cramped below 0.5 inch.
        let mut margins = json!(null);
        if let Some(pg) = tree.descendants().find(|n| n.has_tag_name("pgMar")) {
            let get = |k: &str| {
                pg.attributes()
                    .find(|a| a.name() == k)
                    .and_then(|a| a.value().parse::<i64>().ok())
            };
            let m = [
                ("top", get("top")),
                ("bottom", get("bottom")),
                ("left", get("left")),
                ("right", get("right")),
            ];
            let cramped: Vec<String> = m
                .iter()
                .filter_map(|(k, v)| {
                    v.filter(|t| *t < 720)
                        .map(|t| format!("{k} {:.2}in", t as f64 / 1440.0))
                })
                .collect();
            if !cramped.is_empty() {
                push(
                    "cramped_margins",
                    "high",
                    "page".into(),
                    format!("margins under 0.5 inch: {}", cramped.join(", ")),
                );
            }
            margins = json!({"top": m[0].1, "bottom": m[1].1, "left": m[2].1, "right": m[3].1, "unit": "twips"});
        }
        // Heading size ratios from styles.xml when present.
        let mut heading_sizes = json!(null);
        let mut checked_sizes = false;
        if let Some(styles_xml) = read_part(&a.bytes, "word/styles.xml") {
            if let Ok(st) = roxmltree::Document::parse(&styles_xml) {
                let mut sizes: BTreeMap<u32, f64> = BTreeMap::new();
                for style in st.descendants().filter(|n| n.has_tag_name("style")) {
                    let sid = style
                        .attributes()
                        .find(|a| a.name() == "styleId")
                        .map(|a| a.value().to_string());
                    let Some(level) = heading_level(sid.as_deref()) else {
                        continue;
                    };
                    let sz = style
                        .descendants()
                        .find(|n| n.has_tag_name("sz"))
                        .and_then(|n| {
                            n.attributes()
                                .find(|a| a.name() == "val")
                                .and_then(|a| a.value().parse::<f64>().ok())
                        });
                    if let Some(half_points) = sz {
                        sizes.insert(level, half_points / 2.0);
                    }
                }
                if sizes.len() >= 2 {
                    checked_sizes = true;
                    let levels: Vec<u32> = sizes.keys().cloned().collect();
                    for w in levels.windows(2) {
                        let (upper, lower) = (sizes[&w[0]], sizes[&w[1]]);
                        if lower > 0.0 {
                            let ratio = upper / lower;
                            if !(1.1..=1.6).contains(&ratio) {
                                push(
                                    "heading_scale",
                                    "low",
                                    format!("styles Heading {} / Heading {}", w[0], w[1]),
                                    format!("size ratio {ratio:.2} ({upper}pt / {lower}pt); aim near 1.25"),
                                );
                            }
                        }
                    }
                }
                heading_sizes = json!(sizes
                    .iter()
                    .map(|(k, v)| (format!("h{k}"), *v))
                    .collect::<BTreeMap<_, _>>());
            }
        }
        let truncated = findings.len() > max_findings;
        findings.truncate(max_findings);
        let mut not_checked = vec![
            json!({"check": "line_spacing", "reason": "paragraph spacing values are reported as overrides, not judged against a baseline"}),
            json!({"check": "contrast_colours", "reason": "colour values are not judged, only their presence as overrides"}),
        ];
        if !checked_sizes {
            not_checked.push(
                json!({"check": "heading_scale", "reason": "no heading sizes in styles.xml"}),
            );
        }
        if margins.is_null() {
            not_checked.push(json!({"check": "margins", "reason": "no pgMar in document.xml"}));
        }
        let report_text = findings
            .iter()
            .map(|f| {
                format!(
                    "{}: {} — {}",
                    f["location"].as_str().unwrap_or_default(),
                    f["code"].as_str().unwrap_or_default(),
                    f["detail"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(json!({
            "artifact_id": id,
            "content_hash": harbor_canonical::sha256_hex(&a.bytes),
            "report_text": report_text,
            "paragraph_count": doc.paragraphs.len(),
            "heading_counts": heading_counts,
            "contaminated_paragraphs": contaminated,
            "mixed_direction_paragraphs": mixed,
            "margins": margins,
            "heading_sizes": heading_sizes,
            "structure": structure,
            "findings": findings,
            "finding_count": findings.len(),
            "counts_by_code": counts,
            "truncated": truncated,
            "not_checked": not_checked,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_found_and_keyed_by_digits() {
        let n = numbers_in("Revenue grows 12% to $540k; hire 2 in May 2026, 3.5x");
        assert!(n.iter().any(|x| x.trim() == "12%"));
        assert!(n.iter().any(|x| x.trim() == "$540k"));
        assert!(n.iter().any(|x| x.trim() == "2026"));
        assert_eq!(number_key("$540k"), "540");
        assert_eq!(number_key("1,250.00"), "125000");
    }

    #[test]
    fn shift_formula_moves_relative_refs_only() {
        let t = WorkbookConventions::new();
        assert_eq!(t.shift_formula("B2*$B$1", 1, 0).unwrap(), "C2*$B$1");
        assert_eq!(t.shift_formula("SUM(B2:B5)", 2, 0).unwrap(), "SUM(D2:D5)");
        assert_eq!(t.shift_formula("A1+A2", 0, 3).unwrap(), "A4+A5");
        assert_eq!(t.shift_formula("LOG10(B2)", 1, 0).unwrap(), "LOG10(C2)");
        assert!(t.shift_formula("A1", -1, 0).is_none());
    }

    #[test]
    fn heading_levels_parse_common_style_ids() {
        assert_eq!(heading_level(Some("Heading1")), Some(1));
        assert_eq!(heading_level(Some("heading 2")), Some(2));
        assert_eq!(heading_level(Some("Title")), None);
        assert_eq!(heading_level(None), None);
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use crate::tools::{MemoryArtifacts, ToolRegistry};
    use std::sync::atomic::AtomicBool;

    fn fixture(name: &str) -> Vec<u8> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::read(root.join("fixtures/office").join(name)).unwrap()
    }

    fn call(tool: &str, name: &str, extra: Value) -> Value {
        let artifacts = MemoryArtifacts::new().with("a", name, fixture(name));
        let cancel = AtomicBool::new(false);
        let host = json!({});
        let ctx = ToolContext {
            artifacts: &artifacts,
            knowledge: None,
            provider: None,
            model: None,
            workspace_root: None,
            host_inputs: &host,
            cancel: &cancel,
            trace_key: None,
            deadline: None,
        };
        let registry = ToolRegistry::builtin();
        let mut args = json!({"artifact_id": "a"});
        if let Some(o) = extra.as_object() {
            for (k, v) in o {
                args[k] = v.clone();
            }
        }
        let allow: BTreeSet<String> = [tool.to_string()].into_iter().collect();
        registry.call(&ctx, tool, &args, &allow).unwrap().output
    }

    #[test]
    fn deck_inspect_finds_placeholders_unsourced_numbers_and_repeats() {
        let out = call("deck.inspect", "board_deck.pptx", json!({}));
        assert_eq!(out["slide_count"], 6);
        let kinds: Vec<&str> = out["slides"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds[0], "cover");
        assert_eq!(kinds[1], "agenda");
        assert_eq!(kinds[5], "summary");
        let codes = &out["counts_by_code"];
        assert_eq!(codes["placeholder_text"], 2, "{codes}");
        assert!(codes["unsourced_number"].as_u64().unwrap() >= 1, "{codes}");
        let unsourced: Vec<u64> = out["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["code"] == "unsourced_number")
            .map(|f| f["slide"].as_u64().unwrap())
            .collect();
        // Slide 3 has numbers and notes without a source; slide 4 cites
        // its CRM export; slide 5 has a percentage and no notes at all.
        assert!(unsourced.contains(&3), "{unsourced:?}");
        assert!(!unsourced.contains(&4), "{unsourced:?}");
        assert!(unsourced.contains(&5), "{unsourced:?}");
        assert!(out["text"].as_str().unwrap().contains("Lorem ipsum"));
        assert!(out["not_checked"].as_array().unwrap().len() >= 4);
    }

    #[test]
    fn workbook_conventions_verifies_shifted_formulas_and_flags_links() {
        let out = call("workbook.conventions", "dcf_model.xlsx", json!({}));
        let findings = out["findings"].as_array().unwrap();
        let d2 = findings
            .iter()
            .find(|f| f["address"] == "D2")
            .expect("D2 finding");
        assert_eq!(d2["code"], "hardcoded_in_formula_line", "{d2}");
        assert_eq!(d2["suggested_formula"], "=C2*1.1");
        assert_eq!(d2["reproduces_value"], true);
        assert_eq!(d2["content_hash"].as_str().unwrap().len(), 64);
        let d4 = findings
            .iter()
            .find(|f| f["address"] == "D4")
            .expect("D4 finding");
        assert_eq!(d4["code"], "constant_in_formula_line", "{d4}");
        assert_eq!(d4["reproduces_value"], false);
        let b5 = findings
            .iter()
            .find(|f| f["address"] == "B5")
            .expect("B5 finding");
        assert_eq!(b5["code"], "external_link");
        assert_eq!(b5["linked_file"], "Assumptions.xlsx");
        // Inputs outside formula spans (B2, B3, the year row) are not findings.
        assert!(!findings
            .iter()
            .any(|f| f["address"] == "B2" || f["address"] == "C1"));
        assert_eq!(out["counts_by_code"]["hardcoded_in_formula_line"], 1);
    }

    #[test]
    fn docx_inspect_reports_hierarchy_contamination_direction_and_margins() {
        let out = call("docx.inspect", "report_styles.docx", json!({}));
        let codes = &out["counts_by_code"];
        assert_eq!(codes["heading_skip"], 1, "{codes}");
        assert_eq!(codes["direct_formatting"], 2, "{codes}"); // font/size/colour + spacing
        assert_eq!(codes["heading_without_body"], 1, "{codes}");
        assert_eq!(codes["rtl_not_marked"], 1, "{codes}");
        assert_eq!(codes["mixed_direction"], 1, "{codes}");
        assert_eq!(codes["cramped_margins"], 1, "{codes}");
        assert_eq!(codes["heading_scale"], 1, "{codes}");
        let findings = out["findings"].as_array().unwrap();
        // Emphasis-only run (bold) is not contamination.
        assert!(!findings.iter().any(|f| f["location"] == "¶ 6"));
        assert!(findings
            .iter()
            .any(|f| f["code"] == "direct_formatting" && f["location"] == "¶ 5"));
        assert_eq!(out["heading_sizes"]["h1"], 16.0);
        assert_eq!(out["contaminated_paragraphs"], 2);
    }

    #[test]
    fn verify_numbers_flags_invented_figures() {
        let artifacts = MemoryArtifacts::new();
        let cancel = AtomicBool::new(false);
        let host = json!({});
        let ctx = ToolContext {
            artifacts: &artifacts,
            knowledge: None,
            provider: None,
            model: None,
            workspace_root: None,
            host_inputs: &host,
            cancel: &cancel,
            trace_key: None,
            deadline: None,
        };
        let registry = ToolRegistry::builtin();
        let allow: BTreeSet<String> = ["text.verify_numbers".to_string()].into_iter().collect();
        let out = registry
            .call(
                &ctx,
                "text.verify_numbers",
                &json!({
                    "source": "Shipped 3 features; churn 3.1%; 12 bugs closed.",
                    "texts": ["Progress: shipped 3 features, churn down to 3.1%.", "Plans: close 20 bugs next week."]
                }),
                &allow,
            )
            .unwrap()
            .output;
        assert_eq!(out["grounded"], false);
        assert_eq!(out["missing"].as_array().unwrap().len(), 1);
        assert_eq!(out["missing"][0]["number"], "20");
        assert_eq!(out["missing"][0]["index"], 1);
    }
}
