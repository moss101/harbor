//! Pinned engine identity and the workbook facade used by Harbor.

use chrono::Timelike;
use formualizer::workbook::Workbook;

use crate::value::{CellError, CellValue, RecalcCell, RecalcStatus};

/// The pinned calculation engine identity (22_Formula_Coverage.json
/// `selected_engine`, with the HBR-153 pinning resolved).
///
/// `source_revision` names the exact crates.io release; `integrity_sha256`
/// is the SHA-256 over the packed `.crate` archives of formualizer and its
/// in-tree workspace crates at the pinned version (formualizer,
/// formualizer-common, formualizer-parse, formualizer-eval,
/// formualizer-workbook), computed by `tools/pin_engine.py` and recorded in
/// docs/decisions. A different toolchain resolves the same bytes from the
/// same registry, and Cargo.lock pins the resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIdentity {
    pub family: &'static str,
    pub version: &'static str,
    pub source_revision: &'static str,
    pub integrity_sha256: &'static str,
    pub adapter_revision: &'static str,
}

pub const ENGINE: EngineIdentity = EngineIdentity {
    family: "Formualizer",
    version: "0.9.3",
    source_revision: "crates.io/formualizer@0.9.3",
    // Recorded by tools/pin_engine.py (see docs/decisions/0001-formula-engine-pin.md).
    integrity_sha256: "64b7771c27fcd3da6229ae4653acc4269cf6635412b283fa0dd16e8359b54139",
    adapter_revision: "harbor_formula/1",
};

pub fn engine_identity() -> EngineIdentity {
    EngineIdentity { ..ENGINE }
}

/// A Harbor-owned workbook backed by the pinned engine.
pub struct HarborWorkbook {
    wb: Workbook,
}

impl Default for HarborWorkbook {
    fn default() -> Self {
        Self::new()
    }
}

impl HarborWorkbook {
    pub fn new() -> Self {
        let mut wb = Workbook::new();
        if !wb.has_sheet("Sheet1") {
            wb.add_sheet("Sheet1").expect("default sheet");
        }
        HarborWorkbook { wb }
    }

    pub fn add_sheet(&mut self, name: &str) {
        if !self.wb.has_sheet(name) {
            self.wb.add_sheet(name).expect("sheet name");
        }
    }

    /// Set a literal cell value.
    pub fn set_value(&mut self, sheet: &str, row: u32, col: u32, v: CellValue) {
        let lit = to_literal(&v);
        self.wb
            .set_value(sheet, row, col, lit)
            .expect("set literal value");
    }

    /// Set a formula (with the leading '=').
    pub fn set_formula(&mut self, sheet: &str, row: u32, col: u32, formula: &str) {
        self.wb
            .set_formula(sheet, row, col, formula)
            .expect("set formula");
    }

    /// Evaluate one cell and map the engine result into the Harbor value
    /// model.
    pub fn evaluate_cell(&mut self, sheet: &str, row: u32, col: u32) -> CellValue {
        match self.wb.evaluate_cell(sheet, row, col) {
            Ok(lit) => from_literal(&lit),
            Err(e) => from_engine_error(&e.to_string()),
        }
    }

    /// Raw display for debugging.
    pub fn display_cell(&mut self, sheet: &str, row: u32, col: u32) -> String {
        match self.wb.evaluate_cell(sheet, row, col) {
            Ok(lit) => format!("{lit:?}"),
            Err(e) => format!("ERR:{e}"),
        }
    }

    /// Export the workbook as XLSX bytes (used by the artifact save path
    /// and save-reload fixtures).
    pub fn to_xlsx_bytes(&self) -> Vec<u8> {
        self.wb.to_xlsx_bytes().expect("xlsx serialization")
    }

    pub fn inner(&self) -> &Workbook {
        &self.wb
    }

    pub fn inner_mut(&mut self) -> &mut Workbook {
        &mut self.wb
    }

    /// Recalculate a set of formula cells, producing provenance-tagged
    /// cells that distinguish cached-only from recomputed values.
    pub fn recalc_cells(
        &mut self,
        cells: Vec<(String, u32, u32, Option<String>)>,
    ) -> Vec<RecalcCell> {
        cells
            .into_iter()
            .map(|(sheet, row, col, formula)| {
                let recalculated = self.evaluate_cell(&sheet, row, col);
                let status = if formula.is_some() {
                    RecalcStatus::Recalculated
                } else {
                    RecalcStatus::Literal
                };
                RecalcCell {
                    sheet,
                    row,
                    col,
                    formula,
                    cached: None,
                    recalculated: Some(recalculated),
                    status,
                }
            })
            .collect()
    }
}

fn naive_to_serial(dt: chrono::NaiveDateTime) -> f64 {
    let epoch = chrono::NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap(),
        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    );
    // Total seconds since epoch, expressed in days (num_seconds includes
    // whole days).
    let delta = dt - epoch;
    delta.num_seconds() as f64 / 86_400.0 + delta.subsec_nanos() as f64 / (86_400.0 * 1e9)
}

pub fn to_literal(v: &CellValue) -> formualizer::LiteralValue {
    use formualizer::LiteralValue as L;
    match v {
        CellValue::Blank => L::Empty,
        CellValue::Number(n) => L::Number(*n),
        CellValue::Text(s) => L::Text(s.clone()),
        CellValue::Bool(b) => L::Boolean(*b),
        CellValue::Error(e) => L::Error(err_to_lit(e)),
    }
}

fn err_to_lit(e: &CellError) -> formualizer::ExcelError {
    use formualizer::ExcelError;
    use formualizer::ExcelErrorKind as K;
    ExcelError::new(match e {
        CellError::DivZero => K::Div,
        CellError::Value => K::Value,
        CellError::Ref => K::Ref,
        CellError::Name => K::Name,
        CellError::Num => K::Num,
        CellError::NA => K::Na,
        CellError::Null => K::Null,
        CellError::Spill => K::Spill,
        CellError::Calc => K::Calc,
        CellError::GettingData => K::Error,
    })
}

pub fn from_literal(lit: &formualizer::LiteralValue) -> CellValue {
    use formualizer::LiteralValue as L;
    match lit {
        L::Empty | L::Pending => CellValue::Blank,
        L::Int(i) => CellValue::Number(*i as f64),
        L::Number(n) => CellValue::Number(*n),
        L::Text(s) => CellValue::Text(s.clone()),
        L::Boolean(b) => CellValue::Bool(*b),
        // Dates surface as Excel 1900-system serial numbers (the workbook
        // storage model). The 1900-01-01..1900-02-28 leap-bug zone is
        // excluded from the qualified corpus (docs/decisions/0002).
        L::Date(d) => CellValue::Number(naive_to_serial(d.and_hms_opt(0, 0, 0).unwrap())),
        L::DateTime(dt) => CellValue::Number(naive_to_serial(*dt)),
        L::Time(t) => CellValue::Number(
            (t.num_seconds_from_midnight() as f64 + t.nanosecond() as f64 / 1e9) / 86_400.0,
        ),
        L::Duration(_) => CellValue::Text(lit.to_string()),
        L::Array(a) => {
            // Scalar context: take first element when present.
            if let Some(row) = a.first() {
                if let Some(first) = row.first() {
                    return from_literal(first);
                }
            }
            CellValue::Blank
        }
        L::Error(e) => from_engine_error(&e.kind.to_string()),
    }
}

fn from_engine_error(s: &str) -> CellValue {
    // Map engine error spellings to Harbor codes.
    let lower = s.to_ascii_lowercase();
    let e = if lower.contains("div") {
        CellError::DivZero
    } else if lower.contains("#value") || lower.contains("value") {
        CellError::Value
    } else if lower.contains("#ref") || lower.contains("ref") {
        CellError::Ref
    } else if lower.contains("#name") || lower.contains("name") {
        CellError::Name
    } else if lower.contains("#num") || lower.contains("num") {
        CellError::Num
    } else if lower.contains("#n/a") || lower.contains("na") {
        CellError::NA
    } else if lower.contains("null") {
        CellError::Null
    } else if lower.contains("spill") {
        CellError::Spill
    } else if lower.contains("calc") {
        CellError::Calc
    } else {
        CellError::Value
    };
    CellValue::Error(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_smoke() {
        let mut wb = HarborWorkbook::new();
        wb.set_formula("Sheet1", 1, 1, "=1+2*3");
        assert_eq!(wb.evaluate_cell("Sheet1", 1, 1), CellValue::Number(7.0));
    }

    #[test]
    fn cells_and_ranges() {
        let mut wb = HarborWorkbook::new();
        wb.set_value("Sheet1", 1, 1, CellValue::Number(10.0));
        wb.set_value("Sheet1", 2, 1, CellValue::Number(20.0));
        wb.set_value("Sheet1", 3, 1, CellValue::Number(30.0));
        wb.set_formula("Sheet1", 1, 2, "=SUM(A1:A3)");
        assert_eq!(wb.evaluate_cell("Sheet1", 1, 2), CellValue::Number(60.0));
        wb.set_formula("Sheet1", 2, 2, "=AVERAGE(A1:A3)");
        assert_eq!(wb.evaluate_cell("Sheet1", 2, 2), CellValue::Number(20.0));
    }

    #[test]
    fn cross_sheet_reference() {
        let mut wb = HarborWorkbook::new();
        wb.add_sheet("Data");
        wb.set_value("Data", 1, 1, CellValue::Number(42.0));
        wb.set_formula("Sheet1", 1, 1, "=Data!A1*2");
        assert_eq!(wb.evaluate_cell("Sheet1", 1, 1), CellValue::Number(84.0));
    }

    #[test]
    fn lookup_and_logic() {
        let mut wb = HarborWorkbook::new();
        // A: keys, B: values
        wb.set_value("Sheet1", 1, 1, CellValue::Text("alpha".into()));
        wb.set_value("Sheet1", 1, 2, CellValue::Number(1.0));
        wb.set_value("Sheet1", 2, 1, CellValue::Text("beta".into()));
        wb.set_value("Sheet1", 2, 2, CellValue::Number(2.0));
        wb.set_formula("Sheet1", 1, 3, "=VLOOKUP(\"beta\",A1:B2,2,FALSE)");
        assert_eq!(wb.evaluate_cell("Sheet1", 1, 3), CellValue::Number(2.0));
        wb.set_formula("Sheet1", 2, 3, "=IF(A1=\"alpha\",TRUE,FALSE)");
        assert_eq!(wb.evaluate_cell("Sheet1", 2, 3), CellValue::Bool(true));
        wb.set_formula("Sheet1", 3, 3, "=IFERROR(1/0,\"safe\")");
        assert_eq!(
            wb.evaluate_cell("Sheet1", 3, 3),
            CellValue::Text("safe".into())
        );
    }

    #[test]
    fn div_zero_maps_to_error() {
        let mut wb = HarborWorkbook::new();
        wb.set_formula("Sheet1", 1, 1, "=1/0");
        assert_eq!(
            wb.evaluate_cell("Sheet1", 1, 1),
            CellValue::Error(CellError::DivZero)
        );
    }
}
