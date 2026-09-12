//! XLSX workbook document: read, typed cell operations, recalculation via
//! the pinned harbor_formula engine, and provenance-tagged values.

use std::collections::BTreeMap;
use std::io::Cursor;

use umya_spreadsheet::reader::xlsx as xlsx_reader;
use umya_spreadsheet::writer::xlsx as xlsx_writer;

use harbor_formula::value::{CellError, CellValue};

#[derive(Debug, Clone, Default)]
pub struct SheetData {
    pub name: String,
    /// (col, row) 1-based -> cell. Row/col follow Excel numbering.
    pub cells: BTreeMap<(u32, u32), SheetCell>,
}

#[derive(Debug, Clone)]
pub struct SheetCell {
    pub formula: Option<String>,
    pub cached: Option<CellValue>,
}

impl SheetData {
    /// Target identity for preconditions, e.g. `Sheet1!B3`.
    pub fn cell_target(&self, col: u32, row: u32) -> String {
        format!("{}!{}{}", self.name, col_letter(col), row)
    }
}

pub fn col_letter(mut col: u32) -> String {
    let mut out = Vec::new();
    while col > 0 {
        let rem = ((col - 1) % 26) as u8;
        out.push(b'A' + rem);
        col = (col - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

pub fn col_number(s: &str) -> u32 {
    let mut n = 0u32;
    for b in s.bytes() {
        n = n * 26 + (b.to_ascii_uppercase() - b'A') as u32 + 1;
    }
    n
}

/// What the save path preserved from the loaded package without
/// interpretation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreservationReport {
    /// Parts carried over verbatim from the loaded package (parts the edit
    /// backend does not model: pivot tables/caches, external links, VBA
    /// projects, slicers, connections).
    pub carried_parts: Vec<String>,
    /// Relationship entries restored so every carried part stays reachable
    /// from the package relationship graph.
    pub restored_relationships: Vec<String>,
}

/// A workbook loaded for inspection/edit. Structure preservation is the
/// umya backend's contract; unsupported parts (pivot caches, external
/// links, VBA) are carried over byte-for-byte by the writer (PRESERVE_ONLY
/// rows of the Office Feature Matrix) with their relationships and content
/// types restored so the package graph remains resolvable.
pub struct WorkbookDoc {
    book: umya_spreadsheet::Workbook,
    sheets: BTreeMap<String, SheetData>,
    /// Every entry of the loaded package (name -> bytes), the source for
    /// carry-over on save.
    original: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkbookError {
    #[error("xlsx load failed: {0}")]
    Load(String),
    #[error("zip: {0}")]
    BadZip(String),
    #[error("sheet not found: {0}")]
    SheetNotFound(String),
    #[error("cell ref invalid: {0}")]
    BadRef(String),
}

impl WorkbookDoc {
    pub fn load(bytes: &[u8]) -> Result<Self, WorkbookError> {
        let book = xlsx_reader::read_reader(&mut Cursor::new(bytes), true)
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        let mut sheets = BTreeMap::new();
        for name in book.get_sheet_collection().iter().map(|s| s.get_name().to_string()) {
            let mut data = SheetData { name: name.clone(), cells: BTreeMap::new() };
            if let Ok(sheet) = book.get_sheet_by_name(&name) {
                let (max_col, max_row) = sheet.get_highest_column_and_row();
                for row in 1..=max_row {
                    for col in 1..=max_col {
                        if let Some(cell) = sheet.get_cell((col, row)) {
                            let f: &str = cell.get_formula();
                            let formula = if f.is_empty() { None } else { Some(f.to_string()) };
                            let value = cell.get_value();
                            let cached = if value.is_empty() {
                                None
                            } else {
                                Some(string_to_value(&value))
                            };
                            if formula.is_some() || cached.is_some() {
                                data.cells.insert((col, row), SheetCell { formula, cached });
                            }
                        }
                    }
                }
            }
            sheets.insert(name, data);
        }
        let mut original = BTreeMap::new();
        {
            use std::io::Read as _;
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
                let name = entry.name().to_string();
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
                original.insert(name, buf);
            }
        }
        drop(archive);
        Ok(WorkbookDoc { book, sheets, original })
    }

    /// Parts of the loaded package the edit backend does not model (they
    /// will be carried over verbatim on save).
    pub fn unmodeled_parts(&self) -> Vec<String> {
        unmodeled_candidates(&self.original)
            .into_keys()
            .collect()
    }

    pub fn sheet(&self, name: &str) -> Result<&SheetData, WorkbookError> {
        self.sheets.get(name).ok_or_else(|| WorkbookError::SheetNotFound(name.into()))
    }

    pub fn sheet_names(&self) -> Vec<String> {
        self.sheets.keys().cloned().collect()
    }

    /// Apply a cell.set: value or formula. Returns the new displayed value
    /// after recalculation of that cell.
    pub fn set_cell(
        &mut self,
        sheet: &str,
        row: u32,
        col: u32,
        set: CellSet,
    ) -> Result<CellValue, WorkbookError> {
        let idx = self
            .book
            .get_sheet_collection()
            .iter()
            .position(|s| s.get_name() == sheet)
            .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
        let s = self.book.get_sheet_mut(&idx).map_err(|e| WorkbookError::Load(e.to_string()))?;
        match &set {
            CellSet::Formula(f) => {
                s.get_cell_mut((col, row)).set_formula(f);
            }
            CellSet::Value(v) => {
                let mut cell = s.get_cell_mut((col, row));
                match v {
                    CellValue::Blank => { cell.set_value(""); }
                    CellValue::Number(n) => { cell.set_value_number(*n); }
                    CellValue::Text(t) => { cell.set_value(t); }
                    CellValue::Bool(b) => { cell.set_value(if *b { "TRUE" } else { "FALSE" }); }
                    CellValue::Error(_) => return Err(WorkbookError::BadRef("error literal set".into())),
                }
            }
        }
        // Recalculate through the pinned engine on an in-memory copy.
        let bytes = self.to_bytes()?;
        let mut calc = harbor_formula::engine::HarborWorkbook::new();
        calc.add_sheet(sheet);
        // Rebuild the engine inputs from the loaded workbook for the target
        // sheet and its references: full workbook copy into the engine.
        for (name, data) in &self.sheets {
            if name != sheet {
                calc.add_sheet(name);
            }
            for ((c, r), cell) in &data.cells {
                if let Some(cv) = &cell.cached {
                    if cell.formula.is_none() {
                        calc.set_value(name, *r, *c, cv.clone());
                    }
                }
            }
        }
        // Re-apply formulas of other cells (they may be referenced).
        for (name, data) in &self.sheets {
            for ((c, r), cell) in &data.cells {
                if let Some(f) = &cell.formula {
                    if !(name == sheet && *c == col && *r == row) {
                        calc.set_formula(name, *r, *c, f);
                    }
                }
            }
        }
        match &set {
            CellSet::Formula(f) => calc.set_formula(sheet, row, col, f),
            CellSet::Value(v) => calc.set_value(sheet, row, col, v.clone()),
        }
        let computed = calc.evaluate_cell(sheet, row, col);
        // Persist the recalculated value as the cell's cached value so the
        // saved file is consistent for Excel and for Harbor re-reads
        // (save-reload fixture dimension). Only formula cells need this:
        // literal sets already carry their value.
        if matches!(set, CellSet::Formula(_)) {
            let keep_formula = set.clone();
            let idx = self
                .book
                .get_sheet_collection()
                .iter()
                .position(|s| s.get_name() == sheet)
                .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
            let cell_ref = self.book.get_sheet_mut(&idx).map_err(|e| WorkbookError::Load(e.to_string()))?;
            let cell = cell_ref.get_cell_mut((col, row));
            match &computed {
                CellValue::Blank => { cell.set_value(""); }
                CellValue::Number(n) => { cell.set_value(format!("{n}")); }
                CellValue::Text(t) => { cell.set_value(t); }
                CellValue::Bool(b) => { cell.set_value(if *b { "TRUE" } else { "FALSE" }); }
                CellValue::Error(e) => { cell.set_value(e.code()); }
            }
            // umya's set_value clears the formula; re-apply it so the cell
            // remains a formula cell with a cached value.
            if let CellSet::Formula(f) = &keep_formula {
                cell.set_formula(f);
            }
        }
        Ok(computed)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, WorkbookError> {
        Ok(self.write_package_with_report()?.0)
    }

    /// Serialize, carrying over unmodeled package parts byte-identically,
    /// and report what was preserved.
    pub fn write_package_with_report(
        &self,
    ) -> Result<(Vec<u8>, PreservationReport), WorkbookError> {
        let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
        xlsx_writer::write_writer(&self.book, &mut buf)
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
        let inner = buf.into_inner().map_err(|e| WorkbookError::Load(e.to_string()))?;
        let emitted = inner.into_inner();
        merge_carried_parts(emitted, &self.original)
    }

    pub fn sheets_snapshot(&self) -> &BTreeMap<String, SheetData> {
        &self.sheets
    }

    /// Recalculate EVERY formula cell through the pinned engine and persist
    /// the results as cached values (recalc + save-reload consistency).
    /// Returns (cell -> value) for all formula cells.
    pub fn recalculate_all(&mut self) -> Result<BTreeMap<(String, u32, u32), CellValue>, WorkbookError> {
        // Refresh the sheet snapshot from the live book first.
        self.refresh_snapshot()?;
        let bytes = self.to_bytes()?;
        let mut calc = harbor_formula::engine::HarborWorkbook::new();
        let fresh = WorkbookDoc::load(&bytes)?;
        for (name, data) in fresh.sheets_snapshot() {
            calc.add_sheet(name);
            for ((c, r), cell) in &data.cells {
                if let Some(cv) = &cell.cached {
                    if cell.formula.is_none() {
                        calc.set_value(name, *r, *c, cv.clone());
                    }
                }
            }
        }
        // Second pass: register all formulas (dependencies may point anywhere).
        let formula_cells: Vec<(String, u32, u32)> = fresh
            .sheets_snapshot()
            .iter()
            .flat_map(|(name, data)| {
                data.cells
                    .iter()
                    .filter(|(_, c)| c.formula.is_some())
                    .map(move |((c, r), _)| (name.clone(), *r, *c))
            })
            .collect();
        for (name, r, c) in &formula_cells {
            let f = fresh
                .sheets_snapshot()
                .get(name)
                .and_then(|d| d.cells.get(&(*c, *r)))
                .and_then(|c| c.formula.clone())
                .unwrap();
            calc.set_formula(name, *r, *c, &f);
        }
        // Evaluate all formula cells; persist cached values.
        let mut out = BTreeMap::new();
        for (name, r, c) in &formula_cells {
            let formula = fresh
                .sheets_snapshot()
                .get(name)
                .and_then(|d| d.cells.get(&(*c, *r)))
                .and_then(|c| c.formula.clone())
                .unwrap_or_default();
            let v = calc.evaluate_cell(name, *r, *c);
            let i = self
                .book
                .get_sheet_collection()
                .iter()
                .position(|s| s.get_name() == name)
                .ok_or_else(|| WorkbookError::SheetNotFound(name.into()))?;
            let sheet = self.book.get_sheet_mut(&i).map_err(|e| WorkbookError::Load(e.to_string()))?;
            let cell = sheet.get_cell_mut((*c, *r));
            match &v {
                CellValue::Blank => { cell.set_value(""); }
                CellValue::Number(n) => { cell.set_value(format!("{n}")); }
                CellValue::Text(t) => { cell.set_value(t); }
                CellValue::Bool(b) => { cell.set_value(if *b { "TRUE" } else { "FALSE" }); }
                CellValue::Error(e) => { cell.set_value(e.code()); }
            }
            if !formula.is_empty() {
                cell.set_formula(&formula);
            }
            out.insert((name.clone(), *r, *c), v);
        }
        self.refresh_snapshot()?;
        Ok(out)
    }

    /// Add a basic chart of `kind` over `series` ranges anchored at
    /// `from`..`to` on `sheet` (matrix rows 13/19: bar/column, line, pie
    /// and scatter basic series).
    pub fn add_chart(
        &mut self,
        kind: XlsxChartKind,
        sheet: &str,
        from: &str,
        to: &str,
        series: Vec<String>,
        title: &str,
    ) -> Result<(), WorkbookError> {
        let idx = self
            .book
            .get_sheet_collection()
            .iter()
            .position(|s| s.get_name() == sheet)
            .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
        let s = self.book.get_sheet_mut(&idx).map_err(|e| WorkbookError::Load(e.to_string()))?;
        use umya_spreadsheet::structs::{Chart, ChartType};
        use umya_spreadsheet::structs::drawing::spreadsheet::MarkerType;
        let mut from_marker = MarkerType::default();
        from_marker.set_coordinate(from);
        let mut to_marker = MarkerType::default();
        to_marker.set_coordinate(to);
        let series_refs: Vec<&str> = series.iter().map(|x| x.as_str()).collect();
        let chart_type = match kind {
            XlsxChartKind::Bar => ChartType::BarChart,
            XlsxChartKind::Line => ChartType::LineChart,
            XlsxChartKind::Pie => ChartType::PieChart,
            XlsxChartKind::Scatter => ChartType::ScatterChart,
        };
        let mut chart = Chart::default();
        chart.new_chart(&chart_type, from_marker, to_marker, series_refs);
        chart.set_title(title);
        s.add_chart(chart);
        Ok(())
    }

    /// Add a basic bar/column chart over `series` ranges anchored at
    /// `from`..`to` on `sheet` (matrix row 13: bar/column basic series).
    pub fn add_bar_chart(
        &mut self,
        sheet: &str,
        from: &str,
        to: &str,
        series: Vec<String>,
        title: &str,
    ) -> Result<(), WorkbookError> {
        self.add_chart(XlsxChartKind::Bar, sheet, from, to, series, title)
    }

    /// Count chart parts in raw xlsx bytes (round-trip conformance check).
    pub fn count_charts_in_bytes(bytes: &[u8]) -> Result<usize, WorkbookError> {
        let mut ar =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        let mut count = 0usize;
        for i in 0..ar.len() {
            let name = ar
                .by_index(i)
                .map_err(|e| WorkbookError::BadZip(e.to_string()))?
                .name()
                .to_string();
            if name.starts_with("xl/charts/chart") {
                count += 1;
            }
        }
        Ok(count)
    }

    fn refresh_snapshot(&mut self) -> Result<(), WorkbookError> {
        let mut sheets = BTreeMap::new();
        for name in self.book.get_sheet_collection().iter().map(|s| s.get_name().to_string()) {
            let mut data = SheetData { name: name.clone(), cells: BTreeMap::new() };
            if let Ok(sheet) = self.book.get_sheet_by_name(&name) {
                let (max_col, max_row) = sheet.get_highest_column_and_row();
                for row in 1..=max_row {
                    for col in 1..=max_col {
                        if let Some(cell) = sheet.get_cell((col, row)) {
                            let f: &str = cell.get_formula();
                            let formula = if f.is_empty() { None } else { Some(f.to_string()) };
                            let value = cell.get_value();
                            let cached = if value.is_empty() { None } else { Some(string_to_value(&value)) };
                            if formula.is_some() || cached.is_some() {
                                data.cells.insert((col, row), SheetCell { formula, cached });
                            }
                        }
                    }
                }
            }
            sheets.insert(name, data);
        }
        self.sheets = sheets;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum CellSet {
    Value(CellValue),
    Formula(String),
}

pub enum WorkbookOp {
    CellSet { sheet: String, row: u32, col: u32, set: CellSet },
}

fn string_to_value(s: &str) -> CellValue {
    match s {
        "TRUE" => return CellValue::Bool(true),
        "FALSE" => return CellValue::Bool(false),
        _ => {}
    }
    if let Some(code) = CellError::from_code(s) {
        return CellValue::Error(code);
    }
    if let Ok(n) = s.parse::<f64>() {
        return CellValue::Number(n);
    }
    CellValue::Text(s.to_string())
}

/// Basic chart kinds Harbor can create on a workbook (matrix row 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XlsxChartKind {
    Bar,
    Line,
    Pie,
    Scatter,
}

/// Package entry names the umya edit backend models (it rewrites them from
/// its object model). Anything else found in a loaded package is carried
/// over verbatim on save.
const MODELED_PREFIXES: &[&str] = &[
    "[Content_Types].xml",
    "_rels/",
    "docProps/",
    "xl/workbook.xml",
    "xl/_rels/",
    "xl/worksheets/",
    "xl/theme/",
    "xl/styles.xml",
    "xl/sharedStrings.xml",
    "xl/calcChain.xml",
    "xl/charts/",
    "xl/drawings/",
    "xl/media/",
    "xl/tables/",
];

fn unmodeled_candidates(original: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    original
        .iter()
        .filter(|(name, _)| {
            !MODELED_PREFIXES
                .iter()
                .any(|p| name.starts_with(p))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Merge original package parts that the edit backend did not emit back
/// into the saved package, restoring the relationship entries and content
/// type declarations needed to keep them resolvable (PRESERVE_ONLY matrix
/// rows: pivot tables/caches, external links, VBA projects, slicers).
fn merge_carried_parts(
    emitted: Vec<u8>,
    original: &BTreeMap<String, Vec<u8>>,
) -> Result<(Vec<u8>, PreservationReport), WorkbookError> {
    use std::collections::BTreeSet;
    use std::io::{Read as _, Write as _};

    let zip_err = |e: zip::result::ZipError| WorkbookError::BadZip(e.to_string());
    let mut out_archive = zip::ZipArchive::new(Cursor::new(emitted.as_slice())).map_err(zip_err)?;
    let mut out_names: BTreeSet<String> = BTreeSet::new();
    for i in 0..out_archive.len() {
        out_names.insert(out_archive.by_index(i).map_err(zip_err)?.name().to_string());
    }
    // Every part present in the final package: what the backend emitted
    // plus everything the loaded package contained (carried verbatim).
    let final_parts: BTreeSet<&str> = out_names
        .iter()
        .map(|s| s.as_str())
        .chain(original.keys().map(|k| k.as_str()))
        .collect();
    // ---- Restore relationship entries pointing at carried parts. ----
    let mut restored: Vec<String> = Vec::new();
    // rels file -> (Id, target part) entries to restore.
    let mut restorations: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for (rels_name, rels_bytes) in original.iter().filter(|(n, _)| n.contains("_rels/")) {
        let text = match std::str::from_utf8(rels_bytes) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let doc = match roxmltree::Document::parse(text) {
            Ok(d) => d,
            Err(_) => continue,
        };
        // Source directory the targets resolve against: strip the trailing
        // "_rels/<file>.rels" from the rels entry name.
        let source_dir = match rels_name.rsplit_once("_rels/") {
            Some((dir, _)) => dir.trim_end_matches('/').to_string(),
            None => continue,
        };
        for rel in doc.descendants().filter(|n| n.tag_name().name() == "Relationship") {
            if rel.attribute("TargetMode") == Some("External") {
                continue;
            }
            let (Some(id), Some(target)) = (rel.attribute("Id"), rel.attribute("Target")) else {
                continue;
            };
            let resolved = normalize_package_path(&source_dir, target);
            if !final_parts.contains(resolved.as_str()) {
                continue;
            }
            // Skip when the emitted package already declares this Id in an
            // existing rels file (Id collision means umya manages it).
            if let Ok(mut f) = out_archive.by_name(rels_name) {
                let mut existing = String::new();
                if f.read_to_string(&mut existing).is_ok()
                    && existing.contains(&format!("Id=\"{id}\""))
                {
                    continue;
                }
            }
            let rtype = rel.attribute("Type").unwrap_or("").to_string();
            restorations
                .entry(rels_name.clone())
                .or_default()
                .push((id.to_string(), rtype, target.to_string()));
        }
    }
    for (rels_name, entries) in &restorations {
        restored.extend(
            entries
                .iter()
                .map(|(id, _, target)| format!("{rels_name}#{id}->{target}")),
        );
    }

    let carried: Vec<(String, Vec<u8>)> = original
        .iter()
        .filter(|(name, _)| {
            !out_names.contains(*name) && !restorations.contains_key(*name)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if carried.is_empty() && restorations.is_empty() {
        return Ok((emitted, PreservationReport::default()));
    }
    let carried_names: BTreeSet<&str> = carried.iter().map(|(n, _)| n.as_str()).collect();
    let _ = &carried_names;

    // ---- Patch [Content_Types].xml for carried parts. ----
    let mut ct = {
        let mut f = out_archive
            .by_name("[Content_Types].xml")
            .map_err(zip_err)?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        s
    };
    if let Some(orig_ct) = original.get("[Content_Types].xml") {
        let orig_text = std::str::from_utf8(orig_ct).unwrap_or_default();
        for part in carried_names.iter() {
            let part_name = format!("/{}", part);
            let override_tag_prefix = format!("<Override PartName=\"{part_name}\" ");
            if ct.contains(&override_tag_prefix) {
                continue;
            }
            if let Some(line_start) = orig_text.find(&override_tag_prefix) {
                let line_end = orig_text[line_start..].find("/>").map(|e| e + 2);
                if let Some(end_off) = line_end {
                    let tag = &orig_text[line_start..line_start + end_off];
                    ct = ct.replace("</Types>", &format!("{tag}</Types>"));
                }
            } else if let Some(ext) = part.rsplit_once('.').map(|(_, e)| e.to_lowercase()) {
                // No Override: the part relies on a Default extension
                // declaration. Ensure it exists in the output too.
                let default_prefix = format!("<Default Extension=\"{ext}\" ");
                if !ct.contains(&default_prefix) {
                    if let Some(start) = orig_text.find(&default_prefix) {
                        if let Some(end_off) = orig_text[start..].find("/>").map(|e| e + 2) {
                            let tag = &orig_text[start..start + end_off];
                            ct = ct.replace("</Types>", &format!("{tag}</Types>"));
                        }
                    }
                }
            }
        }
        // Macro-enabled workbooks: carrying vbaProject.bin requires the
        // workbook part to keep its original (macroEnabled) content type.
        if carried_names.contains("xl/vbaProject.bin") {
            if let Some(orig_wb) = override_content_type(orig_text, "/xl/workbook.xml") {
                ct = replace_override_content_type(&ct, "/xl/workbook.xml", &orig_wb);
            }
        }
    }

    // ---- Rewrite the package. ----
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..out_archive.len() {
        let mut f = out_archive.by_index(i).map_err(zip_err)?;
        let name = f.name().to_string();
        writer.start_file(name.clone(), opts).map_err(zip_err)?;
        if name == "[Content_Types].xml" {
            writer
                .write_all(ct.as_bytes())
                .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        } else if restorations.contains_key(&name) {
            let mut existing = String::new();
            f.read_to_string(&mut existing)
                .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
            let mut merged = existing.clone();
            if let Some(entries) = restorations.get(&name) {
                for (id, rtype, target) in entries {
                    let tag = format!(
                        "<Relationship Id=\"{id}\" Type=\"{rtype}\" Target=\"{target}\"/>"
                    );
                    if !merged.contains(&format!("Id=\"{id}\"")) {
                        merged = merged.replace("</Relationships>", &format!("{tag}</Relationships>"));
                    }
                }
            }
            writer
                .write_all(merged.as_bytes())
                .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        } else {
            std::io::copy(&mut f, &mut writer).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        }
    }
    // Relationship files that existed only in the original package (the
    // emitted output had no rels for that part at all).
    for (rels_name, entries) in &restorations {
        if out_names.contains(rels_name) {
            continue;
        }
        let mut merged = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for (id, rtype, target) in entries {
            merged.push_str(&format!(
                "<Relationship Id=\"{id}\" Type=\"{rtype}\" Target=\"{target}\"/>"
            ));
        }
        merged.push_str("</Relationships>");
        writer
            .start_file(rels_name.clone(), opts)
            .map_err(zip_err)?;
        writer
            .write_all(merged.as_bytes())
            .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
    }
    for (name, bytes) in &carried {
        writer.start_file(name.clone(), opts).map_err(zip_err)?;
        writer.write_all(bytes).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
    }
    let report = PreservationReport {
        carried_parts: carried.iter().map(|(n, _)| n.clone()).collect(),
        restored_relationships: restored,
    };
    let final_bytes = writer.finish().map_err(zip_err)?.into_inner();
    Ok((final_bytes, report))
}

fn normalize_package_path(source_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if source_dir.is_empty() {
        Vec::new()
    } else {
        source_dir.split('/').collect()
    };
    for seg in target.split('/') {
        match seg {
            "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Extract the ContentType attribute of the Override for `part_name`.
fn override_content_type(content_types_xml: &str, part_name: &str) -> Option<String> {
    let needle = format!("<Override PartName=\"{part_name}\" ");
    let start = content_types_xml.find(&needle)?;
    let rest = &content_types_xml[start..];
    let ct_key = "ContentType=\"";
    let ct_off = rest.find(ct_key)? + ct_key.len();
    let end = rest[ct_off..].find('"')?;
    Some(rest[ct_off..ct_off + end].to_string())
}

/// Replace (or add) the ContentType of the Override for `part_name`.
fn replace_override_content_type(xml: &str, part_name: &str, new_ct: &str) -> String {
    let needle = format!("<Override PartName=\"{part_name}\" ");
    let Some(start) = xml.find(&needle) else {
        return xml.to_string();
    };
    let rest = &xml[start..];
    let Some(tag_end_off) = rest.find("/>").map(|e| e + 2) else {
        return xml.to_string();
    };
    let tag = &rest[..tag_end_off];
    let rebuilt = match tag.find("ContentType=\"") {
        Some(ct_off) => {
            let after = &tag[ct_off + "ContentType=\"".len()..];
            let close = after.find('"').unwrap_or(0);
            format!(
                "{}{}{}",
                &tag[..ct_off + "ContentType=\"".len()],
                new_ct,
                &after[close..]
            )
        }
        None => tag.to_string(),
    };
    format!("{}{}{}", &xml[..start], rebuilt, &xml[start + tag_end_off..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_workbook() -> Vec<u8> {
        let mut wb = harbor_formula::engine::HarborWorkbook::new();
        wb.set_value("Sheet1", 1, 1, CellValue::Number(10.0));
        wb.set_value("Sheet1", 2, 1, CellValue::Number(20.0));
        wb.set_formula("Sheet1", 3, 1, "=A1+A2");
        wb.to_xlsx_bytes()
    }

    #[test]
    fn load_edit_save_roundtrip() {
        let bytes = roundtrip_workbook();
        let mut doc = WorkbookDoc::load(&bytes).unwrap();
        assert_eq!(doc.sheet_names(), vec!["Sheet1".to_string()]);
        // The engine's export carries no cached values: a loaded formula
        // cell must NOT present a cached value (never trust caches that do
        // not exist).
        let sheet = doc.sheet("Sheet1").unwrap();
        let cached = sheet.cells.get(&(1, 3)).unwrap();
        assert!(cached.formula.is_some());
        assert_eq!(cached.cached, None);
        // Edit an input: the set cell reports its own value.
        let v = doc
            .set_cell("Sheet1", 2, 1, CellSet::Value(CellValue::Number(25.0)))
            .unwrap();
        assert_eq!(v, CellValue::Number(25.0));
        // Full recalculation through the pinned engine updates every
        // formula cell's cached value.
        let recalc = doc.recalculate_all().unwrap();
        assert_eq!(
            recalc.get(&("Sheet1".to_string(), 3, 1)),
            Some(&CellValue::Number(35.0))
        );
        // Save and reload: recalculated cached values persist.
        let out = doc.to_bytes().unwrap();
        let reloaded = WorkbookDoc::load(&out).unwrap();
        let cell = reloaded.sheet("Sheet1").unwrap().cells.get(&(1, 3)).unwrap().clone();
        assert_eq!(cell.cached, Some(CellValue::Number(35.0)));
    }

    #[test]
    fn col_letter_number_roundtrip() {
        assert_eq!(col_letter(1), "A");
        assert_eq!(col_letter(26), "Z");
        assert_eq!(col_letter(27), "AA");
        assert_eq!(col_letter(52), "AZ");
        assert_eq!(col_number("AZ"), 52);
    }
}
