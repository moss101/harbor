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
    /// The package stores this literal as a string. `cached` reads a
    /// numeric-looking string as a number (what a formula sees), so this
    /// is the only way to tell a number stored as text from a number.
    pub stored_as_text: bool,
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

/// The spreadsheet grid's limits: XFD and 1048576.
pub const MAX_COL: u32 = 16_384;
pub const MAX_ROW: u32 = 1_048_576;

/// Column letters to a 1-based index, saturating rather than overflowing.
///
/// `n = n * 26 + ...` overflowed on a long run of letters. In debug that
/// is a panic; in release, where overflow wraps, it is worse — the
/// address silently resolves to a DIFFERENT cell than the one named, on
/// a commit path whose whole job is to touch exactly what was approved.
/// Found by the `batch_from_value` fuzz target on the address
/// "Dxxxxxxxxxxxxxxxxxx2" (crash-92b3025e).
///
/// A non-letter byte also used to underflow `b - b'A'`. Both now saturate
/// past `MAX_COL`, which is out of range by construction, so callers
/// reject it through their existing bounds check instead of trusting an
/// arithmetic accident.
pub fn col_number(s: &str) -> u32 {
    let mut n = 0u32;
    for b in s.bytes() {
        if !b.is_ascii_alphabetic() {
            return u32::MAX;
        }
        n = n
            .saturating_mul(26)
            .saturating_add((b.to_ascii_uppercase() - b'A') as u32 + 1);
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
    /// Conditional formats authored through Harbor's own serializer
    /// (umya 3.1's cfRule operand is address-typed; Harbor never guesses
    /// XML, so these are injected at strict CT_Worksheet/CT_Stylesheet
    /// element positions after the umya write).
    pending_cf: Vec<PendingCf>,
}

/// One authored conditional format rule (cellIs, single numeric operand,
/// highlight fill). `dxf_id` is assigned at serialization time.
struct PendingCf {
    sheet: String,
    sqref: String,
    operator: &'static str,
    operand: f64,
    fill_rgb: String,
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
        // The upstream reader panics on some malformed packages (a corrupt
        // deflate stream inside the zip, found by fuzzing) instead of
        // returning an error. A user-chosen file must never unwind through
        // the tool layer or the executor: every entry is inflated once
        // here first so a corrupt stream is a typed Load error, and the
        // reader call is contained as a second line of defence.
        {
            let mut probe = zip::ZipArchive::new(Cursor::new(bytes))
                .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
            crate::inflate_probe(&mut probe)
                .map_err(|e| WorkbookError::Load(format!("malformed workbook: {e}")))?;
        }
        let book =
            std::panic::catch_unwind(|| xlsx_reader::read_reader(&mut Cursor::new(bytes), true))
                .map_err(|payload| {
                    let reason = payload
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "reader panicked".into());
                    WorkbookError::Load(format!("malformed workbook: {reason}"))
                })?
                .map_err(|e| WorkbookError::Load(e.to_string()))?;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        let mut sheets = BTreeMap::new();
        for name in book.sheet_collection().iter().map(|s| s.name().to_string()) {
            let mut data = SheetData {
                name: name.clone(),
                cells: BTreeMap::new(),
            };
            if let Ok(sheet) = book.sheet_by_name(&name) {
                let (max_col, max_row) = sheet.highest_column_and_row();
                for row in 1..=max_row {
                    for col in 1..=max_col {
                        if let Some(cell) = sheet.cell((col, row)) {
                            let f: &str = cell.formula();
                            let formula = if f.is_empty() {
                                None
                            } else {
                                Some(f.to_string())
                            };
                            let value = cell.value();
                            let cached = if value.is_empty() {
                                None
                            } else {
                                Some(string_to_value(&value))
                            };
                            let stored_as_text =
                                formula.is_none() && cell.cell_value().data_type() == "s";
                            if formula.is_some() || cached.is_some() {
                                data.cells.insert(
                                    (col, row),
                                    SheetCell {
                                        formula,
                                        cached,
                                        stored_as_text,
                                    },
                                );
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
        Ok(WorkbookDoc {
            book,
            sheets,
            original,
            pending_cf: Vec::new(),
        })
    }

    /// A workbook with no sheets and no loaded package: the empty base a
    /// creation batch builds on. Document properties keep the backend's
    /// fixed defaults (no clock is read), so the same batch always
    /// serializes to the same bytes — the commit path re-derives the
    /// approved output hash from the batch alone.
    pub fn new_empty() -> Self {
        WorkbookDoc {
            book: umya_spreadsheet::new_file_empty_worksheet(),
            sheets: BTreeMap::new(),
            original: BTreeMap::new(),
            pending_cf: Vec::new(),
        }
    }

    /// Add an empty sheet. The first sheet added becomes the active one.
    pub fn add_sheet(&mut self, name: &str) -> Result<(), WorkbookError> {
        if self.sheets.contains_key(name) {
            return Err(WorkbookError::BadRef(format!(
                "sheet {name} already exists"
            )));
        }
        let first = self.book.sheet_collection().is_empty();
        let ws = self
            .book
            .new_sheet(name)
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
        let mut view = umya_spreadsheet::structs::SheetView::default();
        view.set_workbook_view_id(0);
        if first {
            view.set_tab_selected(true);
        }
        let mut views = umya_spreadsheet::structs::SheetViews::default();
        views.add_sheet_view_list_mut(view);
        ws.set_sheets_views(views);
        ws.set_active_cell("A1");
        if first {
            self.book.set_active_sheet(0);
        }
        self.sheets.insert(
            name.to_string(),
            SheetData {
                name: name.to_string(),
                cells: BTreeMap::new(),
            },
        );
        Ok(())
    }

    fn worksheet_mut(
        &mut self,
        sheet: &str,
    ) -> Result<&mut umya_spreadsheet::Worksheet, WorkbookError> {
        let idx = self
            .book
            .sheet_collection()
            .iter()
            .position(|s| s.name() == sheet)
            .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
        self.book
            .sheet_mut(idx)
            .map_err(|e| WorkbookError::Load(e.to_string()))
    }

    /// Write a value or formula WITHOUT recalculating. Creation writes a
    /// whole sheet this way and then calls [`recalculate_all`] once; the
    /// per-cell recalculation of [`set_cell`] would make that quadratic.
    /// Text is stored as text (never re-typed from its spelling).
    ///
    /// [`recalculate_all`]: WorkbookDoc::recalculate_all
    /// [`set_cell`]: WorkbookDoc::set_cell
    pub fn put_cell(
        &mut self,
        sheet: &str,
        row: u32,
        col: u32,
        set: CellSet,
    ) -> Result<(), WorkbookError> {
        if col == 0 || col > MAX_COL || row == 0 || row > MAX_ROW {
            return Err(WorkbookError::BadRef(format!("{col},{row}")));
        }
        let ws = self.worksheet_mut(sheet)?;
        let cell = ws.cell_mut((col, row));
        let snapshot = match &set {
            CellSet::Formula(f) => {
                cell.set_formula(f.trim_start_matches('='));
                SheetCell {
                    formula: Some(f.trim_start_matches('=').to_string()),
                    cached: None,
                    stored_as_text: false,
                }
            }
            CellSet::Value(v) => {
                match v {
                    CellValue::Blank => {
                        cell.set_value_string("");
                    }
                    CellValue::Number(n) => {
                        cell.set_value_number(*n);
                    }
                    CellValue::Text(t) => {
                        cell.set_value_string(t.clone());
                    }
                    CellValue::Bool(b) => {
                        cell.set_value_bool(*b);
                    }
                    CellValue::Error(_) => {
                        return Err(WorkbookError::BadRef("error literal set".into()))
                    }
                }
                SheetCell {
                    formula: None,
                    cached: (!matches!(v, CellValue::Blank)).then(|| v.clone()),
                    stored_as_text: matches!(v, CellValue::Text(_)),
                }
            }
        };
        let data = self
            .sheets
            .get_mut(sheet)
            .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
        if snapshot.formula.is_some() || snapshot.cached.is_some() {
            data.cells.insert((col, row), snapshot);
        } else {
            data.cells.remove(&(col, row));
        }
        Ok(())
    }

    /// Bold the cells `col_from..=col_to` of `row` (header and total rows).
    pub fn set_bold(
        &mut self,
        sheet: &str,
        row: u32,
        col_from: u32,
        col_to: u32,
    ) -> Result<(), WorkbookError> {
        let ws = self.worksheet_mut(sheet)?;
        for col in col_from..=col_to {
            ws.style_mut((col, row)).font_mut().set_bold(true);
        }
        Ok(())
    }

    /// Apply a number format code to `col`, rows `row_from..=row_to`.
    pub fn set_number_format(
        &mut self,
        sheet: &str,
        col: u32,
        row_from: u32,
        row_to: u32,
        code: &str,
    ) -> Result<(), WorkbookError> {
        let ws = self.worksheet_mut(sheet)?;
        for row in row_from..=row_to {
            ws.style_mut((col, row))
                .number_format_mut()
                .set_format_code(code);
        }
        Ok(())
    }

    pub fn set_column_width(
        &mut self,
        sheet: &str,
        col: u32,
        width: f64,
    ) -> Result<(), WorkbookError> {
        let ws = self.worksheet_mut(sheet)?;
        ws.column_dimension_mut(&col_letter(col)).set_width(width);
        Ok(())
    }

    /// Keep the first row visible while scrolling (a header row).
    pub fn freeze_first_row(&mut self, sheet: &str) -> Result<(), WorkbookError> {
        use umya_spreadsheet::structs::{Pane, PaneStateValues, PaneValues};
        let ws = self.worksheet_mut(sheet)?;
        let mut pane = Pane::default();
        pane.set_vertical_split(1.0);
        pane.top_left_cell_mut().set_coordinate("A2");
        pane.set_active_pane(PaneValues::BottomLeft);
        pane.set_state(PaneStateValues::Frozen);
        if let Some(view) = ws.sheet_views_mut().sheet_view_list_mut().first_mut() {
            view.set_pane(pane);
        }
        Ok(())
    }

    /// Document title (docProps/core.xml).
    pub fn set_title(&mut self, title: &str) {
        let props = self.book.properties_mut();
        props.set_title(title);
        props.set_creator("Harbor");
        props.set_last_modified_by("Harbor");
    }

    /// Parts of the loaded package the edit backend does not model (they
    /// will be carried over verbatim on save).
    pub fn unmodeled_parts(&self) -> Vec<String> {
        unmodeled_candidates(&self.original).into_keys().collect()
    }

    pub fn sheet(&self, name: &str) -> Result<&SheetData, WorkbookError> {
        self.sheets
            .get(name)
            .ok_or_else(|| WorkbookError::SheetNotFound(name.into()))
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
            .sheet_collection()
            .iter()
            .position(|s| s.name() == sheet)
            .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
        let s = self
            .book
            .sheet_mut(idx)
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
        match &set {
            CellSet::Formula(f) => {
                s.cell_mut((col, row)).set_formula(f);
            }
            CellSet::Value(v) => {
                let cell = s.cell_mut((col, row));
                match v {
                    CellValue::Blank => {
                        cell.set_value("");
                    }
                    CellValue::Number(n) => {
                        cell.set_value_number(*n);
                    }
                    CellValue::Text(t) => {
                        cell.set_value(t);
                    }
                    CellValue::Bool(b) => {
                        cell.set_value(if *b { "TRUE" } else { "FALSE" });
                    }
                    CellValue::Error(_) => {
                        return Err(WorkbookError::BadRef("error literal set".into()))
                    }
                }
            }
        }
        // Recalculate through the pinned engine on an in-memory copy.
        let _bytes = self.to_bytes()?;
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
                .sheet_collection()
                .iter()
                .position(|s| s.name() == sheet)
                .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
            let cell_ref = self
                .book
                .sheet_mut(idx)
                .map_err(|e| WorkbookError::Load(e.to_string()))?;
            let cell = cell_ref.cell_mut((col, row));
            match &computed {
                CellValue::Blank => {
                    cell.set_value("");
                }
                CellValue::Number(n) => {
                    cell.set_value(format!("{n}"));
                }
                CellValue::Text(t) => {
                    cell.set_value(t);
                }
                CellValue::Bool(b) => {
                    cell.set_value(if *b { "TRUE" } else { "FALSE" });
                }
                CellValue::Error(e) => {
                    cell.set_value(e.code());
                }
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
        let inner = buf
            .into_inner()
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
        let emitted = inner.into_inner();
        let (merged, report) = merge_carried_parts(emitted, &self.original)?;
        if self.pending_cf.is_empty() {
            return Ok((merged, report));
        }
        // Conditional-format injection preserves the upstream report.
        let (bytes, _) = inject_conditional_formats(merged, &self.pending_cf)?;
        Ok((bytes, report))
    }

    /// Author a conditional highlight (cellIs comparison against one
    /// numeric operand with a solid fill). Harbor's serializer emits the
    /// cfRule + differential format at schema-valid positions; existing
    /// CF parts from the loaded package are carried over untouched.
    pub fn add_conditional_format(
        &mut self,
        sheet: &str,
        sqref: &str,
        operator: CfOperator,
        operand: f64,
        fill_rgb: &str,
    ) -> Result<(), WorkbookError> {
        if !self.sheets.contains_key(sheet) {
            return Err(WorkbookError::SheetNotFound(sheet.into()));
        }
        if !valid_sqref(sqref) {
            return Err(WorkbookError::BadRef(sqref.into()));
        }
        let rgb = fill_rgb.trim_start_matches('#').to_ascii_uppercase();
        if rgb.len() != 6 || !rgb.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(WorkbookError::BadRef(format!(
                "fill color must be 6 hex digits: {fill_rgb}"
            )));
        }
        self.pending_cf.push(PendingCf {
            sheet: sheet.to_string(),
            sqref: sqref.to_string(),
            operator: operator.as_str(),
            operand,
            fill_rgb: rgb,
        });
        Ok(())
    }

    pub fn sheets_snapshot(&self) -> &BTreeMap<String, SheetData> {
        &self.sheets
    }

    /// Recalculate EVERY formula cell through the pinned engine and persist
    /// the results as cached values (recalc + save-reload consistency).
    /// Returns (cell -> value) for all formula cells.
    pub fn recalculate_all(
        &mut self,
    ) -> Result<BTreeMap<(String, u32, u32), CellValue>, WorkbookError> {
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
                .sheet_collection()
                .iter()
                .position(|s| s.name() == name)
                .ok_or_else(|| WorkbookError::SheetNotFound(name.into()))?;
            let sheet = self
                .book
                .sheet_mut(i)
                .map_err(|e| WorkbookError::Load(e.to_string()))?;
            let cell = sheet.cell_mut((*c, *r));
            match &v {
                CellValue::Blank => {
                    cell.set_value("");
                }
                CellValue::Number(n) => {
                    cell.set_value(format!("{n}"));
                }
                CellValue::Text(t) => {
                    cell.set_value(t);
                }
                CellValue::Bool(b) => {
                    cell.set_value(if *b { "TRUE" } else { "FALSE" });
                }
                CellValue::Error(e) => {
                    cell.set_value(e.code());
                }
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
            .sheet_collection()
            .iter()
            .position(|s| s.name() == sheet)
            .ok_or_else(|| WorkbookError::SheetNotFound(sheet.into()))?;
        let s = self
            .book
            .sheet_mut(idx)
            .map_err(|e| WorkbookError::Load(e.to_string()))?;
        use umya_spreadsheet::structs::drawing::spreadsheet::MarkerType;
        use umya_spreadsheet::structs::{Chart, ChartType};
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
        let mut ar = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
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
        for name in self
            .book
            .sheet_collection()
            .iter()
            .map(|s| s.name().to_string())
        {
            let mut data = SheetData {
                name: name.clone(),
                cells: BTreeMap::new(),
            };
            if let Ok(sheet) = self.book.sheet_by_name(&name) {
                let (max_col, max_row) = sheet.highest_column_and_row();
                for row in 1..=max_row {
                    for col in 1..=max_col {
                        if let Some(cell) = sheet.cell((col, row)) {
                            let f: &str = cell.formula();
                            let formula = if f.is_empty() {
                                None
                            } else {
                                Some(f.to_string())
                            };
                            let value = cell.value();
                            let cached = if value.is_empty() {
                                None
                            } else {
                                Some(string_to_value(&value))
                            };
                            let stored_as_text =
                                formula.is_none() && cell.cell_value().data_type() == "s";
                            if formula.is_some() || cached.is_some() {
                                data.cells.insert(
                                    (col, row),
                                    SheetCell {
                                        formula,
                                        cached,
                                        stored_as_text,
                                    },
                                );
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
    CellSet {
        sheet: String,
        row: u32,
        col: u32,
        set: CellSet,
    },
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
        .filter(|(name, _)| !MODELED_PREFIXES.iter().any(|p| name.starts_with(p)))
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
        for rel in doc
            .descendants()
            .filter(|n| n.tag_name().name() == "Relationship")
        {
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
            restorations.entry(rels_name.clone()).or_default().push((
                id.to_string(),
                rtype,
                target.to_string(),
            ));
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
        .filter(|(name, _)| !out_names.contains(*name) && !restorations.contains_key(*name))
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
        f.read_to_string(&mut s)
            .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
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
                    let tag =
                        format!("<Relationship Id=\"{id}\" Type=\"{rtype}\" Target=\"{target}\"/>");
                    if !merged.contains(&format!("Id=\"{id}\"")) {
                        merged =
                            merged.replace("</Relationships>", &format!("{tag}</Relationships>"));
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
        writer
            .write_all(bytes)
            .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
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
    format!(
        "{}{}{}",
        &xml[..start],
        rebuilt,
        &xml[start + tag_end_off..]
    )
}

/// Authored cellIs operators (single numeric operand).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfOperator {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

impl CfOperator {
    pub fn as_str(&self) -> &'static str {
        match self {
            CfOperator::GreaterThan => "greaterThan",
            CfOperator::LessThan => "lessThan",
            CfOperator::Equal => "equal",
            CfOperator::NotEqual => "notEqual",
            CfOperator::GreaterThanOrEqual => "greaterThanOrEqual",
            CfOperator::LessThanOrEqual => "lessThanOrEqual",
        }
    }

    pub fn parse(s: &str) -> Option<CfOperator> {
        Some(match s {
            "greater_than" | "greaterThan" => CfOperator::GreaterThan,
            "less_than" | "lessThan" => CfOperator::LessThan,
            "equal" => CfOperator::Equal,
            "not_equal" | "notEqual" => CfOperator::NotEqual,
            "greater_than_or_equal" | "greaterThanOrEqual" => CfOperator::GreaterThanOrEqual,
            "less_than_or_equal" | "lessThanOrEqual" => CfOperator::LessThanOrEqual,
            _ => return None,
        })
    }
}

/// A1-style range ("B2:B21" or single cell "B2"), no sheet prefix — the
/// sheet comes from the rule's own worksheet part.
fn valid_sqref(s: &str) -> bool {
    fn cell(part: &str) -> bool {
        let digits = part.chars().take_while(|c| c.is_ascii_alphabetic()).count();
        let letters_ok = digits >= 1 && digits <= 3
            && part[..digits].chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_lowercase());
        let num = &part[digits..];
        letters_ok && !num.is_empty() && num.len() <= 7 && num.chars().all(|c| c.is_ascii_digit())
    }
    match s.split_once(':') {
        Some((a, b)) => cell(a) && cell(b),
        None => cell(s),
    }
}

/// Harbor's conditional-format serializer: rewrites the umya-emitted
/// package inserting (1) `<dxfs>` into styles.xml at its CT_Stylesheet
/// position (before tableStyles/colors/extLst) and (2) each
/// `<conditionalFormatting>` into its sheet part at the CT_Worksheet
/// position (before pageMargins/pageSetup, after mergeCells-level
/// elements). dxfId values continue from any dxfs the loaded package
/// already carried.
fn inject_conditional_formats(
    bytes: Vec<u8>,
    pending: &[PendingCf],
) -> Result<(Vec<u8>, PreservationReport), WorkbookError> {
    use std::io::{Read, Write};
    // Sheet name -> part path via workbook.xml + rels (umya's writer
    // names sheets sheet1.xml.. in order; resolve robustly instead of
    // assuming: map through xl/workbook.xml order).
    let mut ar = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..ar.len() {
        let mut e = ar.by_index(i).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        let name = e.name().to_string();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        entries.push((name, buf));
    }

    // Existing dxfs count (loaded packages may carry one).
    let existing_dxfs = entries
        .iter()
        .find(|(n, _)| n == "xl/styles.xml")
        .map(|(_, b)| count_dxfs(&String::from_utf8_lossy(b)))
        .unwrap_or(0);

    let mut styles_dxf_xml = String::new();
    let mut next_dxf = existing_dxfs;
    let mut by_sheet: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for cf in pending {
        let fill = &cf.fill_rgb;
        styles_dxf_xml.push_str(&format!(
            "<dxf><fill><patternFill><bgColor rgb=\"FF{fill}\"/></patternFill></fill></dxf>"
        ));
        // Excel writes numbers in canonical form; operand text is plain.
        let operand = format_operand(cf.operand);
        by_sheet.entry(cf.sheet.clone()).or_default().push(format!(
            "<conditionalFormatting sqref=\"{}\"><cfRule type=\"cellIs\" dxfId=\"{}\" priority=\"{}\" operator=\"{}\"><formula>{}</formula></cfRule></conditionalFormatting>",
            cf.sqref, next_dxf, next_dxf + 1, cf.operator, operand
        ));
        next_dxf += 1;
    }

    for (name, buf) in &mut entries {
        if name == "xl/styles.xml" {
            let xml = String::from_utf8_lossy(buf).to_string();
            let injected = if existing_dxfs > 0 {
                // Grow the existing dxfs container's count and items.
                grow_dxfs(&xml, &styles_dxf_xml)
            } else {
                let block = format!("<dxfs count=\"{}\">{}</dxfs>", next_dxf, styles_dxf_xml);
                insert_before_styles_anchor(&xml, &block)
            };
            *buf = injected.into_bytes();
        } else if let Some(idx) = name.strip_prefix("xl/worksheets/sheet").and_then(|r| r.strip_suffix(".xml")) {
            let _ = idx;
            // Only sheets with pending rules are touched; resolve names
            // from the part's own content is not possible (name is in
            // workbook.xml), so match below via order map.
        }
    }
    // Resolve sheet order -> part names from workbook.xml.
    let wb_xml = entries
        .iter()
        .find(|(n, _)| n == "xl/workbook.xml")
        .map(|(_, b)| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();
    let sheet_names: Vec<String> = wb_xml
        .split("<sheet ")
        .skip(1)
        .filter_map(|seg| {
            let seg = seg.split("/>").next().unwrap_or("");
            seg.split("name=\"").nth(1).map(|rest| rest.split('\"').next().unwrap_or("").to_string())
        })
        .collect();
    for (sheet, blocks) in &by_sheet {
        let Some(pos) = sheet_names.iter().position(|s| s == sheet) else {
            return Err(WorkbookError::SheetNotFound(sheet.clone()));
        };
        let part = format!("xl/worksheets/sheet{}.xml", pos + 1);
        for (name, buf) in &mut entries {
            if *name == part {
                let xml = String::from_utf8_lossy(buf).to_string();
                let all = blocks.join("");
                let injected = insert_before_sheet_anchor(&xml, &all);
                *buf = injected.into_bytes();
            }
        }
    }

    let mut out = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut out);
        let opts: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
        for (name, buf) in &entries {
            zw.start_file(name.as_str(), opts)
                .map_err(|e| WorkbookError::BadZip(e.to_string()))?;
            zw.write_all(buf).map_err(|e| WorkbookError::BadZip(e.to_string()))?;
        }
        zw.finish().map_err(|e| WorkbookError::BadZip(e.to_string()))?;
    }
    Ok((out.into_inner(), PreservationReport::default()))
}

fn format_operand(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn count_dxfs(styles_xml: &str) -> usize {
    styles_xml
        .split("<dxfs count=\"")
        .nth(1)
        .and_then(|rest| rest.split('\"').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn grow_dxfs(styles_xml: &str, items: &str) -> String {
    // Re-emit the dxfs container with the existing items plus the new
    // ones (count continues from what the loaded package carried).
    let open = match styles_xml.find("<dxfs ") {
        Some(i) => i,
        None => return styles_xml.to_string(),
    };
    let close = match styles_xml.find("</dxfs>") {
        Some(i) => i + "</dxfs>".len(),
        None => return styles_xml.to_string(),
    };
    let inner_start = styles_xml[open..]
        .find('>')
        .map(|i| open + i + 1)
        .unwrap_or(open);
    let inner_end = close - "</dxfs>".len();
    let inner = &styles_xml[inner_start..inner_end];
    let count = count_dxfs(styles_xml) + items.matches("<dxf>").count();
    format!(
        "{}<dxfs count=\"{}\">{inner}{items}</dxfs>{}",
        &styles_xml[..open],
        count,
        &styles_xml[close..]
    )
}

fn insert_before_styles_anchor(xml: &str, block: &str) -> String {
    for anchor in ["<tableStyles", "<colors", "<extLst"] {
        if let Some(i) = xml.find(anchor) {
            return format!("{}{}{}", &xml[..i], block, &xml[i..]);
        }
    }
    // No later elements: insert before the closing tag.
    let i = xml.len() - "</styleSheet>".len();
    format!("{}{}{}", &xml[..i], block, &xml[i..])
}

fn insert_before_sheet_anchor(xml: &str, block: &str) -> String {
    for anchor in ["<pageMargins", "<pageSetup", "<headerFooter", "<rowBreaks", "<colBreaks", "<drawing"] {
        if let Some(i) = xml.find(anchor) {
            return format!("{}{}{}", &xml[..i], block, &xml[i..]);
        }
    }
    format!("{}{}</worksheet>", &xml[..xml.len() - "</worksheet>".len()], block)
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

    /// Inject a real conditionalFormatting part into the sheet XML by
    /// hand (umya 3.1 cfRule authoring is address-typed and unqualified,
    /// so Harbor does not author CF — it must PRESERVE it).
    fn with_conditional_format(bytes: &[u8]) -> Vec<u8> {
        use std::io::{Read, Write};
        let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..ar.len() {
            let mut e = ar.by_index(i).unwrap();
            let name = e.name().to_string();
            let mut buf = Vec::new();
            e.read_to_end(&mut buf).unwrap();
            entries.push((name, buf));
        }
        let cf = br#"<conditionalFormatting sqref="A1:A5"><cfRule type="cellIs" dxfId="0" priority="1" operator="greaterThan"><formula>15</formula></cfRule></conditionalFormatting>"#;
        // The rule references dxfId 0, so styles.xml must carry the dxfs
        // table too (a cfRule without its dxf is unreadable even to Excel).
        let dxf = br#"<dxfs count="1"><dxf><fill><patternFill><bgColor rgb="FFFFC7CE"/></patternFill></fill></dxf></dxfs>"#;
        for (name, buf) in &mut entries {
            if name == "xl/worksheets/sheet1.xml" {
                let xml = String::from_utf8_lossy(buf).to_string();
                let injected = xml.replace(
                    "</worksheet>",
                    &format!("{}</worksheet>", std::str::from_utf8(cf).unwrap()),
                );
                *buf = injected.into_bytes();
            } else if name == "xl/styles.xml" {
                let xml = String::from_utf8_lossy(buf).to_string();
                let injected = xml.replace(
                    "</styleSheet>",
                    &format!("{}</styleSheet>", std::str::from_utf8(dxf).unwrap()),
                );
                *buf = injected.into_bytes();
            }
        }
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut out);
            for (name, buf) in &entries {
                let opts: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
                zw.start_file(name.as_str(), opts).unwrap();
                zw.write_all(buf).unwrap();
            }
            zw.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn authored_conditional_format_roundtrips() {
        let bytes = roundtrip_workbook();
        let mut doc = WorkbookDoc::load(&bytes).unwrap();
        doc.add_conditional_format(
            "Sheet1",
            "A1:B4",
            CfOperator::GreaterThan,
            15.0,
            "FFC7CE",
        )
        .unwrap();
        let out = doc.to_bytes().unwrap();
        let out_bytes = out.clone();

        // 1. Schema positions: cf after </sheetData> and before
        //    <pageMargins; dxfs before <tableStyles.
        let mut ar = zip::ZipArchive::new(Cursor::new(&out_bytes[..])).unwrap();
        use std::io::Read as _;
        let mut sheet_xml = String::new();
        ar.by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut sheet_xml)
            .unwrap();
        let cf_pos = sheet_xml.find("<conditionalFormatting").unwrap();
        let data_end = sheet_xml.find("</sheetData>").unwrap();
        let margins = sheet_xml.find("<pageMargins").unwrap();
        assert!(cf_pos > data_end && cf_pos < margins, "cf at schema position");
        let mut styles_xml = String::new();
        ar.by_name("xl/styles.xml")
            .unwrap()
            .read_to_string(&mut styles_xml)
            .unwrap();
        let dxfs = styles_xml.find("<dxfs").unwrap();
        let table_styles = styles_xml.find("<tableStyles").unwrap();
        assert!(dxfs < table_styles, "dxfs before tableStyles");

        // 2. umya reads the authored rule back (operator, range, fill).
        let re = WorkbookDoc::load(&out).unwrap();
        let idx = re
            .book
            .sheet_collection()
            .iter()
            .position(|s| s.name() == "Sheet1")
            .unwrap();
        let ws = &re.book.sheet_collection()[idx];
        let cfs = ws.conditional_formatting_collection();
        assert_eq!(cfs.len(), 1);
        assert!(cfs[0].get_sequence_of_references().get_sqref().contains("A1:B4"));
        let rules = cfs[0].get_conditional_collection();
        assert_eq!(rules.len(), 1);
        use umya_spreadsheet::EnumTrait as _;
        assert_eq!(rules[0].get_operator().value_string(), "greaterThan");

        // 3. The rule survives a subsequent edit round trip (the earlier
        //    preservation contract, now starting from an AUTHORED rule).
        let mut doc2 = WorkbookDoc::load(&out).unwrap();
        doc2.set_cell("Sheet1", 2, 1, CellSet::Value(CellValue::Number(99.0)))
            .unwrap();
        let out2 = doc2.to_bytes().unwrap();
        let mut sheet2 = String::new();
        zip::ZipArchive::new(Cursor::new(out2))
            .unwrap()
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut sheet2)
            .unwrap();
        assert!(sheet2.contains("<conditionalFormatting sqref=\"A1:B4\""));
    }

    #[test]
    fn authored_conditional_format_validates_inputs() {
        let bytes = roundtrip_workbook();
        let mut doc = WorkbookDoc::load(&bytes).unwrap();
        // Unknown sheet, bad range, bad color: typed refusals.
        assert!(matches!(
            doc.add_conditional_format("Nope", "A1:A2", CfOperator::Equal, 1.0, "FF0000"),
            Err(WorkbookError::SheetNotFound(_))
        ));
        assert!(matches!(
            doc.add_conditional_format("Sheet1", "A1:..", CfOperator::Equal, 1.0, "FF0000"),
            Err(WorkbookError::BadRef(_))
        ));
        assert!(matches!(
            doc.add_conditional_format("Sheet1", "A1:A2", CfOperator::Equal, 1.0, "red"),
            Err(WorkbookError::BadRef(_))
        ));
    }

    #[test]
    fn conditional_formatting_preserved_through_edit() {
        let bytes = with_conditional_format(&roundtrip_workbook());
        // The package loads despite the unmodeled CF part.
        let mut doc = WorkbookDoc::load(&bytes).unwrap();
        doc.set_cell("Sheet1", 2, 1, CellSet::Value(CellValue::Number(25.0)))
            .unwrap();
        let out = doc.to_bytes().unwrap();
        // The CF rule survived the edit round trip byte-for-byte in the
        // sheet XML (preserve-only: Harbor never rewrites it).
        let mut ar = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
        let mut sheet_bytes = Vec::new();
        use std::io::Read as _;
        ar.by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_end(&mut sheet_bytes)
            .unwrap();
        let sheet = String::from_utf8(sheet_bytes).unwrap();
        // umya parses and re-emits the CF model (attribute order may
        // differ), so the honest contract is structural preservation:
        // the rule, its range, operator and operand all survive.
        assert!(sheet.contains("<conditionalFormatting sqref=\"A1:A5\""));
        assert!(sheet.contains("type=\"cellIs\""));
        assert!(sheet.contains("operator=\"greaterThan\""));
        assert!(sheet.contains("<formula>15</formula>"));
    }

    #[test]
    fn formatting_ops_roundtrip() {
        let bytes = roundtrip_workbook();
        let mut doc = WorkbookDoc::load(&bytes).unwrap();
        doc.set_bold("Sheet1", 1, 1, 3).unwrap();
        doc.set_number_format("Sheet1", 2, 1, 5, "0.0%").unwrap();
        doc.set_column_width("Sheet1", 1, 42.5).unwrap();
        doc.freeze_first_row("Sheet1").unwrap();
        let out = doc.to_bytes().unwrap();
        // Reload through the umya backend and verify the styling survived
        // the round trip (bold on B1..D1, format on B2, width, frozen pane).
        let re = WorkbookDoc::load(&out).unwrap();
        let idx = re
            .book
            .sheet_collection()
            .iter()
            .position(|s| s.name() == "Sheet1")
            .unwrap();
        let ws = &re.book.sheet_collection()[idx];
        assert!(ws.style((2, 1)).font().map(|f| f.bold()).unwrap_or(false));
        assert!(!ws.style((2, 2)).font().map(|f| f.bold()).unwrap_or(false));
        assert_eq!(
            ws.style((2, 2)).number_format().map(|f| f.format_code()),
            Some("0.0%")
        );
        assert_eq!(ws.column_dimension("A").unwrap().width(), 42.5);
        assert!(ws
            .sheets_views()
            .sheet_view_list()
            .first()
            .and_then(|v| v.pane())
            .is_some());
    }

    #[test]
    fn add_bar_chart_persists_chart_part() {
        let bytes = roundtrip_workbook();
        let before = WorkbookDoc::count_charts_in_bytes(&bytes).unwrap();
        let mut doc = WorkbookDoc::load(&bytes).unwrap();
        doc.add_bar_chart(
            "Sheet1",
            "E2",
            "K18",
            vec!["Sheet1!$B$2:$B$5".to_string()],
            "Quarterly",
        )
        .unwrap();
        let out = doc.to_bytes().unwrap();
        let after = WorkbookDoc::count_charts_in_bytes(&out).unwrap();
        assert_eq!(after, before + 1);
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
        let cell = reloaded
            .sheet("Sheet1")
            .unwrap()
            .cells
            .get(&(1, 3))
            .unwrap()
            .clone();
        assert_eq!(cell.cached, Some(CellValue::Number(35.0)));
    }

    fn created_budget() -> Vec<u8> {
        let mut doc = WorkbookDoc::new_empty();
        doc.set_title("Household budget");
        doc.add_sheet("Budget").unwrap();
        let text = |t: &str| CellSet::Value(CellValue::Text(t.into()));
        let num = |n: f64| CellSet::Value(CellValue::Number(n));
        doc.put_cell("Budget", 1, 1, text("Item")).unwrap();
        doc.put_cell("Budget", 1, 2, text("Amount")).unwrap();
        doc.put_cell("Budget", 2, 1, text("Rent")).unwrap();
        doc.put_cell("Budget", 2, 2, num(1200.0)).unwrap();
        doc.put_cell("Budget", 3, 1, text("001")).unwrap();
        doc.put_cell("Budget", 3, 2, num(150.5)).unwrap();
        doc.put_cell("Budget", 4, 1, text("Total")).unwrap();
        doc.put_cell("Budget", 4, 2, CellSet::Formula("=SUM(B2:B3)".into()))
            .unwrap();
        doc.set_bold("Budget", 1, 1, 2).unwrap();
        doc.set_bold("Budget", 4, 1, 2).unwrap();
        doc.set_number_format("Budget", 2, 2, 4, "#,##0.00")
            .unwrap();
        doc.set_column_width("Budget", 1, 14.0).unwrap();
        doc.freeze_first_row("Budget").unwrap();
        let values = doc.recalculate_all().unwrap();
        assert_eq!(
            values.get(&("Budget".to_string(), 4, 2)),
            Some(&CellValue::Number(1350.5))
        );
        doc.to_bytes().unwrap()
    }

    #[test]
    fn created_workbook_is_deterministic_and_reloads() {
        let a = created_budget();
        let b = created_budget();
        // No clock, no random ids: the commit path re-derives this hash
        // from the batch alone and must get the approved bytes back.
        assert_eq!(
            harbor_canonical::sha256_hex(&a),
            harbor_canonical::sha256_hex(&b)
        );
        let problems = crate::package_integrity(&a);
        assert!(problems.is_empty(), "{problems:?}");
        let back = WorkbookDoc::load(&a).unwrap();
        assert_eq!(back.sheet_names(), vec!["Budget".to_string()]);
        let sheet = back.sheet("Budget").unwrap();
        let total = sheet.cells.get(&(2, 4)).unwrap();
        assert_eq!(total.formula.as_deref(), Some("SUM(B2:B3)"));
        assert_eq!(total.cached, Some(CellValue::Number(1350.5)));
        assert_eq!(
            sheet.cells.get(&(1, 2)).unwrap().cached,
            Some(CellValue::Text("Rent".into()))
        );
        // The empty workbook has no sheet until one is added, and a
        // duplicate sheet name is refused.
        let mut empty = WorkbookDoc::new_empty();
        assert!(empty.sheet_names().is_empty());
        empty.add_sheet("One").unwrap();
        assert!(empty.add_sheet("One").is_err());
        assert!(empty
            .put_cell("Missing", 1, 1, CellSet::Value(CellValue::Number(1.0)))
            .is_err());
        assert!(empty
            .put_cell("One", 0, 1, CellSet::Value(CellValue::Number(1.0)))
            .is_err());
    }

    #[test]
    fn col_letter_number_roundtrip() {
        assert_eq!(col_letter(1), "A");
        assert_eq!(col_letter(26), "Z");
        assert_eq!(col_letter(27), "AA");
        assert_eq!(col_letter(52), "AZ");
        assert_eq!(col_number("AZ"), 52);
        // The grid's real edge, and past it.
        assert_eq!(col_number("XFD"), MAX_COL);
        // 19 letters used to overflow the accumulator: a panic in debug,
        // a silent wrap to some other column in release.
        assert_eq!(col_number("Dxxxxxxxxxxxxxxxxxx"), u32::MAX);
        assert_eq!(col_number("AAAAAAAAAAAAAAAAAAAA"), u32::MAX);
        // A non-letter used to underflow b - b'A'.
        assert_eq!(col_number("A1"), u32::MAX);
        assert_eq!(col_number("-"), u32::MAX);
    }
}
