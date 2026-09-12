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

/// A workbook loaded for inspection/edit. Structure preservation is the
/// umya backend's contract; unsupported parts (pivot caches, external
/// links) are preserved byte-for-byte by the writer (PRESERVE_ONLY rows of
/// the Office Feature Matrix).
pub struct WorkbookDoc {
    book: umya_spreadsheet::Workbook,
    sheets: BTreeMap<String, SheetData>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkbookError {
    #[error("xlsx load failed: {0}")]
    Load(String),
    #[error("sheet not found: {0}")]
    SheetNotFound(String),
    #[error("cell ref invalid: {0}")]
    BadRef(String),
}

impl WorkbookDoc {
    pub fn load(bytes: &[u8]) -> Result<Self, WorkbookError> {
        let book = xlsx_reader::read_reader(&mut Cursor::new(bytes), true)
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
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
        Ok(WorkbookDoc { book, sheets })
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
        let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
        xlsx_writer::write_writer(&self.book, &mut buf)
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
        let inner = buf.into_inner().map_err(|e| WorkbookError::Load(e.to_string()))?;
        Ok(inner.into_inner())
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
