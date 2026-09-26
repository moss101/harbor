//! Table cleanup (decision 0008): everything mechanical about cleaning a
//! table is decided here, in code — which cells have stray whitespace,
//! which numbers are stored as text, which spellings of a category are the
//! same value — and the model's only job, when the user gave an
//! instruction, is to pick which classes of fix to apply and which columns
//! to leave alone. Formula cells are never touched; duplicate and blank
//! rows are reported for review, because the batch contract has no row
//! deletion and a guessed deletion is not a cleanup.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use harbor_artifacts::WorkbookDoc;
use harbor_formula::value::CellValue;

use super::builtin::{addr, cell_content_hash, cell_target_id, detect_kind, ArtifactKind};
use super::{RiskClass, Tool, ToolContext, ToolError, ToolSpec};

const MB: usize = 1024 * 1024;

/// The fix classes a cleanup can apply, in the order they compose.
pub const FIX_CLASSES: &[&str] = &["whitespace", "number_as_text", "case_variants"];

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fold(s: &str) -> String {
    collapse(s).to_lowercase()
}

/// A plain number stored as text: optional sign, digits with or without
/// well-formed thousands separators, optional decimals. Currency symbols
/// and percent signs are NOT converted — dropping them would change what
/// the cell displays, since a cleanup cannot set number formats.
pub fn plain_number(s: &str) -> Option<f64> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^-?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?$").expect("plain number regex")
    });
    let t = s.trim();
    if !re.is_match(t) {
        return None;
    }
    t.replace(',', "").parse::<f64>().ok()
}

fn looks_numeric(s: &str) -> bool {
    plain_number(s).is_some() || super::create::parse_number_text(s).is_some()
}

/// Apply the selected fix classes to one cell's text, in the fixed order
/// whitespace → number_as_text → case_variants. Returns the new text and
/// whether the result is a number.
pub fn transform(
    before: &str,
    classes: &BTreeSet<String>,
    case_map: Option<&BTreeMap<String, String>>,
) -> (String, bool) {
    let mut v = before.to_string();
    if classes.contains("whitespace") {
        v = collapse(&v);
    }
    if classes.contains("number_as_text") {
        if let Some(n) = plain_number(&v) {
            let shown = if n.fract() == 0.0 && n.abs() < 9.0e15 {
                format!("{}", n as i64)
            } else {
                format!("{n}")
            };
            return (shown, true);
        }
    }
    if classes.contains("case_variants") {
        if let Some(canonical) = case_map.and_then(|m| m.get(&fold(&v))) {
            v = if classes.contains("whitespace") {
                canonical.clone()
            } else {
                // Keep the cell's own spacing when whitespace is not being
                // fixed: only the letters' case changes.
                if collapse(&v) == v {
                    canonical.clone()
                } else {
                    v
                }
            };
        }
    }
    (v, false)
}

struct Column {
    col: u32,
    header: String,
    numeric: bool,
    non_empty: usize,
    distinct: usize,
}

pub struct TableInspect {
    spec: ToolSpec,
}

impl TableInspect {
    pub fn new() -> Self {
        TableInspect {
            spec: ToolSpec {
                id: "table.inspect".into(),
                description: "Inspect a table in an attached workbook for mechanical cleanup: header row, column kinds, and every cell with stray whitespace, a plain number stored as text, or a category spelled several ways (with the canonical spelling). Duplicate rows, blank rows inside the table and unparseable entries in numeric columns are listed for review. Formula cells are never candidates. Nothing is changed.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "artifact_id": {"type": "string", "minLength": 1, "maxLength": 128},
                        "sheet": {"type": ["string", "null"], "maxLength": 31},
                        "max_findings": {"type": "integer", "minimum": 1, "maximum": 5000}
                    },
                    "required": ["artifact_id"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec!["artifact.engine".into()],
                timeout_ms: 60_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for TableInspect {
    fn default() -> Self {
        Self::new()
    }
}

fn text_of(v: &CellValue) -> Option<String> {
    match v {
        CellValue::Text(t) => Some(t.clone()),
        _ => None,
    }
}

impl Tool for TableInspect {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let tool = self.spec.id.clone();
        let id = args
            .get("artifact_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let a = ctx.artifacts.get(&id).ok_or_else(|| {
            ToolError::Unavailable(
                tool.clone(),
                format!("artifact {id} is not attached to this run"),
            )
        })?;
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
        let snapshot = wb.sheets_snapshot();
        let requested = args
            .get("sheet")
            .and_then(Value::as_str)
            .map(str::to_string);
        let sheet = match requested {
            Some(name) => {
                if !snapshot.contains_key(&name) {
                    return Err(ToolError::failed(
                        &tool,
                        format!(
                            "sheet {name:?} is not in this workbook (sheets: {})",
                            wb.sheet_names().join(", ")
                        ),
                    ));
                }
                name
            }
            None => snapshot
                .iter()
                .max_by_key(|(n, d)| (d.cells.len(), std::cmp::Reverse((*n).clone())))
                .map(|(n, _)| n.clone())
                .ok_or_else(|| ToolError::failed(&tool, "the workbook has no sheets"))?,
        };
        let data = &snapshot[&sheet];
        ctx.check_alive(&tool)?;
        let rows: BTreeSet<u32> = data.cells.keys().map(|(_, r)| *r).collect();
        let cols: BTreeSet<u32> = data.cells.keys().map(|(c, _)| *c).collect();
        let (Some(&first), Some(&last)) = (rows.iter().next(), rows.iter().last()) else {
            return Ok(json!({
                "artifact_id": id, "sheet": sheet, "content_hash": harbor_canonical::sha256_hex(&a.bytes),
                "fixes": [], "review": [], "issue_count": 0, "counts": {}, "case_maps": {},
                "columns": [], "header_row": null, "summary": "The sheet is empty.",
                "fixable_classes": FIX_CLASSES,
            }));
        };
        // What the cell holds as the user sees it: a numeric string stored
        // as text reads back as a number, so the stored type decides.
        let value_at = |c: u32, r: u32| {
            data.cells
                .get(&(c, r))
                .and_then(|x| match (&x.cached, x.stored_as_text) {
                    (Some(CellValue::Number(n)), true) => Some(CellValue::Text(
                        super::builtin::cell_value_repr(&CellValue::Number(*n)),
                    )),
                    (other, _) => other.clone(),
                })
        };
        let formula_at = |c: u32, r: u32| {
            data.cells
                .get(&(c, r))
                .map(|x| x.formula.is_some())
                .unwrap_or(false)
        };
        // Header: the first non-empty row when every value in it is text
        // and there is data below it.
        let header_is_text = cols.iter().all(|c| match value_at(*c, first) {
            None | Some(CellValue::Blank) => true,
            Some(CellValue::Text(_)) => !formula_at(*c, first),
            _ => false,
        });
        let header_row = (header_is_text && last > first).then_some(first);
        let data_from = header_row.map(|h| h + 1).unwrap_or(first);
        // Column profiles.
        let mut columns = Vec::new();
        for &c in &cols {
            let header = header_row
                .and_then(|h| value_at(c, h))
                .and_then(|v| text_of(&v))
                .map(|t| collapse(&t))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| format!("Column {}", harbor_artifacts::workbook::col_letter(c)));
            let mut non_empty = 0usize;
            let mut numeric = 0usize;
            let mut distinct = BTreeSet::new();
            for r in data_from..=last {
                match value_at(c, r) {
                    None | Some(CellValue::Blank) => {}
                    Some(CellValue::Number(_)) => {
                        non_empty += 1;
                        numeric += 1;
                    }
                    Some(CellValue::Text(t)) => {
                        if t.trim().is_empty() {
                            continue;
                        }
                        non_empty += 1;
                        if looks_numeric(&t) {
                            numeric += 1;
                        }
                        distinct.insert(fold(&t));
                    }
                    Some(_) => non_empty += 1,
                }
            }
            columns.push(Column {
                col: c,
                header,
                numeric: non_empty > 0 && numeric * 10 >= non_empty * 6,
                non_empty,
                distinct: distinct.len(),
            });
        }
        // Canonical spellings per text column: the most frequent spelling
        // of each folded value (first seen wins a tie).
        let mut case_maps: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for col in columns.iter().filter(|c| !c.numeric) {
            let mut groups: BTreeMap<String, Vec<(String, usize, u32)>> = BTreeMap::new();
            for r in data_from..=last {
                if formula_at(col.col, r) {
                    continue;
                }
                if let Some(CellValue::Text(t)) = value_at(col.col, r) {
                    let spelled = collapse(&t);
                    if spelled.is_empty() {
                        continue;
                    }
                    let g = groups.entry(fold(&t)).or_default();
                    match g.iter_mut().find(|(s, _, _)| *s == spelled) {
                        Some(e) => e.1 += 1,
                        None => g.push((spelled, 1, r)),
                    }
                }
            }
            let mut map = BTreeMap::new();
            for (key, spellings) in groups {
                if spellings.len() < 2 {
                    continue;
                }
                let canonical = spellings
                    .iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)))
                    .map(|s| s.0.clone())
                    .unwrap_or_default();
                map.insert(key, canonical);
            }
            if !map.is_empty() {
                case_maps.insert(harbor_artifacts::workbook::col_letter(col.col), map);
            }
        }
        // Per-cell fixes.
        let mut fixes = Vec::new();
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        let mut review = Vec::new();
        let all: BTreeSet<String> = FIX_CLASSES.iter().map(|s| s.to_string()).collect();
        for col in &columns {
            let letter = harbor_artifacts::workbook::col_letter(col.col);
            let map = case_maps.get(&letter);
            for r in data_from..=last {
                ctx.check_alive(&tool)?;
                if formula_at(col.col, r) {
                    continue;
                }
                let Some(CellValue::Text(t)) = value_at(col.col, r) else {
                    continue;
                };
                if t.trim().is_empty() {
                    continue;
                }
                let mut classes = Vec::new();
                if collapse(&t) != t {
                    classes.push("whitespace");
                }
                if col.numeric && plain_number(&t).is_some() {
                    classes.push("number_as_text");
                } else if col.numeric && !looks_numeric(&t) {
                    review.push(json!({
                        "class": "unparseable_numbers", "address": addr(col.col, r),
                        "column_header": col.header, "value": t,
                        "detail": format!("{:?} in numeric column {:?} is not a number", t, col.header),
                    }));
                    *counts.entry("unparseable_numbers").or_default() += 1;
                } else if col.numeric {
                    review.push(json!({
                        "class": "number_with_symbol", "address": addr(col.col, r),
                        "column_header": col.header, "value": t,
                        "detail": format!("{:?} is a number stored as text with a symbol; converting it would drop the symbol", t),
                    }));
                    *counts.entry("number_with_symbol").or_default() += 1;
                }
                if !col.numeric {
                    if let Some(canonical) = map.and_then(|m| m.get(&fold(&t))) {
                        if *canonical != collapse(&t) {
                            classes.push("case_variants");
                        }
                    }
                }
                if classes.is_empty() {
                    continue;
                }
                for c in &classes {
                    *counts.entry(c).or_default() += 1;
                }
                if fixes.len() >= max_findings {
                    continue;
                }
                let address = addr(col.col, r);
                let cell = data.cells.get(&(col.col, r));
                let (after, number) = transform(&t, &all, map);
                fixes.push(json!({
                    "sheet": sheet,
                    "address": address,
                    "target_id": cell_target_id(&sheet, &address),
                    "content_hash": cell_content_hash(
                        cell.and_then(|x| x.formula.as_deref()),
                        cell.and_then(|x| x.cached.as_ref()),
                    ),
                    "column": letter,
                    "column_header": col.header,
                    "before": t,
                    "after": after,
                    "number": number,
                    "classes": classes,
                }));
            }
        }
        // Row-level review: blank rows inside the table, duplicate rows.
        let mut seen_rows: BTreeMap<Vec<String>, u32> = BTreeMap::new();
        for r in data_from..=last {
            let key: Vec<String> = cols
                .iter()
                .map(|c| match value_at(*c, r) {
                    Some(CellValue::Text(t)) => fold(&t),
                    Some(CellValue::Number(n)) => format!("{n}"),
                    Some(CellValue::Bool(b)) => b.to_string(),
                    _ => String::new(),
                })
                .collect();
            if key.iter().all(String::is_empty) {
                review.push(json!({"class": "empty_rows", "row": r, "detail": format!("row {r} is blank inside the table")}));
                *counts.entry("empty_rows").or_default() += 1;
                continue;
            }
            if let Some(prev) = seen_rows.get(&key) {
                review.push(json!({"class": "duplicate_rows", "rows": [prev, r], "detail": format!("row {r} repeats row {prev}")}));
                *counts.entry("duplicate_rows").or_default() += 1;
            } else {
                seen_rows.insert(key, r);
            }
        }
        let issue_count: u64 = counts.values().sum();
        let summary = if issue_count == 0 {
            format!("No cleanup needed on sheet {sheet:?}: no stray whitespace, no numbers stored as text, no category spelled two ways, no blank or duplicate rows.")
        } else {
            format!(
                "Sheet {sheet:?}: {} whitespace, {} number-as-text and {} spelling-variant fix(es) available; {} duplicate row(s), {} blank row(s) and {} unparseable or symbol-bearing number(s) left for review.",
                counts.get("whitespace").unwrap_or(&0),
                counts.get("number_as_text").unwrap_or(&0),
                counts.get("case_variants").unwrap_or(&0),
                counts.get("duplicate_rows").unwrap_or(&0),
                counts.get("empty_rows").unwrap_or(&0),
                counts.get("unparseable_numbers").unwrap_or(&0) + counts.get("number_with_symbol").unwrap_or(&0),
            )
        };
        Ok(json!({
            "artifact_id": id,
            "content_hash": harbor_canonical::sha256_hex(&a.bytes),
            "sheet": sheet,
            "sheets": wb.sheet_names(),
            "header_row": header_row,
            "first_data_row": data_from,
            "last_row": last,
            "columns": columns.iter().map(|c| json!({
                "column": harbor_artifacts::workbook::col_letter(c.col),
                "header": c.header,
                "kind": if c.numeric { "number" } else { "text" },
                "non_empty": c.non_empty,
                "distinct": c.distinct,
            })).collect::<Vec<_>>(),
            "headers": columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(),
            "fixes": fixes,
            "fix_count": fixes.len(),
            "case_maps": case_maps,
            "review": review,
            "counts": counts,
            "issue_count": issue_count,
            "fixable_classes": FIX_CLASSES,
            "summary": summary,
        }))
    }
}

pub struct TableBuildCleanup {
    spec: ToolSpec,
}

impl TableBuildCleanup {
    pub fn new() -> Self {
        TableBuildCleanup {
            spec: ToolSpec {
                id: "table.build_cleanup".into(),
                description: "Turn a table.inspect result and a choice of fix classes (and columns to skip) into cell.set operations whose preconditions are copied from the inspection. Each cell's new value is recomputed from its original text with only the chosen classes applied. Produces operations for artifact.propose_batch; writes nothing.".into(),
                args_schema: json!({
                    "type": "object",
                    "properties": {
                        "inspection": {"type": "object"},
                        "fix": {"type": "array", "maxItems": 3, "items": {"enum": FIX_CLASSES}},
                        "skip_columns": {"type": "array", "maxItems": 50, "items": {"type": "string", "maxLength": 120}}
                    },
                    "required": ["inspection", "fix"],
                    "additionalProperties": false
                }),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 10_000,
                max_output_bytes: 8 * MB,
            },
        }
    }
}

impl Default for TableBuildCleanup {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for TableBuildCleanup {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
        let inspection = &args["inspection"];
        let chosen: BTreeSet<String> = args["fix"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let skip: BTreeSet<String> = args
            .get("skip_columns")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(fold).collect())
            .unwrap_or_default();
        let case_maps: BTreeMap<String, BTreeMap<String, String>> =
            serde_json::from_value(inspection.get("case_maps").cloned().unwrap_or(json!({})))
                .unwrap_or_default();
        let mut operations = Vec::new();
        let mut applied: BTreeMap<String, u64> = BTreeMap::new();
        let mut skipped_columns: BTreeSet<String> = BTreeSet::new();
        for f in inspection
            .get("fixes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let header = f.get("column_header").and_then(Value::as_str).unwrap_or("");
            let letter = f.get("column").and_then(Value::as_str).unwrap_or("");
            if skip.contains(&fold(header)) || skip.contains(&fold(letter)) {
                skipped_columns.insert(header.to_string());
                continue;
            }
            let classes: BTreeSet<String> = f
                .get("classes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let selected: BTreeSet<String> = classes.intersection(&chosen).cloned().collect();
            if selected.is_empty() {
                continue;
            }
            let before = f.get("before").and_then(Value::as_str).unwrap_or("");
            let (after, number) = transform(before, &selected, case_maps.get(letter));
            // "15" stored as text becomes the number 15: same characters, a
            // different cell. Only an unchanged TEXT result is a no-op.
            if after == before && !number {
                continue;
            }
            for c in &selected {
                *applied.entry(c.clone()).or_default() += 1;
            }
            let reason = selected.iter().cloned().collect::<Vec<_>>().join(", ");
            operations.push(json!({
                "kind": "cell.set",
                "target_id": f["target_id"],
                "expected_content_hash": f["content_hash"],
                "args": {
                    "sheet_id": f["sheet"],
                    "address": f["address"],
                    "value": after,
                    "value_kind": if number { "number_decimal" } else { "text" },
                },
                "reason": reason,
            }));
        }
        Ok(json!({
            "operations": operations,
            "change_count": operations.len(),
            "applied": applied,
            "skipped_columns": skipped_columns.into_iter().collect::<Vec<_>>(),
            "review": inspection.get("review").cloned().unwrap_or(json!([])),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{MemoryArtifacts, ToolRegistry};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn transforms_compose_in_a_fixed_order() {
        let all: BTreeSet<String> = FIX_CLASSES.iter().map(|s| s.to_string()).collect();
        let map: BTreeMap<String, String> = [("north".to_string(), "North".to_string())].into();
        assert_eq!(
            transform("  north ", &all, Some(&map)),
            ("North".into(), false)
        );
        assert_eq!(transform(" 1,200 ", &all, None), ("1200".into(), true));
        let ws: BTreeSet<String> = ["whitespace".to_string()].into();
        assert_eq!(
            transform("  north ", &ws, Some(&map)),
            ("north".into(), false)
        );
        assert_eq!(plain_number("$1,200"), None, "symbols are left for review");
        assert_eq!(plain_number("12,34"), None);
        assert_eq!(plain_number("-40.5"), Some(-40.5));
    }

    #[test]
    fn inspect_and_build_on_a_messy_table() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(root.join("fixtures/office/messy_table.xlsx")).unwrap();
        let artifacts = MemoryArtifacts::new().with("t", "messy_table.xlsx", bytes);
        let cancel = AtomicBool::new(false);
        let host = json!({});
        let ctx = ToolContext::new(&artifacts, &host, &cancel);
        let r = ToolRegistry::builtin();
        let allow: BTreeSet<String> = [
            "table.inspect".to_string(),
            "table.build_cleanup".to_string(),
        ]
        .into();
        let inspection = r
            .call(&ctx, "table.inspect", &json!({"artifact_id": "t"}), &allow)
            .unwrap()
            .output;
        assert_eq!(inspection["header_row"], 1, "{inspection}");
        let counts = &inspection["counts"];
        assert_eq!(counts["whitespace"], 2, "{counts}");
        assert_eq!(counts["number_as_text"], 2, "{counts}");
        assert_eq!(counts["case_variants"], 2, "{counts}");
        assert_eq!(counts["duplicate_rows"], 1, "{counts}");
        assert_eq!(counts["empty_rows"], 1, "{counts}");
        assert_eq!(counts["number_with_symbol"], 1, "{counts}");
        let all = r
            .call(
                &ctx,
                "table.build_cleanup",
                &json!({"inspection": inspection, "fix": FIX_CLASSES}),
                &allow,
            )
            .unwrap()
            .output;
        let ops = all["operations"].as_array().unwrap();
        assert!(ops
            .iter()
            .any(|o| o["args"]["value"] == "North" && o["args"]["value_kind"] == "text"));
        assert!(ops
            .iter()
            .any(|o| o["args"]["value"] == "1200" && o["args"]["value_kind"] == "number_decimal"));
        // A digit string stored as text converts even though its characters
        // do not change.
        assert!(ops.iter().any(|o| o["args"]["address"] == "C3"
            && o["args"]["value"] == "15"
            && o["args"]["value_kind"] == "number_decimal"));
        assert_eq!(ops.len(), 5, "{all}");
        // Skipping the Region column leaves its cells alone.
        let skipped = r
            .call(
                &ctx,
                "table.build_cleanup",
                &json!({"inspection": inspection, "fix": ["whitespace"], "skip_columns": ["region"]}),
                &allow,
            )
            .unwrap()
            .output;
        assert!(skipped["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| !o["args"]["address"].as_str().unwrap().starts_with('A')));
        assert_eq!(skipped["skipped_columns"], json!(["Region"]));
    }
}
