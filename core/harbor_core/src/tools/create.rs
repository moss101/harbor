//! Creating new artifacts (decision 0008): a typed spec becomes a creation
//! batch over the empty base, and the batch becomes bytes.
//!
//! 02 §"Artifact batches and safe save": "A newly created artifact first
//! registers an immutable empty base version with the SHA-256 of empty
//! bytes." A creation batch binds that base, every precondition names a
//! target that does not exist yet (so each expects the empty hash too), and
//! the operations are the schema's own kinds — `sheet.insert` + `cell.set`
//! for a workbook, `slide.insert` + `slide.update` for a deck, `block.insert`
//! for a document, `metadata.set` for properties any of them carry.
//!
//! The division of labour is the one decision 0006 found works for a small
//! model: the model picks parameters (headers, rows, which columns to total,
//! slide titles and bullets, section text) and the code here decides
//! everything mechanical — every formula, every layout choice, every byte.
//! Rendering is deterministic (no clock, no random ids, fixed zip
//! timestamps), so the commit path re-derives the approved output hash from
//! the batch alone, exactly as it does for an edit batch. Every rendered
//! package passes [`harbor_artifacts::package_integrity`] before it is
//! proposed: a structural defect is refused here instead of surfacing as an
//! Office "repair" prompt.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use harbor_artifacts::docx::{create_docx, BlockStyle, DocxBlock};
use harbor_artifacts::workbook::CellSet;
use harbor_artifacts::{
    ArtifactBatch, ChartKind, ChartSpec, DeckStyle, OpKind, Operation, PptxDeck, Precondition,
    SlideContent, WorkbookDoc, EMPTY_CONTENT_HASH,
};
use harbor_canonical::JsonValue;
use harbor_formula::value::CellValue;

use super::builtin::{
    addr, batch_json, cell_value_repr, check_formula_allowed, default_batch_id, parse_addr,
    DiffEntry,
};
use super::{RiskClass, Tool, ToolContext, ToolError, ToolSpec};

const MB: usize = 1024 * 1024;

/// Which file a creation batch builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatedKind {
    Xlsx,
    Pptx,
    Docx,
}

impl CreatedKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CreatedKind::Xlsx => "xlsx",
            CreatedKind::Pptx => "pptx",
            CreatedKind::Docx => "docx",
        }
    }
}

// ---------------------------------------------------------------------------
// Batch construction

struct Ops {
    ops: Vec<Operation>,
}

impl Ops {
    fn new() -> Self {
        Ops { ops: Vec::new() }
    }

    fn push(&mut self, kind: OpKind, target_id: String, args: JsonValue) {
        let op_id = format!("op-{}", self.ops.len() + 1);
        self.ops.push(Operation {
            op_id,
            kind,
            precondition: Precondition {
                target_id,
                expected_content_hash: EMPTY_CONTENT_HASH.to_string(),
            },
            args,
        });
    }

    /// The batch over the empty base. The artifact id is derived from the
    /// operations so the same spec always names the same new artifact
    /// (cassettes and evals stay deterministic).
    fn into_batch(self, kind: CreatedKind) -> ArtifactBatch {
        let batch_id = default_batch_id(EMPTY_CONTENT_HASH, &self.ops);
        let suffix = batch_id.rsplit('-').next().unwrap_or("0").to_string();
        ArtifactBatch {
            batch_id,
            artifact_id: format!("new-{}-{suffix}", kind.as_str()),
            base_version_id: "v-empty".into(),
            base_content_hash: EMPTY_CONTENT_HASH.into(),
            operations: self.ops,
        }
    }
}

fn js(s: impl Into<String>) -> JsonValue {
    JsonValue::str(s)
}

fn ji(n: i64) -> JsonValue {
    JsonValue::Int(n)
}

/// Canonical JSON has no floats (02 contract): numbers travel as decimal
/// strings. Integral values are written without a fraction.
fn decimal_string(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n}");
        if s.contains('e') || s.contains('E') {
            format!("{n:.10}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        } else {
            s
        }
    }
}

/// A name that is safe as a file name on every platform Harbor ships on.
pub fn file_name_for(title: &str, extension: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in title.chars() {
        let keep = ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.' | ',' | '(' | ')' | '&');
        if keep {
            out.push(ch);
            last_space = false;
        } else if !last_space && !out.is_empty() {
            out.push(' ');
            last_space = true;
        }
    }
    let mut name: String = out.trim().trim_matches('.').chars().take(80).collect();
    name = name.trim().to_string();
    if name.is_empty() {
        name = fallback.to_string();
    }
    format!("{name}.{extension}")
}

// ---------------------------------------------------------------------------
// Workbook spec

/// Column types a created table understands. The type decides the number
/// format and whether the column can be totalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Text,
    Number,
    Integer,
    Currency,
    Percent,
    Date,
}

impl ColumnType {
    fn parse(s: &str) -> Option<ColumnType> {
        Some(match s {
            "text" => ColumnType::Text,
            "number" => ColumnType::Number,
            "integer" => ColumnType::Integer,
            "currency" => ColumnType::Currency,
            "percent" => ColumnType::Percent,
            "date" => ColumnType::Date,
            _ => return None,
        })
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            ColumnType::Number | ColumnType::Integer | ColumnType::Currency | ColumnType::Percent
        )
    }

    fn as_str(&self) -> &'static str {
        match self {
            ColumnType::Text => "text",
            ColumnType::Number => "number",
            ColumnType::Integer => "integer",
            ColumnType::Currency => "currency",
            ColumnType::Percent => "percent",
            ColumnType::Date => "date",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl ComputeOp {
    fn parse(s: &str) -> Option<ComputeOp> {
        Some(match s {
            "add" => ComputeOp::Add,
            "subtract" => ComputeOp::Subtract,
            "multiply" => ComputeOp::Multiply,
            "divide" => ComputeOp::Divide,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Computed {
    pub header: String,
    pub op: ComputeOp,
    pub left: usize,
    pub right: usize,
    pub kind: ColumnType,
}

#[derive(Debug, Clone)]
pub struct WorkbookSpec {
    pub title: String,
    pub sheet: String,
    pub columns: Vec<(String, ColumnType)>,
    /// One entry per data row, one cell per declared column.
    pub rows: Vec<Vec<CellValue>>,
    pub computed: Vec<Computed>,
    /// Indexes into the full column list (declared then computed).
    pub totals: Vec<usize>,
    pub currency_symbol: Option<String>,
    /// Structural slips repaired before parsing ([`normalize_table_spec`]),
    /// in words; shown with the proposal so nothing is dropped silently.
    pub notes: Vec<String>,
}

pub const MAX_COLUMNS: usize = 20;
pub const MAX_ROWS: usize = 500;
pub const MAX_COMPUTED: usize = 5;

/// A sheet name Excel accepts: 1–31 characters, none of `[]:*?/\`.
pub fn sheet_name_problem(name: &str) -> Option<String> {
    let n = name.chars().count();
    if name.trim().is_empty() {
        return Some("the sheet needs a name".into());
    }
    if n > 31 {
        return Some(format!(
            "sheet name {name:?} is {n} characters; Excel allows 31"
        ));
    }
    if let Some(c) = name
        .chars()
        .find(|c| matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
    {
        return Some(format!(
            "sheet name {name:?} contains {c:?}, which Excel does not allow"
        ));
    }
    if name.starts_with('\'') || name.ends_with('\'') {
        return Some(format!(
            "sheet name {name:?} may not start or end with an apostrophe"
        ));
    }
    None
}

/// Parse a cell as a number the way a reader of a description would:
/// `1,200`, `$1,200.50`, `(300)`, `12%`, `3.5k`. Returns the value and
/// whether it was written as a percentage.
pub fn parse_number_text(raw: &str) -> Option<(f64, bool)> {
    let mut t = raw.trim().to_string();
    if t.is_empty() {
        return None;
    }
    let mut negative = false;
    if t.starts_with('(') && t.ends_with(')') {
        negative = true;
        t = t[1..t.len() - 1].trim().to_string();
    }
    if let Some(rest) = t.strip_prefix('-') {
        negative = !negative;
        t = rest.trim().to_string();
    }
    for sym in ["$", "€", "£", "¥"] {
        if let Some(rest) = t.strip_prefix(sym) {
            t = rest.trim().to_string();
        }
    }
    let upper = t.to_ascii_uppercase();
    for code in ["USD", "EUR", "GBP", "SAR", "AED", "QAR", "KWD", "EGP"] {
        if let Some(rest) = upper.strip_prefix(code) {
            t = t[t.len() - rest.len()..].trim().to_string();
        } else if let Some(rest) = upper.strip_suffix(code) {
            t = t[..rest.len()].trim().to_string();
        }
    }
    let mut percent = false;
    let mut scale = 1.0;
    if let Some(rest) = t.strip_suffix('%') {
        percent = true;
        t = rest.trim().to_string();
    } else if let Some(rest) = t.strip_suffix(['k', 'K']) {
        scale = 1_000.0;
        t = rest.trim().to_string();
    } else if let Some(rest) = t.strip_suffix("bn") {
        scale = 1_000_000_000.0;
        t = rest.trim().to_string();
    } else if let Some(rest) = t.strip_suffix(['m', 'M']) {
        scale = 1_000_000.0;
        t = rest.trim().to_string();
    }
    // Thousands separators only between digit groups.
    if t.contains(',') {
        let parts: Vec<&str> = t.split('.').collect();
        let int_part = parts[0];
        let groups: Vec<&str> = int_part.split(',').collect();
        let well_grouped = groups.len() > 1
            && !groups[0].is_empty()
            && groups[0].len() <= 3
            && groups[1..].iter().all(|g| g.len() == 3);
        if !well_grouped {
            return None;
        }
        t = t.replace(',', "");
    }
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if t.matches('.').count() > 1 {
        return None;
    }
    let v: f64 = t.parse().ok()?;
    let v = v * scale;
    Some((if negative { -v } else { v }, percent))
}

/// A stand-in the model writes for "no value" — `"null"`, `"none"`,
/// `"N/A"` — as text. Printed, it reads as content ("null" on a cover
/// slide, found on the iOS simulator run); every optional text a build
/// tool reads treats it as absent.
pub(crate) fn is_null_word(t: &str) -> bool {
    matches!(
        t.trim().to_ascii_lowercase().as_str(),
        "null" | "none" | "n/a" | "undefined" | "nil"
    )
}

/// An optional text field: trimmed, and absent when empty or a null word.
fn optional_text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty() && !is_null_word(t))
        .map(str::to_string)
}

fn cell_from_json(v: &Value, kind: ColumnType) -> Result<CellValue, String> {
    match v {
        Value::Null => Ok(CellValue::Blank),
        Value::Bool(b) => Ok(if kind.is_numeric() {
            return Err(format!("{b} is not a number"));
        } else {
            CellValue::Text(if *b { "Yes".into() } else { "No".into() })
        }),
        Value::Number(n) => {
            let x = n.as_f64().ok_or("not a finite number")?;
            if !x.is_finite() {
                return Err("not a finite number".into());
            }
            if kind.is_numeric() {
                Ok(CellValue::Number(
                    if kind == ColumnType::Percent && x.abs() > 1.0 {
                        x / 100.0
                    } else {
                        x
                    },
                ))
            } else {
                Ok(CellValue::Text(decimal_string(x)))
            }
        }
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() || is_null_word(t) {
                return Ok(CellValue::Blank);
            }
            if kind.is_numeric() {
                let (x, pct) =
                    parse_number_text(t).ok_or_else(|| format!("{t:?} is not a number"))?;
                Ok(CellValue::Number(
                    if kind == ColumnType::Percent && (pct || x.abs() > 1.0) {
                        x / 100.0
                    } else {
                        x
                    },
                ))
            } else {
                Ok(CellValue::Text(t.to_string()))
            }
        }
        other => Err(format!("{other} is not a cell value")),
    }
}

impl WorkbookSpec {
    /// Parse the model-facing spec shape. Errors name the offending part
    /// so a verifier can hand them back to the model verbatim.
    pub fn from_value(v: &Value) -> Result<WorkbookSpec, Vec<String>> {
        let (normalized, notes) = normalize_table_spec(v);
        let v = &normalized;
        let mut problems = Vec::new();
        let text = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
        };
        let title = text("title").to_string();
        if title.is_empty() {
            problems.push("the workbook needs a title".into());
        }
        let sheet = if text("sheet").is_empty() {
            "Sheet1".to_string()
        } else {
            text("sheet").to_string()
        };
        if let Some(p) = sheet_name_problem(&sheet) {
            problems.push(p);
        }
        let mut columns: Vec<(String, ColumnType)> = Vec::new();
        let mut seen = BTreeSet::new();
        for (i, c) in v
            .get("columns")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let header = c
                .get("header")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let kind = c
                .get("type")
                .and_then(Value::as_str)
                .and_then(ColumnType::parse);
            if header.is_empty() {
                problems.push(format!("column {} has no header", i + 1));
                continue;
            }
            if !seen.insert(header.to_lowercase()) {
                problems.push(format!("column header {header:?} is used twice"));
            }
            match kind {
                Some(k) => columns.push((header, k)),
                None => problems.push(format!(
                    "column {header:?} has no type (text, number, integer, currency, percent or date)"
                )),
            }
        }
        if columns.is_empty() {
            problems.push("the table needs at least one column".into());
        }
        if columns.len() > MAX_COLUMNS {
            problems.push(format!(
                "the table has {} columns; keep it to {MAX_COLUMNS}",
                columns.len()
            ));
        }
        let mut computed = Vec::new();
        let find = |cols: &[(String, ColumnType)], name: &str| {
            cols.iter()
                .position(|(h, _)| h.eq_ignore_ascii_case(name.trim()))
        };
        for c in v
            .get("computed")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let header = c
                .get("header")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let op = c
                .get("op")
                .and_then(Value::as_str)
                .and_then(ComputeOp::parse);
            let left = c.get("left").and_then(Value::as_str).unwrap_or("");
            let right = c.get("right").and_then(Value::as_str).unwrap_or("");
            if header.is_empty() {
                problems.push("a computed column has no header".into());
                continue;
            }
            if !seen.insert(header.to_lowercase()) {
                problems.push(format!("column header {header:?} is used twice"));
            }
            let (Some(l), Some(r)) = (find(&columns, left), find(&columns, right)) else {
                problems.push(format!(
                    "computed column {header:?} names {left:?} and {right:?}; both must be columns of the table"
                ));
                continue;
            };
            if !columns[l].1.is_numeric() || !columns[r].1.is_numeric() {
                problems.push(format!(
                    "computed column {header:?} needs two numeric columns; {left:?} and {right:?} are not both numeric"
                ));
                continue;
            }
            let Some(op) = op else {
                problems.push(format!(
                    "computed column {header:?} needs op add, subtract, multiply or divide"
                ));
                continue;
            };
            let kind = match op {
                ComputeOp::Divide => ColumnType::Number,
                _ if columns[l].1 == ColumnType::Currency
                    || columns[r].1 == ColumnType::Currency =>
                {
                    ColumnType::Currency
                }
                _ => columns[l].1,
            };
            computed.push(Computed {
                header,
                op,
                left: l,
                right: r,
                kind,
            });
        }
        if computed.len() > MAX_COMPUTED {
            problems.push(format!(
                "{} computed columns; keep it to {MAX_COMPUTED}",
                computed.len()
            ));
        }
        let mut rows = Vec::new();
        let raw_rows = v
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if raw_rows.len() > MAX_ROWS {
            problems.push(format!(
                "{} rows; a created table holds at most {MAX_ROWS}",
                raw_rows.len()
            ));
        }
        for (ri, r) in raw_rows.iter().take(MAX_ROWS).enumerate() {
            let Some(cells) = r.as_array() else {
                problems.push(format!("row {} is not a list of cells", ri + 1));
                continue;
            };
            if cells.len() != columns.len() {
                problems.push(format!(
                    "row {} has {} cells for {} columns",
                    ri + 1,
                    cells.len(),
                    columns.len()
                ));
                continue;
            }
            let mut out = Vec::new();
            for (ci, cell) in cells.iter().enumerate() {
                match cell_from_json(cell, columns[ci].1) {
                    Ok(c) => out.push(c),
                    Err(e) => {
                        problems.push(format!("row {}, column {:?}: {e}", ri + 1, columns[ci].0));
                        out.push(CellValue::Blank);
                    }
                }
            }
            rows.push(out);
        }
        let all_headers: Vec<(String, ColumnType)> = columns
            .iter()
            .cloned()
            .chain(computed.iter().map(|c| (c.header.clone(), c.kind)))
            .collect();
        let mut totals = Vec::new();
        for t in v
            .get("total_columns")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let name = t.as_str().unwrap_or("");
            match find(&all_headers, name) {
                Some(i) if all_headers[i].1.is_numeric() => {
                    if !totals.contains(&i) {
                        totals.push(i)
                    }
                }
                Some(_) => problems.push(format!(
                    "{name:?} is not a numeric column, so it cannot be totalled"
                )),
                None => problems.push(format!(
                    "total column {name:?} is not a column of the table"
                )),
            }
        }
        totals.sort_unstable();
        let currency_symbol = v
            .get("currency_symbol")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(4).collect::<String>());
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(WorkbookSpec {
            title,
            sheet,
            columns,
            rows,
            computed,
            totals,
            currency_symbol,
            notes,
        })
    }

    pub fn headers(&self) -> Vec<(String, ColumnType)> {
        self.columns
            .iter()
            .cloned()
            .chain(self.computed.iter().map(|c| (c.header.clone(), c.kind)))
            .collect()
    }

    fn total_row(&self) -> Option<u32> {
        (!self.totals.is_empty() && !self.rows.is_empty()).then(|| self.rows.len() as u32 + 2)
    }
}

/// Repair the structural slips a small model makes when it designs a
/// table — the arithmetic and layout are this code's job, so a slip in
/// them is corrected here instead of costing the user the whole table.
/// Only ever REMOVES: a typed column that duplicates a computed one (its
/// model-computed values are replaced by the formula), a first row that
/// repeats the headers, template example rows (two or more cells spelled
/// like their header plus a number: "Company1", "Role 1"), computed
/// columns over non-numeric operands, and totals over columns that cannot
/// be totalled. Every repair is returned as a note. Figures are never
/// touched: whether they come from the description is the verifier's
/// question, not this function's.
/// Where a figure is written in the text, whatever its formatting
/// ("1,200" and "1200" are the same figure): byte offsets of each match.
pub(crate) fn figure_positions(text: &str, x: f64) -> Vec<usize> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\d[\d,]*(?:\.\d+)?").expect("number regex"));
    re.find_iter(text)
        .filter(|m| {
            m.as_str()
                .replace(',', "")
                .parse::<f64>()
                .is_ok_and(|y| (y - x).abs() < 1e-9)
        })
        .map(|m| m.start())
        .collect()
}

/// A figure the description gives once, copied into a second column of
/// the same row (the iOS simulator run: "internet 60" under Planned also
/// written as Spent 60). Code can tell where it belongs without guessing
/// when the description names a column before the figure — "Planned: …
/// internet 60. Spent so far: …" — so the copy under the other column is
/// emptied and the repair reported. Like [`normalize_table_spec`] it only
/// removes; when no column is named before the figure nothing changes and
/// the verifier's problem stands.
pub fn repair_duplicated_figures(v: &Value, source: &str) -> (Value, Vec<String>) {
    let mut out = v.clone();
    let mut notes = Vec::new();
    if source.trim().is_empty() {
        return (out, notes);
    }
    let lower_source = source.to_lowercase();
    let columns: Vec<(String, String)> = out
        .get("columns")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|c| {
                    (
                        c.get("header")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                        c.get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let numeric = |t: &str| matches!(t, "number" | "integer" | "currency" | "percent");
    let label_col = columns.iter().position(|(_, t)| t == "text");
    // The last column named at or before `pos` in the description.
    let named_before = |pos: usize, candidates: &[usize]| -> Option<usize> {
        candidates
            .iter()
            .filter_map(|&ci| {
                let h = columns[ci].0.to_lowercase();
                if h.is_empty() {
                    return None;
                }
                lower_source[..pos].rfind(&h).map(|at| (at, ci))
            })
            .max()
            .map(|(_, ci)| ci)
    };
    let Some(rows) = out.get_mut("rows").and_then(Value::as_array_mut) else {
        return (out, notes);
    };
    for (ri, row) in rows.iter_mut().enumerate() {
        let Some(cells) = row.as_array_mut() else {
            continue;
        };
        let label = label_col
            .and_then(|c| cells.get(c))
            .and_then(Value::as_str)
            .map(|l| l.trim().to_string())
            .unwrap_or_default();
        let value_at = |c: &Value| {
            c.as_f64()
                .or_else(|| c.as_str().and_then(parse_number_text).map(|(x, _)| x))
        };
        let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
        for (ci, cell) in cells.iter().enumerate() {
            let Some((_, t)) = columns.get(ci) else {
                continue;
            };
            if !numeric(t) {
                continue;
            }
            if let Some(x) = value_at(cell) {
                match groups.iter_mut().find(|(y, _)| *y == x) {
                    Some((_, cis)) => cis.push(ci),
                    None => groups.push((x, vec![ci])),
                }
            }
        }
        for (x, cis) in groups {
            let mut positions = figure_positions(source, x);
            if cis.len() < 2 || positions.len() >= cis.len() || positions.is_empty() {
                continue;
            }
            // Prefer the mentions that follow the row's own label.
            if !label.is_empty() {
                let l = label.to_lowercase();
                let near: Vec<usize> = positions
                    .iter()
                    .copied()
                    .filter(|&p| {
                        let from = p.saturating_sub(l.len() + 24);
                        lower_source.get(from..p).is_some_and(|w| w.contains(&l))
                    })
                    .collect();
                if !near.is_empty() {
                    positions = near;
                }
            }
            let keep: Vec<usize> = positions
                .iter()
                .filter_map(|&p| named_before(p, &cis))
                .collect();
            if keep.is_empty() || keep.len() >= cis.len() {
                continue;
            }
            let mut emptied = Vec::new();
            for &ci in &cis {
                if !keep.contains(&ci) {
                    cells[ci] = Value::Null;
                    emptied.push(format!("{:?}", columns[ci].0));
                }
            }
            let shown = if x.fract() == 0.0 {
                format!("{}", x as i64)
            } else {
                format!("{x}")
            };
            let row_name = if label.is_empty() {
                String::new()
            } else {
                format!(" ({label})")
            };
            notes.push(format!(
                "row {}{row_name}: the description gives {shown} once, under {:?}; the copy under {} was removed",
                ri + 1,
                columns[keep[0]].0,
                emptied.join(" and ")
            ));
        }
    }
    (out, notes)
}

pub fn normalize_table_spec(v: &Value) -> (Value, Vec<String>) {
    let mut out = v.clone();
    let mut notes = Vec::new();
    let lower = |s: &str| s.trim().to_lowercase();
    let header_of = |c: &Value| {
        c.get("header")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let computed_headers: BTreeSet<String> = out
        .get("computed")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|c| lower(&header_of(c))).collect())
        .unwrap_or_default();
    // (a) a typed column that a computed column replaces.
    if let Some(columns) = out.get("columns").and_then(Value::as_array).cloned() {
        let n = columns.len();
        let dup: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, c)| computed_headers.contains(&lower(&header_of(c))))
            .map(|(i, _)| i)
            .collect();
        if !dup.is_empty() && dup.len() < n {
            for &i in &dup {
                notes.push(format!(
                    "{:?} is computed by a formula, so the values typed for it were dropped",
                    header_of(&columns[i])
                ));
            }
            let keep: Vec<Value> = columns
                .iter()
                .enumerate()
                .filter(|(i, _)| !dup.contains(i))
                .map(|(_, c)| c.clone())
                .collect();
            out["columns"] = Value::Array(keep);
            if let Some(rows) = out.get_mut("rows").and_then(Value::as_array_mut) {
                for row in rows.iter_mut() {
                    if let Some(cells) = row.as_array_mut() {
                        if cells.len() == n {
                            let kept: Vec<Value> = cells
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| !dup.contains(i))
                                .map(|(_, c)| c.clone())
                                .collect();
                            *cells = kept;
                        }
                    }
                }
            }
        }
    }
    let headers: Vec<String> = out
        .get("columns")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(header_of).collect())
        .unwrap_or_default();
    let numeric: BTreeSet<String> = out
        .get("columns")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|c| {
                    c.get("type")
                        .and_then(Value::as_str)
                        .and_then(ColumnType::parse)
                        .map(|t| t.is_numeric())
                        .unwrap_or(false)
                })
                .map(|c| lower(&header_of(c)))
                .collect()
        })
        .unwrap_or_default();
    if let Some(rows) = out.get_mut("rows").and_then(Value::as_array_mut) {
        // (b) the header row repeated as the first data row.
        let repeats_headers = rows
            .first()
            .and_then(Value::as_array)
            .map(|cells| {
                !headers.is_empty()
                    && cells.len() == headers.len()
                    && cells
                        .iter()
                        .zip(&headers)
                        .all(|(c, h)| c.as_str().map(|s| lower(s) == lower(h)).unwrap_or(false))
            })
            .unwrap_or(false);
        if repeats_headers {
            rows.remove(0);
            notes.push("the first row repeated the column headers and was removed".into());
        }
        // (c) template example rows.
        let before = rows.len();
        rows.retain(|row| {
            let Some(cells) = row.as_array() else {
                return true;
            };
            let templated = cells
                .iter()
                .zip(&headers)
                .filter(|(c, h)| {
                    let Some(t) = c.as_str() else { return false };
                    let (t, h) = (lower(t), lower(h));
                    t.strip_prefix(h.as_str())
                        .map(|rest| {
                            let rest = rest.trim();
                            !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
                        })
                        .unwrap_or(false)
                })
                .count();
            templated < 2
        });
        if rows.len() < before {
            notes.push(format!(
                "{} example row(s) such as \"Company1\" were removed; the description lists no such items",
                before - rows.len()
            ));
        }
    }
    // (d) computed columns over operands that are not numeric columns.
    let mut dropped_computed = BTreeSet::new();
    if let Some(computed) = out.get_mut("computed").and_then(Value::as_array_mut) {
        computed.retain(|c| {
            let ok = ["left", "right"].iter().all(|k| {
                c.get(*k)
                    .and_then(Value::as_str)
                    .map(|s| numeric.contains(&lower(s)))
                    .unwrap_or(false)
            });
            if !ok {
                let h = c
                    .get("header")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                notes.push(format!(
                    "computed column {h:?} was dropped: it needs two numeric columns of the table"
                ));
                dropped_computed.insert(lower(&h));
            }
            ok
        });
    }
    // (e) totals over columns that cannot be totalled.
    let computed_now: BTreeSet<String> = out
        .get("computed")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|c| lower(&header_of(c))).collect())
        .unwrap_or_default();
    if let Some(totals) = out.get_mut("total_columns").and_then(Value::as_array_mut) {
        totals.retain(|t| {
            let name = t.as_str().map(lower).unwrap_or_default();
            let ok = numeric.contains(&name) || computed_now.contains(&name);
            if !ok && !dropped_computed.contains(&name) {
                notes.push(format!(
                    "no total for {:?}: only numeric columns can be totalled",
                    t.as_str().unwrap_or_default()
                ));
            }
            ok
        });
    }
    (out, notes)
}

fn number_format(kind: ColumnType, symbol: Option<&str>) -> Option<String> {
    match kind {
        ColumnType::Currency => Some(match symbol {
            Some(s) if s.chars().all(|c| c.is_ascii_alphabetic()) => {
                format!("#,##0.00\" {s}\"")
            }
            Some(s) => format!("\"{s}\"#,##0.00"),
            None => "#,##0.00".into(),
        }),
        ColumnType::Integer => Some("#,##0".into()),
        // Not "#,##0.##": Excel shows 1200 as "1,200." and Apple's
        // renderer (Files preview, Quick Look) shows the cell blank.
        ColumnType::Number => Some("#,##0.00".into()),
        ColumnType::Percent => Some("0.0%".into()),
        ColumnType::Text | ColumnType::Date => None,
    }
}

/// The creation batch for a table: header row, data rows, computed columns
/// and a total row whose formulas this code writes (the model never
/// spells a formula), plus the layout and number formats as metadata.
pub fn workbook_batch(spec: &WorkbookSpec) -> ArtifactBatch {
    let mut ops = Ops::new();
    let sid = "s1";
    ops.push(
        OpKind::MetadataSet,
        "meta:title".into(),
        JsonValue::object([("key", js("title")), ("value", js(spec.title.clone()))]),
    );
    ops.push(
        OpKind::SheetInsert,
        format!("sheet:{sid}"),
        JsonValue::object([
            ("sheet_id", js(sid)),
            ("name", js(spec.sheet.clone())),
            ("index", ji(0)),
        ]),
    );
    let headers = spec.headers();
    let ncols = headers.len() as u32;
    let cell = |ops: &mut Ops, col: u32, row: u32, kind: &str, value: JsonValue| {
        let address = addr(col, row);
        ops.push(
            OpKind::CellSet,
            format!("cell:{sid}:{address}"),
            JsonValue::object([
                ("sheet_id", js(sid)),
                ("address", js(address)),
                ("value", value),
                ("value_kind", js(kind)),
            ]),
        );
    };
    for (i, (h, _)) in headers.iter().enumerate() {
        cell(&mut ops, i as u32 + 1, 1, "text", js(h.clone()));
    }
    let first = 2u32;
    let last = spec.rows.len() as u32 + 1;
    for (ri, row) in spec.rows.iter().enumerate() {
        let r = first + ri as u32;
        for (ci, v) in row.iter().enumerate() {
            let col = ci as u32 + 1;
            match v {
                CellValue::Blank => {}
                CellValue::Number(n) => {
                    cell(&mut ops, col, r, "number_decimal", js(decimal_string(*n)))
                }
                CellValue::Text(t) => cell(&mut ops, col, r, "text", js(t.clone())),
                CellValue::Bool(b) => cell(&mut ops, col, r, "boolean", JsonValue::Bool(*b)),
                CellValue::Error(_) => {}
            }
        }
        for (k, c) in spec.computed.iter().enumerate() {
            let col = spec.columns.len() as u32 + k as u32 + 1;
            let (l, rr) = (addr(c.left as u32 + 1, r), addr(c.right as u32 + 1, r));
            let formula = match c.op {
                ComputeOp::Add => format!("={l}+{rr}"),
                ComputeOp::Subtract => format!("={l}-{rr}"),
                ComputeOp::Multiply => format!("={l}*{rr}"),
                ComputeOp::Divide => format!("=IFERROR({l}/{rr},\"\")"),
            };
            cell(&mut ops, col, r, "formula", js(formula));
        }
    }
    if let Some(total_row) = spec.total_row() {
        let label_col = (0..headers.len()).find(|i| !spec.totals.contains(i));
        if let Some(lc) = label_col {
            cell(&mut ops, lc as u32 + 1, total_row, "text", js("Total"));
        }
        for &t in &spec.totals {
            let col = t as u32 + 1;
            let formula = format!("=SUM({}:{})", addr(col, first), addr(col, last));
            cell(&mut ops, col, total_row, "formula", js(formula));
        }
    }
    let last_row = spec.total_row().unwrap_or(last.max(1));
    ops.push(
        OpKind::MetadataSet,
        format!("meta:layout.{sid}"),
        JsonValue::object([
            ("key", js(format!("layout.{sid}"))),
            (
                "value",
                JsonValue::object([
                    ("header_row", ji(1)),
                    (
                        "total_row",
                        spec.total_row()
                            .map(|r| ji(r as i64))
                            .unwrap_or(JsonValue::Null),
                    ),
                    ("columns", ji(ncols as i64)),
                ]),
            ),
        ]),
    );
    for (i, (_, kind)) in headers.iter().enumerate() {
        if number_format(*kind, spec.currency_symbol.as_deref()).is_none() || last_row < 2 {
            continue;
        }
        let col = harbor_artifacts::workbook::col_letter(i as u32 + 1);
        let mut value = vec![
            ("format", js(kind.as_str())),
            ("from_row", ji(2)),
            ("to_row", ji(last_row as i64)),
        ];
        if let (ColumnType::Currency, Some(sym)) = (kind, &spec.currency_symbol) {
            value.push(("symbol", js(sym.clone())));
        }
        ops.push(
            OpKind::MetadataSet,
            format!("meta:format.{sid}.{col}"),
            JsonValue::object([
                ("key", js(format!("format.{sid}.{col}"))),
                ("value", JsonValue::object(value)),
            ]),
        );
    }
    ops.into_batch(CreatedKind::Xlsx)
}

// ---------------------------------------------------------------------------
// Deck outline

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineBullet {
    pub text: String,
    /// Source unit (page, paragraph, slide) the bullet cites, if any.
    pub cite: Option<(i64, String)>,
    /// Numbered sentence the bullet points at (`document.sentences`): the
    /// model points, the code copies the sentence into the notes.
    pub source: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineSlide {
    pub title: String,
    pub bullets: Vec<OutlineBullet>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckOutline {
    pub title: String,
    pub subtitle: Option<String>,
    pub slides: Vec<OutlineSlide>,
}

pub const MAX_SLIDES: usize = 30;

/// Remove what a small model repeats to fill a slide count: a bullet whose
/// text, or whose cited sentence, already appears earlier in the deck, and
/// a slide that is left with no bullets. Only ever removes; every removal
/// is returned as a note, so the verifier checks the deck that will be
/// built and the user sees what was dropped.
pub fn normalize_outline(v: &Value) -> (Value, Vec<String>) {
    let mut out = v.clone();
    let mut notes = Vec::new();
    let mut seen_text: BTreeSet<String> = BTreeSet::new();
    let mut seen_source: BTreeSet<i64> = BTreeSet::new();
    let fold = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_end_matches(['.', '!', '?'])
            .to_lowercase()
    };
    let mut dropped_bullets = 0usize;
    let mut dropped_slides = Vec::new();
    if let Some(slides) = out.get_mut("slides").and_then(Value::as_array_mut) {
        for (i, slide) in slides.iter_mut().enumerate() {
            if let Some(bullets) = slide.get_mut("bullets").and_then(Value::as_array_mut) {
                let before = bullets.len();
                bullets.retain(|b| {
                    let text = b
                        .as_str()
                        .or_else(|| b.get("text").and_then(Value::as_str))
                        .unwrap_or("");
                    let repeat_text = !seen_text.insert(fold(text));
                    let repeat_source = b
                        .get("source")
                        .and_then(Value::as_i64)
                        .map(|n| !seen_source.insert(n))
                        .unwrap_or(false);
                    !(repeat_text || repeat_source)
                });
                dropped_bullets += before - bullets.len();
                if before > 0 && bullets.is_empty() {
                    dropped_slides.push(i + 1);
                }
            }
        }
        let mut i = 0usize;
        slides.retain(|_| {
            i += 1;
            !dropped_slides.contains(&i)
        });
    }
    if dropped_bullets > 0 {
        notes.push(format!(
            "{dropped_bullets} repeated bullet(s) were removed; each point appears once"
        ));
    }
    for n in &dropped_slides {
        notes.push(format!(
            "slide {n} only repeated earlier slides and was removed"
        ));
    }
    (out, notes)
}

impl DeckOutline {
    pub fn from_value(v: &Value) -> Result<DeckOutline, Vec<String>> {
        let mut problems = Vec::new();
        let title = v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() {
            problems.push("the deck needs a title".into());
        }
        let subtitle = optional_text(v.get("subtitle"));
        let mut slides = Vec::new();
        let raw = v
            .get("slides")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if raw.is_empty() {
            problems.push("the deck has no slides".into());
        }
        if raw.len() > MAX_SLIDES {
            problems.push(format!("{} slides; keep it to {MAX_SLIDES}", raw.len()));
        }
        for (i, s) in raw.iter().take(MAX_SLIDES).enumerate() {
            let t = s
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let mut bullets = Vec::new();
            for b in s
                .get("bullets")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                match &b {
                    Value::String(text) => {
                        if !text.trim().is_empty() && !is_null_word(text) {
                            bullets.push(OutlineBullet {
                                text: text.trim().to_string(),
                                cite: None,
                                source: None,
                            })
                        }
                    }
                    Value::Object(_) => {
                        let text = b
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if text.is_empty() || is_null_word(&text) {
                            continue;
                        }
                        let cite = match (
                            b.get("ref").and_then(Value::as_i64),
                            b.get("quote").and_then(Value::as_str),
                        ) {
                            (Some(r), Some(q)) => Some((r, q.to_string())),
                            _ => None,
                        };
                        let source = b.get("source").and_then(Value::as_i64);
                        bullets.push(OutlineBullet { text, cite, source });
                    }
                    _ => problems.push(format!("slide {}: a bullet is not text", i + 1)),
                }
            }
            if t.is_empty() {
                problems.push(format!("slide {} has no title", i + 1));
            }
            slides.push(OutlineSlide {
                title: t,
                bullets,
                notes: optional_text(s.get("notes")),
            });
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(DeckOutline {
            title,
            subtitle,
            slides,
        })
    }
}

/// Numbered sentences of a source document: number → (unit, text), as
/// `document.sentences` returns them.
pub type SentenceTable = BTreeMap<i64, (i64, String)>;

pub fn sentence_table(v: &Value) -> SentenceTable {
    v.get("sentences")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some((
                        x.get("n")?.as_i64()?,
                        (
                            x.get("unit")?.as_i64()?,
                            x.get("text")?.as_str()?.to_string(),
                        ),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Speaker notes for a slide: the outline's own notes, then the sources
/// its bullets cite, so every sourced claim carries its source into the
/// deck (presentation-builder's contract).
fn slide_notes(
    slide: &OutlineSlide,
    unit_label: &str,
    sentences: Option<&SentenceTable>,
) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(n) = &slide.notes {
        lines.push(n.clone());
    }
    let cited: Vec<String> = slide
        .bullets
        .iter()
        .filter_map(|b| {
            if let Some((r, q)) = &b.cite {
                return Some((*r, q.trim().to_string()));
            }
            let (unit, text) = sentences?.get(&b.source?)?;
            Some((*unit, text.clone()))
        })
        .map(|(r, q)| format!("{unit_label} {r}: \u{201c}{q}\u{201d}"))
        .collect();
    if !cited.is_empty() {
        lines.push("Sources:".into());
        lines.extend(cited);
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub fn deck_batch(
    outline: &DeckOutline,
    unit_label: &str,
    sentences: Option<&SentenceTable>,
) -> ArtifactBatch {
    let mut ops = Ops::new();
    ops.push(
        OpKind::MetadataSet,
        "meta:title".into(),
        JsonValue::object([("key", js("title")), ("value", js(outline.title.clone()))]),
    );
    let slide = |ops: &mut Ops,
                 index: usize,
                 layout: &str,
                 title: &str,
                 bullets: &[String],
                 notes: Option<String>| {
        let slide_id = format!("sl{}", index + 1);
        ops.push(
            OpKind::SlideInsert,
            format!("slide:{slide_id}"),
            JsonValue::object([
                ("slide_id", js(slide_id.clone())),
                ("index", ji(index as i64)),
                ("layout_id", js(layout)),
            ]),
        );
        let mut elements = vec![JsonValue::object([
            ("kind", js("title")),
            ("text", js(title)),
        ])];
        if !bullets.is_empty() {
            elements.push(JsonValue::object([
                ("kind", js("bullets")),
                (
                    "items",
                    JsonValue::Array(bullets.iter().map(|b| js(b.clone())).collect()),
                ),
            ]));
        }
        if let Some(n) = notes {
            elements.push(JsonValue::object([("kind", js("notes")), ("text", js(n))]));
        }
        ops.push(
            OpKind::SlideUpdate,
            format!("slide:{slide_id}"),
            JsonValue::object([
                ("slide_id", js(slide_id)),
                ("elements", JsonValue::Array(elements)),
            ]),
        );
    };
    let cover_lines: Vec<String> = outline.subtitle.iter().cloned().collect();
    slide(&mut ops, 0, "title", &outline.title, &cover_lines, None);
    for (i, s) in outline.slides.iter().enumerate() {
        let bullets: Vec<String> = s.bullets.iter().map(|b| b.text.clone()).collect();
        slide(
            &mut ops,
            i + 1,
            "content",
            &s.title,
            &bullets,
            slide_notes(s, unit_label, sentences),
        );
    }
    ops.into_batch(CreatedKind::Pptx)
}

// ---------------------------------------------------------------------------
// Document outline

#[derive(Debug, Clone, PartialEq)]
pub struct DocSection {
    pub heading: String,
    pub paragraphs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSpec {
    pub title: String,
    pub sections: Vec<DocSection>,
}

/// How sections become blocks: headed (proposal, report, memo) or as a
/// letter (address lines, salutation and sign-off from the inputs, no
/// visible section headings).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LetterParts {
    pub date: Option<String>,
    pub recipient: Option<String>,
    pub sender: Option<String>,
}

impl DocumentSpec {
    pub fn from_value(v: &Value) -> Result<DocumentSpec, Vec<String>> {
        let mut problems = Vec::new();
        let title = v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() {
            problems.push("the document needs a title".into());
        }
        let mut sections = Vec::new();
        for (i, s) in v
            .get("sections")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let heading = s
                .get("heading")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let paragraphs: Vec<String> = s
                .get("paragraphs")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|p| !p.is_empty() && !is_null_word(p))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if heading.is_empty() {
                problems.push(format!("section {} has no heading", i + 1));
            }
            sections.push(DocSection {
                heading,
                paragraphs,
            });
        }
        if sections.is_empty() {
            problems.push("the document has no sections".into());
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(DocumentSpec { title, sections })
    }
}

/// Paragraph text split on blank lines, so each paragraph is one block.
fn split_paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect()
}

/// A paragraph written as a list ("- item" lines) becomes list blocks.
fn paragraph_blocks(text: &str) -> Vec<(BlockStyle, String)> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let bulleted = lines.len() > 1
        && lines
            .iter()
            .all(|l| l.starts_with("- ") || l.starts_with("• ") || l.starts_with("* "));
    if bulleted {
        return lines
            .iter()
            .map(|l| (BlockStyle::Bullet, l[2..].trim().to_string()))
            .collect();
    }
    split_paragraphs(text)
        .into_iter()
        .map(|p| (BlockStyle::Paragraph, p))
        .collect()
}

fn salutation_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)^(dear|hi|hello|to whom it may concern|good (morning|afternoon|evening))\b[^\n]{0,80}$")
            .expect("salutation regex")
    })
}

fn sign_off_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)^(yours (sincerely|faithfully|truly)|sincerely|best regards|kind regards|warm regards|regards|best wishes|best|many thanks|thanks)[,.!]?$")
            .expect("sign-off regex")
    })
}

/// A letter's salutation and sign-off are layout, and Harbor writes them
/// from the inputs. Models add their own anyway ("Dear Sir/Madam", "Yours
/// sincerely, [Your Name]"), which would print twice — so a leading
/// salutation line and a trailing sign-off block (with whatever name or
/// placeholder follows it) are removed from each body paragraph.
pub fn strip_letter_furniture(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().map(str::trim).collect();
    while lines.first().map(|l| l.is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    if lines
        .first()
        .map(|l| salutation_re().is_match(l))
        .unwrap_or(false)
    {
        lines.remove(0);
    }
    if let Some(i) = lines.iter().rposition(|l| sign_off_re().is_match(l)) {
        // Only a block at the end: a sign-off followed by at most two
        // short lines (a name, a title, a placeholder).
        let tail = &lines[i + 1..];
        if tail.len() <= 2 && tail.iter().all(|l| l.split_whitespace().count() <= 6) {
            lines.truncate(i);
        }
    }
    let joined = lines.join("\n");
    let mut out = String::new();
    for para in joined.split("\n\n") {
        let p = para.trim();
        if p.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(p);
    }
    out
}

pub fn document_batch(spec: &DocumentSpec, letter: Option<&LetterParts>) -> ArtifactBatch {
    let mut ops = Ops::new();
    ops.push(
        OpKind::MetadataSet,
        "meta:title".into(),
        JsonValue::object([("key", js("title")), ("value", js(spec.title.clone()))]),
    );
    let mut index = 0usize;
    let mut block = |ops: &mut Ops, style: BlockStyle, text: &str| {
        let block_id = format!("b{}", index + 1);
        ops.push(
            OpKind::BlockInsert,
            format!("block:{block_id}"),
            JsonValue::object([
                ("block_id", js(block_id)),
                ("index", ji(index as i64)),
                ("style", js(style.as_str())),
                ("text", js(text)),
            ]),
        );
        index += 1;
    };
    match letter {
        None => {
            block(&mut ops, BlockStyle::Title, &spec.title);
            for s in &spec.sections {
                block(&mut ops, BlockStyle::Heading1, &s.heading);
                for p in &s.paragraphs {
                    for (style, text) in paragraph_blocks(p) {
                        block(&mut ops, style, &text);
                    }
                }
            }
        }
        Some(parts) => {
            if let Some(sender) = &parts.sender {
                for line in sender.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    block(&mut ops, BlockStyle::Compact, line);
                }
                block(&mut ops, BlockStyle::Paragraph, "");
            }
            if let Some(date) = &parts.date {
                block(&mut ops, BlockStyle::Paragraph, date.trim());
            }
            if let Some(recipient) = &parts.recipient {
                for line in recipient.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    block(&mut ops, BlockStyle::Compact, line);
                }
                block(&mut ops, BlockStyle::Paragraph, "");
            }
            block(&mut ops, BlockStyle::Heading2, &spec.title);
            let salutation = parts
                .recipient
                .as_deref()
                .and_then(|r| r.lines().map(str::trim).find(|l| !l.is_empty()))
                .map(|name| format!("Dear {name},"))
                .unwrap_or_else(|| "Dear Sir or Madam,".into());
            block(&mut ops, BlockStyle::Paragraph, &salutation);
            for s in &spec.sections {
                for p in &s.paragraphs {
                    for (style, text) in paragraph_blocks(&strip_letter_furniture(p)) {
                        block(&mut ops, style, &text);
                    }
                }
            }
            block(&mut ops, BlockStyle::Compact, "Yours sincerely,");
            if let Some(sender) = &parts.sender {
                if let Some(name) = sender.lines().map(str::trim).find(|l| !l.is_empty()) {
                    block(&mut ops, BlockStyle::Compact, "");
                    block(&mut ops, BlockStyle::Compact, name);
                }
            }
        }
    }
    ops.into_batch(CreatedKind::Docx)
}

// ---------------------------------------------------------------------------
// Rendering a creation batch

fn arg<'a>(op: &'a Operation, key: &str) -> Result<&'a JsonValue, String> {
    op.args
        .get(key)
        .ok_or_else(|| format!("op {}: {} needs args.{key}", op.op_id, op.kind.as_str()))
}

fn arg_str<'a>(op: &'a Operation, key: &str) -> Result<&'a str, String> {
    arg(op, key)?
        .as_str()
        .ok_or_else(|| format!("op {}: args.{key} must be a string", op.op_id))
}

fn arg_int(op: &Operation, key: &str) -> Result<i64, String> {
    arg(op, key)?
        .as_int()
        .ok_or_else(|| format!("op {}: args.{key} must be an integer", op.op_id))
}

fn json_int(v: Option<&JsonValue>) -> Option<i64> {
    v.and_then(|x| x.as_int())
}

/// The kind a creation batch builds, from its operations. Every operation
/// must belong to one format (metadata belongs to any).
pub fn creation_kind(batch: &ArtifactBatch) -> Result<CreatedKind, String> {
    if !batch.is_creation() {
        return Err("not a creation batch: its base is not the empty artifact".into());
    }
    let mut kind: Option<CreatedKind> = None;
    for op in &batch.operations {
        if op.precondition.expected_content_hash != EMPTY_CONTENT_HASH {
            return Err(format!(
                "op {}: a creation batch may only target what does not exist yet",
                op.op_id
            ));
        }
        let k = match op.kind {
            OpKind::SheetInsert | OpKind::CellSet => CreatedKind::Xlsx,
            OpKind::SlideInsert | OpKind::SlideUpdate => CreatedKind::Pptx,
            OpKind::BlockInsert => CreatedKind::Docx,
            OpKind::MetadataSet => continue,
            other => {
                return Err(format!(
                    "op {}: {} does not create anything",
                    op.op_id,
                    other.as_str()
                ))
            }
        };
        match kind {
            None => kind = Some(k),
            Some(prev) if prev != k => {
                return Err(format!(
                    "op {}: {} mixes {} and {} operations in one new artifact",
                    op.op_id,
                    op.kind.as_str(),
                    prev.as_str(),
                    k.as_str()
                ))
            }
            _ => {}
        }
    }
    kind.ok_or_else(|| "a creation batch needs at least one content operation".into())
}

fn metadata(op: &Operation) -> Result<(String, &JsonValue), String> {
    Ok((arg_str(op, "key")?.to_string(), arg(op, "value")?))
}

/// Deterministic bytes for a creation batch. Refuses a package that fails
/// the integrity checks.
pub fn render_creation(batch: &ArtifactBatch) -> Result<Vec<u8>, String> {
    let bytes = match creation_kind(batch)? {
        CreatedKind::Xlsx => render_xlsx(batch)?,
        CreatedKind::Pptx => render_pptx(batch)?,
        CreatedKind::Docx => render_docx(batch)?,
    };
    let problems = harbor_artifacts::package_integrity(&bytes);
    if !problems.is_empty() {
        return Err(format!(
            "the created package failed its integrity checks: {}",
            problems.join("; ")
        ));
    }
    Ok(bytes)
}

fn render_xlsx(batch: &ArtifactBatch) -> Result<Vec<u8>, String> {
    let mut wb = WorkbookDoc::new_empty();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut sheets: Vec<(i64, String, String)> = Vec::new();
    for op in batch
        .operations
        .iter()
        .filter(|o| o.kind == OpKind::SheetInsert)
    {
        let id = arg_str(op, "sheet_id")?.to_string();
        let name = arg_str(op, "name")?.to_string();
        if let Some(p) = sheet_name_problem(&name) {
            return Err(format!("op {}: {p}", op.op_id));
        }
        sheets.push((arg_int(op, "index")?, id, name));
    }
    sheets.sort();
    for (_, id, name) in &sheets {
        if names.insert(id.clone(), name.clone()).is_some() {
            return Err(format!("sheet id {id} is inserted twice"));
        }
        wb.add_sheet(name).map_err(|e| e.to_string())?;
    }
    let known: Vec<String> = names.values().cloned().collect();
    let mut seen_cells = BTreeSet::new();
    let mut lengths: BTreeMap<(String, u32), usize> = BTreeMap::new();
    let mut layouts = Vec::new();
    let mut formats = Vec::new();
    for op in &batch.operations {
        match op.kind {
            OpKind::MetadataSet => {
                let (key, value) = metadata(op)?;
                if key == "title" {
                    wb.set_title(value.as_str().unwrap_or(""));
                } else if let Some(id) = key.strip_prefix("layout.") {
                    layouts.push((id.to_string(), value.clone()));
                } else if let Some(rest) = key.strip_prefix("format.") {
                    let (id, col) = rest
                        .split_once('.')
                        .ok_or_else(|| format!("op {}: bad format key {key}", op.op_id))?;
                    formats.push((id.to_string(), col.to_string(), value.clone()));
                } else {
                    return Err(format!(
                        "op {}: a workbook does not carry metadata {key}",
                        op.op_id
                    ));
                }
            }
            OpKind::CellSet => {
                let id = arg_str(op, "sheet_id")?;
                let sheet = names
                    .get(id)
                    .ok_or_else(|| format!("op {}: sheet {id} was never inserted", op.op_id))?
                    .clone();
                let address = arg_str(op, "address")?;
                let (col, row) = parse_addr(address)
                    .ok_or_else(|| format!("op {}: bad address {address}", op.op_id))?;
                if !seen_cells.insert((id.to_string(), col, row)) {
                    return Err(format!("op {}: {id}!{address} is set twice", op.op_id));
                }
                let kind = arg_str(op, "value_kind")?;
                let value = arg(op, "value")?;
                let (set, shown) = match kind {
                    "formula" => {
                        let f = value
                            .as_str()
                            .ok_or_else(|| format!("op {}: formula must be text", op.op_id))?;
                        check_formula_allowed(f, &known)
                            .map_err(|r| format!("op {}: formula refused: {r}", op.op_id))?;
                        (
                            CellSet::Formula(f.trim_start_matches('=').to_string()),
                            12usize,
                        )
                    }
                    "number_decimal" => {
                        let n = match value {
                            JsonValue::Int(i) => *i as f64,
                            JsonValue::Str(s) => s
                                .parse::<f64>()
                                .map_err(|_| format!("op {}: bad number {s}", op.op_id))?,
                            _ => return Err(format!("op {}: bad number", op.op_id)),
                        };
                        let shown = decimal_string(n).len() + 4;
                        (CellSet::Value(CellValue::Number(n)), shown)
                    }
                    "boolean" => (
                        CellSet::Value(CellValue::Bool(value.as_bool().unwrap_or(false))),
                        5,
                    ),
                    "blank" => (CellSet::Value(CellValue::Blank), 0),
                    "text" => {
                        let t = value.as_str().unwrap_or("").to_string();
                        let n = t.chars().count();
                        (CellSet::Value(CellValue::Text(t)), n)
                    }
                    other => return Err(format!("op {}: unknown value_kind {other}", op.op_id)),
                };
                let e = lengths.entry((sheet.clone(), col)).or_insert(0);
                *e = (*e).max(shown);
                wb.put_cell(&sheet, row, col, set)
                    .map_err(|e| format!("op {}: {e}", op.op_id))?;
            }
            _ => {}
        }
    }
    for (id, value) in &layouts {
        let sheet = names
            .get(id)
            .ok_or_else(|| format!("layout for unknown sheet {id}"))?
            .clone();
        let columns = json_int(value.get("columns")).unwrap_or(1).max(1) as u32;
        if let Some(h) = json_int(value.get("header_row")) {
            wb.set_bold(&sheet, h as u32, 1, columns)
                .map_err(|e| e.to_string())?;
            if h == 1 {
                wb.freeze_first_row(&sheet).map_err(|e| e.to_string())?;
            }
        }
        if let Some(t) = json_int(value.get("total_row")) {
            wb.set_bold(&sheet, t as u32, 1, columns)
                .map_err(|e| e.to_string())?;
        }
    }
    for (id, col, value) in &formats {
        let sheet = names
            .get(id)
            .ok_or_else(|| format!("format for unknown sheet {id}"))?
            .clone();
        let kind = value
            .get("format")
            .and_then(|f| f.as_str())
            .and_then(ColumnType::parse)
            .ok_or_else(|| format!("format for {id}.{col} names no column type"))?;
        let symbol = value.get("symbol").and_then(|s| s.as_str());
        let code = number_format(kind, symbol)
            .ok_or_else(|| format!("{} columns carry no number format", kind.as_str()))?;
        let c = harbor_artifacts::workbook::col_number(col);
        if c == 0 || c > harbor_artifacts::workbook::MAX_COL {
            return Err(format!("format for bad column {col}"));
        }
        let from = json_int(value.get("from_row")).unwrap_or(2).max(1) as u32;
        let to = json_int(value.get("to_row"))
            .unwrap_or(from as i64)
            .max(from as i64) as u32;
        wb.set_number_format(&sheet, c, from, to, &code)
            .map_err(|e| e.to_string())?;
    }
    for ((sheet, col), len) in &lengths {
        let width = ((*len + 3) as f64).clamp(9.0, 60.0);
        wb.set_column_width(sheet, *col, width)
            .map_err(|e| e.to_string())?;
    }
    wb.recalculate_all().map_err(|e| e.to_string())?;
    wb.to_bytes().map_err(|e| e.to_string())
}

fn render_pptx(batch: &ArtifactBatch) -> Result<Vec<u8>, String> {
    let mut title = String::new();
    let mut order: Vec<(i64, String, String)> = Vec::new();
    let mut content: BTreeMap<String, SlideContent> = BTreeMap::new();
    for op in &batch.operations {
        match op.kind {
            OpKind::MetadataSet => {
                let (key, value) = metadata(op)?;
                if key != "title" {
                    return Err(format!(
                        "op {}: a deck does not carry metadata {key}",
                        op.op_id
                    ));
                }
                title = value.as_str().unwrap_or("").to_string();
            }
            OpKind::SlideInsert => {
                let id = arg_str(op, "slide_id")?.to_string();
                let layout = arg_str(op, "layout_id")?.to_string();
                if !matches!(layout.as_str(), "title" | "content") {
                    return Err(format!("op {}: unknown layout {layout}", op.op_id));
                }
                if order.iter().any(|(_, s, _)| *s == id) {
                    return Err(format!("op {}: slide {id} is inserted twice", op.op_id));
                }
                order.push((arg_int(op, "index")?, id, layout));
            }
            OpKind::SlideUpdate => {
                let id = arg_str(op, "slide_id")?.to_string();
                let mut slide = SlideContent {
                    title: String::new(),
                    bullets: Vec::new(),
                    notes: None,
                    chart: None,
                    image: None,
                };
                for el in arg(op, "elements")?
                    .as_array()
                    .ok_or_else(|| format!("op {}: elements must be a list", op.op_id))?
                {
                    let kind = el.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                    match kind {
                        "title" => {
                            slide.title = el
                                .get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string()
                        }
                        "bullets" => {
                            slide.bullets = el
                                .get("items")
                                .and_then(|i| i.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|x| x.as_str().map(str::to_string))
                                        .collect()
                                })
                                .unwrap_or_default()
                        }
                        "notes" => {
                            slide.notes =
                                el.get("text").and_then(|t| t.as_str()).map(str::to_string)
                        }
                        "chart" => slide.chart = Some(chart_from_element(el, &op.op_id)?),
                        other => {
                            return Err(format!("op {}: unknown slide element {other:?}", op.op_id))
                        }
                    }
                }
                if content.insert(id.clone(), slide).is_some() {
                    return Err(format!("op {}: slide {id} is updated twice", op.op_id));
                }
            }
            _ => {}
        }
    }
    order.sort();
    let mut slides = Vec::new();
    let mut cover = false;
    for (pos, (_, id, layout)) in order.iter().enumerate() {
        if layout == "title" {
            if pos != 0 {
                return Err(format!(
                    "slide {id}: only the first slide can use the title layout"
                ));
            }
            cover = true;
        }
        slides.push(
            content
                .remove(id)
                .ok_or_else(|| format!("slide {id} has no content"))?,
        );
    }
    if let Some(stray) = content.keys().next() {
        return Err(format!("slide {stray} is updated but never inserted"));
    }
    PptxDeck { title, slides }
        .to_pptx_bytes_with(DeckStyle {
            first_slide_is_cover: cover,
        })
        .map_err(|e| e.to_string())
}

fn chart_from_element(el: &JsonValue, op_id: &str) -> Result<ChartSpec, String> {
    let kind = match el.get("chart_kind").and_then(|k| k.as_str()).unwrap_or("") {
        "bar" => ChartKind::Bar,
        "line" => ChartKind::Line,
        "pie" => ChartKind::Pie,
        other => return Err(format!("op {op_id}: unknown chart kind {other:?}")),
    };
    let categories: Vec<String> = el
        .get("categories")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut series = Vec::new();
    for s in el
        .get("series")
        .and_then(|s| s.as_array())
        .unwrap_or_default()
    {
        let name = s
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let mut values = Vec::new();
        for v in s
            .get("values")
            .and_then(|v| v.as_array())
            .unwrap_or_default()
        {
            let x = match v {
                JsonValue::Int(i) => *i as f64,
                JsonValue::Str(t) => t
                    .parse::<f64>()
                    .map_err(|_| format!("op {op_id}: bad chart value {t}"))?,
                _ => return Err(format!("op {op_id}: bad chart value")),
            };
            values.push(x);
        }
        if values.len() != categories.len() {
            return Err(format!(
                "op {op_id}: series {name:?} has {} values for {} categories",
                values.len(),
                categories.len()
            ));
        }
        series.push((name, values));
    }
    if series.is_empty() || categories.is_empty() {
        return Err(format!("op {op_id}: a chart needs categories and a series"));
    }
    Ok(ChartSpec {
        kind,
        title: el
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        categories,
        series,
    })
}

fn render_docx(batch: &ArtifactBatch) -> Result<Vec<u8>, String> {
    let mut title = String::new();
    let mut blocks: BTreeMap<i64, DocxBlock> = BTreeMap::new();
    for op in &batch.operations {
        match op.kind {
            OpKind::MetadataSet => {
                let (key, value) = metadata(op)?;
                if key != "title" {
                    return Err(format!(
                        "op {}: a document does not carry metadata {key}",
                        op.op_id
                    ));
                }
                title = value.as_str().unwrap_or("").to_string();
            }
            OpKind::BlockInsert => {
                let style_name = arg_str(op, "style")?;
                let style = BlockStyle::parse(style_name)
                    .ok_or_else(|| format!("op {}: unknown style {style_name}", op.op_id))?;
                let index = arg_int(op, "index")?;
                let block = DocxBlock {
                    style,
                    text: arg_str(op, "text")?.to_string(),
                };
                if blocks.insert(index, block).is_some() {
                    return Err(format!(
                        "op {}: block index {index} is used twice",
                        op.op_id
                    ));
                }
            }
            _ => {}
        }
    }
    let ordered: Vec<DocxBlock> = blocks.into_values().collect();
    create_docx(&title, &ordered).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Review view of a creation batch

/// What a new artifact will contain, grouped the way a reviewer reads it:
/// one entry per table row, per slide, per paragraph. Nothing exists
/// before, so `before` is always empty.
pub fn creation_diff(batch: &ArtifactBatch) -> Vec<DiffEntry> {
    let Ok(kind) = creation_kind(batch) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for op in &batch.operations {
        if op.kind == OpKind::MetadataSet {
            if let (Some("title"), Some(v)) = (
                op.args.get("key").and_then(|k| k.as_str()),
                op.args.get("value").and_then(|v| v.as_str()),
            ) {
                out.push(DiffEntry {
                    op_id: op.op_id.clone(),
                    kind: "metadata.set".into(),
                    target_id: op.precondition.target_id.clone(),
                    location: "Title".into(),
                    before: None,
                    after: Some(v.to_string()),
                });
            }
        }
    }
    match kind {
        CreatedKind::Xlsx => {
            let sheet_names: BTreeMap<String, String> = batch
                .operations
                .iter()
                .filter(|o| o.kind == OpKind::SheetInsert)
                .filter_map(|o| {
                    Some((
                        o.args.get("sheet_id")?.as_str()?.to_string(),
                        o.args.get("name")?.as_str()?.to_string(),
                    ))
                })
                .collect();
            let mut rows: BTreeMap<(String, u32), (String, BTreeMap<u32, String>)> =
                BTreeMap::new();
            for op in batch
                .operations
                .iter()
                .filter(|o| o.kind == OpKind::CellSet)
            {
                let id = op
                    .args
                    .get("sheet_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let Some((col, row)) = op
                    .args
                    .get("address")
                    .and_then(|a| a.as_str())
                    .and_then(parse_addr)
                else {
                    continue;
                };
                let shown = match (
                    op.args.get("value_kind").and_then(|k| k.as_str()),
                    op.args.get("value"),
                ) {
                    (Some("formula"), Some(JsonValue::Str(f))) => {
                        format!("={}", f.trim_start_matches('='))
                    }
                    (_, Some(JsonValue::Str(s))) => s.clone(),
                    (_, Some(JsonValue::Int(i))) => i.to_string(),
                    (_, Some(JsonValue::Bool(b))) => b.to_string(),
                    _ => String::new(),
                };
                let e = rows
                    .entry((id.to_string(), row))
                    .or_insert_with(|| (op.op_id.clone(), BTreeMap::new()));
                e.1.insert(col, shown);
            }
            for ((id, row), (first_op, cells)) in rows {
                let max_col = cells.keys().max().copied().unwrap_or(1);
                let line: Vec<String> = (1..=max_col)
                    .map(|c| cells.get(&c).cloned().unwrap_or_default())
                    .collect();
                let name = sheet_names.get(&id).cloned().unwrap_or(id.clone());
                out.push(DiffEntry {
                    op_id: first_op,
                    kind: "row.create".into(),
                    target_id: format!("row:{id}:{row}"),
                    location: format!("{name} · row {row}"),
                    before: None,
                    after: Some(line.join("  |  ")),
                });
            }
        }
        CreatedKind::Pptx => {
            let mut index: BTreeMap<String, i64> = BTreeMap::new();
            for op in batch
                .operations
                .iter()
                .filter(|o| o.kind == OpKind::SlideInsert)
            {
                if let (Some(id), Some(i)) = (
                    op.args.get("slide_id").and_then(|s| s.as_str()),
                    op.args.get("index").and_then(|i| i.as_int()),
                ) {
                    index.insert(id.to_string(), i);
                }
            }
            let mut slides: Vec<(i64, DiffEntry)> = Vec::new();
            for op in batch
                .operations
                .iter()
                .filter(|o| o.kind == OpKind::SlideUpdate)
            {
                let id = op
                    .args
                    .get("slide_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let mut lines = Vec::new();
                for el in op
                    .args
                    .get("elements")
                    .and_then(|e| e.as_array())
                    .unwrap_or_default()
                {
                    match el.get("kind").and_then(|k| k.as_str()) {
                        Some("title") => lines.push(
                            el.get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string(),
                        ),
                        Some("bullets") => {
                            for b in el
                                .get("items")
                                .and_then(|i| i.as_array())
                                .unwrap_or_default()
                            {
                                lines.push(format!("• {}", b.as_str().unwrap_or("")));
                            }
                        }
                        Some("notes") => lines.push(format!(
                            "Notes: {}",
                            el.get("text").and_then(|t| t.as_str()).unwrap_or("")
                        )),
                        Some("chart") => lines.push(format!(
                            "Chart: {}",
                            el.get("title").and_then(|t| t.as_str()).unwrap_or("")
                        )),
                        _ => {}
                    }
                }
                let i = index.get(id).copied().unwrap_or(0);
                slides.push((
                    i,
                    DiffEntry {
                        op_id: op.op_id.clone(),
                        kind: "slide.create".into(),
                        target_id: op.precondition.target_id.clone(),
                        location: format!("Slide {}", i + 1),
                        before: None,
                        after: Some(lines.join("\n")),
                    },
                ));
            }
            slides.sort_by_key(|(i, _)| *i);
            out.extend(slides.into_iter().map(|(_, d)| d));
        }
        CreatedKind::Docx => {
            let mut blocks: Vec<(i64, DiffEntry)> = Vec::new();
            for op in batch
                .operations
                .iter()
                .filter(|o| o.kind == OpKind::BlockInsert)
            {
                let i = op.args.get("index").and_then(|x| x.as_int()).unwrap_or(0);
                let text = op
                    .args
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                if text.is_empty() {
                    continue;
                }
                let style = op
                    .args
                    .get("style")
                    .and_then(|s| s.as_str())
                    .unwrap_or("paragraph");
                blocks.push((
                    i,
                    DiffEntry {
                        op_id: op.op_id.clone(),
                        kind: "block.create".into(),
                        target_id: op.precondition.target_id.clone(),
                        location: format!("¶ {} · {style}", i + 1),
                        before: None,
                        after: Some(text),
                    },
                ));
            }
            blocks.sort_by_key(|(i, _)| *i);
            out.extend(blocks.into_iter().map(|(_, d)| d));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Propose tools

fn proposal_output(
    batch: &ArtifactBatch,
    kind: CreatedKind,
    file_name: String,
    bytes: &[u8],
    preview: Value,
    checks: Value,
) -> Value {
    json!({
        "artifact_id": batch.artifact_id,
        "kind": kind.as_str(),
        "file_name": file_name,
        "base_content_hash": batch.base_content_hash,
        "batch": batch_json(batch),
        "canonical_args_hash": batch.canonical_hash(),
        "proposed_output_hash": harbor_canonical::sha256_hex(bytes),
        "proposed_output_bytes": bytes.len(),
        "op_count": batch.operations.len(),
        "preview": preview,
        "checks": checks,
    })
}

fn requested_name(args: &Value, title: &str, ext: &str, fallback: &str) -> String {
    let base = args
        .get("file_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.strip_suffix(&format!(".{ext}")).unwrap_or(s).to_string())
        .unwrap_or_else(|| title.to_string());
    file_name_for(&base, ext, fallback)
}

pub struct WorkbookBuild {
    spec: ToolSpec,
}

impl WorkbookBuild {
    pub fn new() -> Self {
        WorkbookBuild {
            spec: ToolSpec {
                id: "workbook.build".into(),
                description: "Propose a NEW workbook from a table spec (title, sheet, typed columns, rows, columns to total, computed columns). Every formula — totals, computed columns — is written by the tool, recalculated through the pinned engine and checked; the result is a creation batch over the empty base with the hash of the file it would write. Nothing is saved.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "spec": {"type": "object"},
                        "file_name": {"type": "string", "maxLength": 160},
                        "source": {"type": ["string", "null"], "maxLength": 2000000}
                    },
                    "required": ["spec"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Propose,
                requires: vec!["artifact.engine".into(), "formula.qualified".into()],
                timeout_ms: 60_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for WorkbookBuild {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for WorkbookBuild {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let source = args.get("source").and_then(Value::as_str).unwrap_or("");
        let (repaired, _) = repair_duplicated_figures(&args["spec"], source);
        let spec = WorkbookSpec::from_value(&repaired)
            .map_err(|p| ToolError::failed(&tool, p.join("; ")))?;
        let batch = workbook_batch(&spec);
        batch
            .validate()
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let bytes = render_creation(&batch).map_err(|e| ToolError::failed(&tool, e))?;
        // Read the rendered file back: the preview and the checks describe
        // the bytes that would be written, not the spec that asked for them.
        let doc = WorkbookDoc::load(&bytes).map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let data = doc
            .sheet(&spec.sheet)
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let headers = spec.headers();
        let shown = |c: u32, r: u32| -> Value {
            match data.cells.get(&(c, r)) {
                Some(cell) => match &cell.cached {
                    Some(v) => json!(cell_value_repr(v)),
                    None => json!(""),
                },
                None => json!(""),
            }
        };
        let total_row = spec.total_row();
        let last_row = total_row.unwrap_or(spec.rows.len() as u32 + 1);
        let mut rows = Vec::new();
        for r in 2..=last_row.min(41) {
            rows.push(Value::Array(
                (1..=headers.len() as u32).map(|c| shown(c, r)).collect(),
            ));
        }
        let mut errors = Vec::new();
        let mut formula_cells = 0usize;
        for ((c, r), cell) in &data.cells {
            if cell.formula.is_some() {
                formula_cells += 1;
            }
            if let Some(CellValue::Error(e)) = &cell.cached {
                errors.push(json!({"address": addr(*c, *r), "code": e.code()}));
            }
        }
        let totals: Vec<Value> = spec
            .totals
            .iter()
            .filter_map(|&t| {
                let r = total_row?;
                let c = t as u32 + 1;
                Some(json!({
                    "column": headers[t].0,
                    "address": addr(c, r),
                    "formula": data.cells.get(&(c, r)).and_then(|x| x.formula.clone()).map(|f| format!("={f}")),
                    "value": shown(c, r),
                }))
            })
            .collect();
        let preview = json!({
            "sheet": spec.sheet,
            "header": headers.iter().map(|(h, _)| h.clone()).collect::<Vec<_>>(),
            "rows": rows,
            "row_count": spec.rows.len(),
            "truncated": last_row > 41,
        });
        let checks = json!({
            "formula_cells": formula_cells,
            "errors": errors,
            "totals": totals,
            "engine": harbor_formula::engine::engine_identity().family,
            "package": "sound",
        });
        let name = requested_name(args, &spec.title, "xlsx", "Harbor workbook");
        Ok(proposal_output(
            &batch,
            CreatedKind::Xlsx,
            name,
            &bytes,
            preview,
            checks,
        ))
    }
}

pub struct DeckBuild {
    spec: ToolSpec,
}

impl DeckBuild {
    pub fn new() -> Self {
        DeckBuild {
            spec: ToolSpec {
                id: "deck.build".into(),
                description: "Propose a NEW slide deck from an outline (title, optional subtitle, slides with titles, bullets and notes; bullets may cite a source unit and quote). The tool lays out a cover slide and one content slide per outline slide, carries cited sources into the speaker notes, and returns a creation batch over the empty base with the hash of the file it would write. Nothing is saved.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "outline": {"type": "object"},
                        "file_name": {"type": "string", "maxLength": 160},
                        "unit_label": {"enum": ["page", "paragraph", "slide", "line"]},
                        "sentences": {"type": "object"}
                    },
                    "required": ["outline"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Propose,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 60_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for DeckBuild {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DeckBuild {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let (normalized, notes) = normalize_outline(&args["outline"]);
        let outline = DeckOutline::from_value(&normalized)
            .map_err(|p| ToolError::failed(&tool, p.join("; ")))?;
        let sentences = args.get("sentences").map(sentence_table);
        let unit = args
            .get("unit_label")
            .and_then(Value::as_str)
            .or_else(|| args.pointer("/sentences/unit").and_then(Value::as_str));
        let label = match unit {
            Some("paragraph") => "¶",
            Some("slide") => "Slide",
            Some("line") => "Line",
            _ => "p.",
        };
        let batch = deck_batch(&outline, label, sentences.as_ref());
        batch
            .validate()
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let bytes = render_creation(&batch).map_err(|e| ToolError::failed(&tool, e))?;
        let back = PptxDeck::from_pptx_bytes(&bytes)
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let preview = json!({
            "title": back.title,
            "slides": back.slides.iter().enumerate().map(|(i, s)| json!({
                "index": i + 1,
                "title": s.title,
                "bullets": s.bullets,
                "notes": s.notes,
            })).collect::<Vec<_>>(),
        });
        let checks = json!({
            "slide_count": back.slides.len(),
            "cited_bullets": outline.slides.iter().flat_map(|s| s.bullets.iter()).filter(|b| b.cite.is_some() || b.source.is_some()).count(),
            "normalized": notes,
            "package": "sound",
        });
        let name = requested_name(args, &outline.title, "pptx", "Harbor deck");
        Ok(proposal_output(
            &batch,
            CreatedKind::Pptx,
            name,
            &bytes,
            preview,
            checks,
        ))
    }
}

pub struct DocxBuild {
    spec: ToolSpec,
}

impl DocxBuild {
    pub fn new() -> Self {
        DocxBuild {
            spec: ToolSpec {
                id: "docx.build".into(),
                description: "Propose a NEW Word document from a draft (title and sections of paragraphs). Layout is decided by the tool: a headed document (title, one Heading 1 per section) or, with `letter`, a letter (sender and recipient lines, salutation, body, sign-off). Returns a creation batch over the empty base with the hash of the file it would write. Nothing is saved.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "document": {"type": "object"},
                        "file_name": {"type": "string", "maxLength": 160},
                        "layout": {"enum": ["headed", "letter"]},
                        "letter": {
                            "type": "object",
                            "properties": {
                                "date": {"type": ["string", "null"], "maxLength": 80},
                                "recipient": {"type": ["string", "null"], "maxLength": 400},
                                "sender": {"type": ["string", "null"], "maxLength": 400}
                            },
                            "additionalProperties": false
                        }
                    },
                    "required": ["document"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Propose,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 60_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for DocxBuild {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DocxBuild {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let spec = DocumentSpec::from_value(&args["document"])
            .map_err(|p| ToolError::failed(&tool, p.join("; ")))?;
        let letter = (args.get("layout").and_then(Value::as_str) == Some("letter")).then(|| {
            let part = |k: &str| {
                args.pointer(&format!("/letter/{k}"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            LetterParts {
                date: part("date"),
                recipient: part("recipient"),
                sender: part("sender"),
            }
        });
        let batch = document_batch(&spec, letter.as_ref());
        batch
            .validate()
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let bytes = render_creation(&batch).map_err(|e| ToolError::failed(&tool, e))?;
        let doc = harbor_artifacts::DocxDocument::load(&bytes)
            .map_err(|e| ToolError::failed(&tool, e.to_string()))?;
        let words: usize = doc
            .paragraphs
            .iter()
            .map(|p| p.text.split_whitespace().count())
            .sum();
        let preview = json!({
            "title": spec.title,
            "paragraphs": doc.paragraphs.iter().filter(|p| !p.text.is_empty()).map(|p| json!({
                "style": p.style,
                "text": p.text,
            })).collect::<Vec<_>>(),
        });
        let checks = json!({
            "paragraph_count": doc.paragraphs.len(),
            "word_count": words,
            "layout": if letter.is_some() { "letter" } else { "headed" },
            "package": "sound",
        });
        let name = requested_name(args, &spec.title, "docx", "Harbor document");
        Ok(proposal_output(
            &batch,
            CreatedKind::Docx,
            name,
            &bytes,
            preview,
            checks,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget_spec() -> Value {
        json!({
            "title": "Household budget",
            "sheet": "Budget",
            "columns": [
                {"header": "Item", "type": "text"},
                {"header": "Planned", "type": "currency"},
                {"header": "Actual", "type": "currency"}
            ],
            "rows": [["Rent", 1200, "1,200"], ["Groceries", "$400", 385.5], ["Transport", null, 60]],
            "total_columns": ["Planned", "Actual", "Remaining"],
            "computed": [{"header": "Remaining", "op": "subtract", "left": "Planned", "right": "Actual"}],
            "currency_symbol": "$"
        })
    }

    /// The iOS simulator run printed "null" on a cover slide and in the
    /// speaker notes: the model wrote the word, not JSON null.
    #[test]
    fn null_words_are_absent_values_not_text() {
        let outline = DeckOutline::from_value(&json!({
            "title": "Harbor 1.1", "subtitle": "null",
            "slides": [{"title": "Shipped", "bullets": ["Offline search", "N/A"], "notes": "None"}]
        }))
        .unwrap();
        assert_eq!(outline.subtitle, None);
        assert_eq!(outline.slides[0].notes, None);
        assert_eq!(outline.slides[0].bullets.len(), 1);
        assert!(matches!(
            cell_from_json(&json!("null"), ColumnType::Number),
            Ok(CellValue::Blank)
        ));
        assert!(!is_null_word("Nullable"));
    }

    /// "#,##0.##" rendered 1200 as "1,200." in Excel and as a blank cell
    /// in Apple's Files preview and Quick Look.
    #[test]
    fn plain_numbers_use_a_format_every_renderer_shows() {
        assert_eq!(
            number_format(ColumnType::Number, None).as_deref(),
            Some("#,##0.00")
        );
    }

    #[test]
    fn number_text_parses_like_a_reader() {
        assert_eq!(parse_number_text("1,200"), Some((1200.0, false)));
        assert_eq!(parse_number_text("$1,200.50"), Some((1200.5, false)));
        assert_eq!(parse_number_text("(300)"), Some((-300.0, false)));
        assert_eq!(parse_number_text("12%"), Some((12.0, true)));
        assert_eq!(parse_number_text("3.5k"), Some((3500.0, false)));
        assert_eq!(parse_number_text("SAR 450"), Some((450.0, false)));
        assert_eq!(parse_number_text("12,34"), None);
        assert_eq!(parse_number_text("abc"), None);
        assert_eq!(parse_number_text("1.2.3"), None);
    }

    #[test]
    fn workbook_batch_writes_the_formulas_and_renders_deterministically() {
        let spec = WorkbookSpec::from_value(&budget_spec()).unwrap();
        let batch = workbook_batch(&spec);
        batch.validate().unwrap();
        assert!(batch.is_creation());
        assert_eq!(creation_kind(&batch).unwrap(), CreatedKind::Xlsx);
        let formulas: Vec<String> = batch
            .operations
            .iter()
            .filter(|o| o.args.get("value_kind").and_then(|k| k.as_str()) == Some("formula"))
            .map(|o| o.args.get("value").unwrap().as_str().unwrap().to_string())
            .collect();
        assert!(formulas.contains(&"=B2-C2".to_string()), "{formulas:?}");
        assert!(
            formulas.contains(&"=SUM(B2:B4)".to_string()),
            "{formulas:?}"
        );
        assert!(
            formulas.contains(&"=SUM(D2:D4)".to_string()),
            "{formulas:?}"
        );
        let a = render_creation(&batch).unwrap();
        let b = render_creation(&workbook_batch(&spec)).unwrap();
        assert_eq!(a, b);
        let doc = WorkbookDoc::load(&a).unwrap();
        let sheet = doc.sheet("Budget").unwrap();
        // Totals are computed by the engine; blanks count as zero.
        assert_eq!(
            sheet.cells.get(&(2, 5)).unwrap().cached,
            Some(CellValue::Number(1600.0))
        );
        assert_eq!(
            sheet.cells.get(&(3, 5)).unwrap().cached,
            Some(CellValue::Number(1645.5))
        );
        assert_eq!(
            sheet.cells.get(&(1, 5)).unwrap().cached,
            Some(CellValue::Text("Total".into()))
        );
        let diff = creation_diff(&batch);
        assert_eq!(diff[0].location, "Title");
        assert!(diff
            .iter()
            .any(|d| d.after.as_deref() == Some("Rent  |  1200  |  1200  |  =B2-C2")));
    }

    #[test]
    fn workbook_spec_problems_are_specific() {
        let err = WorkbookSpec::from_value(&json!({
            "title": "",
            "sheet": "Q4/Budget",
            "columns": [{"header": "Item", "type": "text"}, {"header": "Amount", "type": "currency"}],
            "rows": [["Rent", "twelve"], ["Food"]],
            "total_columns": ["Amount"]
        }))
        .unwrap_err();
        let joined = err.join(" | ");
        assert!(joined.contains("needs a title"), "{joined}");
        assert!(joined.contains("contains '/'"), "{joined}");
        assert!(joined.contains("\"twelve\" is not a number"), "{joined}");
        assert!(
            joined.contains("row 2 has 1 cells for 2 columns"),
            "{joined}"
        );
    }

    #[test]
    fn structural_slips_are_repaired_and_reported_never_figures() {
        // What the pinned 1.5B model wrote for a budget: "Remaining" both
        // typed (with its own arithmetic) and computed.
        let spec = WorkbookSpec::from_value(&json!({
            "title": "October budget", "sheet": "Monthly Budget",
            "columns": [{"header": "Item", "type": "text"}, {"header": "Planned", "type": "number"},
                        {"header": "Spent", "type": "number"}, {"header": "Remaining", "type": "number"}],
            "rows": [["Rent", 1200, 1200, 0], ["Groceries", 400, 385, 15]],
            "total_columns": ["Planned", "Spent", "Remaining", "Item"],
            "computed": [{"header": "Remaining", "op": "subtract", "left": "Planned", "right": "Spent"}],
            "currency_symbol": "$"
        }))
        .unwrap();
        assert_eq!(spec.columns.len(), 3, "the typed Remaining column is gone");
        assert_eq!(spec.rows[1].len(), 3);
        assert_eq!(spec.computed.len(), 1);
        assert_eq!(
            spec.totals,
            vec![1, 2, 3],
            "Planned, Spent and the computed Remaining"
        );
        let notes = spec.notes.join(" | ");
        assert!(
            notes.contains("\"Remaining\" is computed by a formula"),
            "{notes}"
        );
        assert!(notes.contains("no total for \"Item\""), "{notes}");
        // And for a tracker: a repeated header row, template example rows
        // and a computed column over text columns.
        let tracker = WorkbookSpec::from_value(&json!({
            "title": "Job applications", "sheet": "Applications",
            "columns": [{"header": "Company", "type": "text"}, {"header": "Role", "type": "text"},
                        {"header": "Status", "type": "text"}, {"header": "Date Applied", "type": "date"}],
            "rows": [["Company", "Role", "Status", "Date Applied"],
                     ["Company1", "Role1", "Status1", "2023-01-01"],
                     ["Company 2", "Role 2", "Status2", "2023-02-01"],
                     ["Acme", "Analyst", "Applied", "12 May"]],
            "total_columns": ["Total"],
            "computed": [{"header": "Total", "op": "add", "left": "Status", "right": "Status"}],
            "currency_symbol": null
        }))
        .unwrap();
        assert_eq!(tracker.rows.len(), 1, "only the real row is kept");
        assert!(tracker.computed.is_empty() && tracker.totals.is_empty());
        let notes = tracker.notes.join(" | ");
        assert!(notes.contains("repeated the column headers"), "{notes}");
        assert!(notes.contains("2 example row(s)"), "{notes}");
        assert!(notes.contains("\"Total\" was dropped"), "{notes}");
    }

    #[test]
    fn letter_furniture_is_stripped_and_the_body_kept() {
        let p = "Dear Sir/Madam,\n\nWe want to renew the lease.\n\nThank you.\n\nYours sincerely,\n[Your Name]";
        assert_eq!(
            strip_letter_furniture(p),
            "We want to renew the lease.\n\nThank you."
        );
        assert_eq!(strip_letter_furniture("Best regards"), "");
        // A sign-off in the middle of a paragraph is prose, not furniture.
        let prose = "Regards,\nthe committee asked us to wait for the survey before we decide anything about it.";
        assert_eq!(strip_letter_furniture(prose), prose);
    }

    #[test]
    fn deck_and_document_batches_render_sound_packages() {
        let outline = DeckOutline::from_value(&json!({
            "title": "Q3 review",
            "subtitle": "Mobile team",
            "slides": [
                {"title": "Shipped", "bullets": ["Offline search", {"text": "Crash-free 99.4%", "ref": 2, "quote": "Crash-free sessions at 99.4%"}], "notes": "Keep it short"},
                {"title": "Next", "bullets": ["Cut 1.1.0-rc1"]}
            ]
        }))
        .unwrap();
        let batch = deck_batch(&outline, "p.", None);
        assert_eq!(creation_kind(&batch).unwrap(), CreatedKind::Pptx);
        let bytes = render_creation(&batch).unwrap();
        assert_eq!(
            bytes,
            render_creation(&deck_batch(&outline, "p.", None)).unwrap()
        );
        let deck = PptxDeck::from_pptx_bytes(&bytes).unwrap();
        assert_eq!(deck.slides.len(), 3);
        assert_eq!(deck.slides[0].title, "Q3 review");
        assert_eq!(deck.slides[0].bullets, vec!["Mobile team".to_string()]);
        let notes = deck.slides[1].notes.clone().unwrap();
        assert!(notes.contains("Keep it short"), "{notes}");
        assert!(
            notes.contains("p. 2: \u{201c}Crash-free sessions at 99.4%\u{201d}"),
            "{notes}"
        );
        let diff = creation_diff(&batch);
        assert!(diff
            .iter()
            .any(|d| d.location == "Slide 2"
                && d.after.as_deref().unwrap().contains("• Offline search")));

        let doc = DocumentSpec::from_value(&json!({
            "title": "Archive proposal",
            "sections": [
                {"heading": "Summary", "paragraphs": ["Move the archive.\n\nKeep two copies."]},
                {"heading": "Next steps", "paragraphs": ["- Approve\n- Schedule"]}
            ]
        }))
        .unwrap();
        let headed = render_creation(&document_batch(&doc, None)).unwrap();
        let read = harbor_artifacts::DocxDocument::load(&headed).unwrap();
        let styles: Vec<Option<&str>> =
            read.paragraphs.iter().map(|p| p.style.as_deref()).collect();
        assert_eq!(
            styles,
            vec![
                Some("Title"),
                Some("Heading1"),
                None,
                None,
                Some("Heading1"),
                Some("ListBullet"),
                Some("ListBullet")
            ]
        );
        let letter = render_creation(&document_batch(
            &doc,
            Some(&LetterParts {
                date: Some("26 September 2026".into()),
                recipient: Some("Amina Khan\nFinance".into()),
                sender: Some("Omar Haddad".into()),
            }),
        ))
        .unwrap();
        let read = harbor_artifacts::DocxDocument::load(&letter).unwrap();
        let texts: Vec<&str> = read.paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert!(texts.contains(&"Dear Amina Khan,"), "{texts:?}");
        assert_eq!(texts.last(), Some(&"Omar Haddad"));
        assert!(
            !texts.contains(&"Summary"),
            "letters carry no section headings"
        );
    }

    #[test]
    fn creation_batches_are_refused_when_they_are_not_creations() {
        let spec = WorkbookSpec::from_value(&budget_spec()).unwrap();
        let mut batch = workbook_batch(&spec);
        batch.operations[2].precondition.expected_content_hash = "a".repeat(64);
        assert!(render_creation(&batch)
            .unwrap_err()
            .contains("does not exist yet"));
        let mut mixed = workbook_batch(&spec);
        mixed.operations.push(Operation {
            op_id: "op-x".into(),
            kind: OpKind::BlockInsert,
            precondition: Precondition {
                target_id: "block:b1".into(),
                expected_content_hash: EMPTY_CONTENT_HASH.into(),
            },
            args: JsonValue::object([("block_id", js("b1"))]),
        });
        assert!(render_creation(&mixed).unwrap_err().contains("mixes"));
        let mut formula = workbook_batch(&spec);
        for op in formula.operations.iter_mut() {
            if op.args.get("value_kind").and_then(|k| k.as_str()) == Some("formula") {
                op.args = JsonValue::object([
                    ("sheet_id", js("s1")),
                    ("address", js("D2")),
                    ("value", js("=WEBSERVICE(\"https://x.example\")")),
                    ("value_kind", js("formula")),
                ]);
                break;
            }
        }
        assert!(render_creation(&formula).unwrap_err().contains("refused"));
        assert_eq!(
            file_name_for("Q4: Budget / plan", "xlsx", "x"),
            "Q4 Budget plan.xlsx"
        );
        assert_eq!(
            file_name_for("///", "docx", "Harbor document"),
            "Harbor document.docx"
        );
    }
}
