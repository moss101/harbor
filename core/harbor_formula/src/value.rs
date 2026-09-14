//! Harbor cell value model with the five trust distinctions required by
//! the spreadsheet trust model (authority §11):
//! formula text, cached result, recalculated result, verified result,
//! unsupported dependency.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Blank,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellError {
    DivZero, // #DIV/0!
    Value,   // #VALUE!
    Ref,     // #REF!
    Name,    // #NAME?
    Num,     // #NUM!
    NA,      // #N/A
    Null,    // #NULL!
    Spill,   // #SPILL!
    Calc,    // #CALC!
    GettingData,
}

impl CellError {
    pub fn code(&self) -> &'static str {
        match self {
            CellError::DivZero => "#DIV/0!",
            CellError::Value => "#VALUE!",
            CellError::Ref => "#REF!",
            CellError::Name => "#NAME?",
            CellError::Num => "#NUM!",
            CellError::NA => "#N/A",
            CellError::Null => "#NULL!",
            CellError::Spill => "#SPILL!",
            CellError::Calc => "#CALC!",
            CellError::GettingData => "#GETTING_DATA",
        }
    }

    pub fn from_code(s: &str) -> Option<CellError> {
        Some(match s {
            "#DIV/0!" => CellError::DivZero,
            "#VALUE!" => CellError::Value,
            "#REF!" => CellError::Ref,
            "#NAME?" => CellError::Name,
            "#NUM!" => CellError::Num,
            "#N/A" => CellError::NA,
            "#NULL!" => CellError::Null,
            "#SPILL!" => CellError::Spill,
            "#CALC!" => CellError::Calc,
            "#GETTING_DATA" => CellError::GettingData,
            _ => return None,
        })
    }
}

impl fmt::Display for CellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellValue::Blank => write!(f, ""),
            CellValue::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            CellValue::Text(s) => write!(f, "{s}"),
            CellValue::Bool(b) => write!(f, "{b}"),
            CellValue::Error(e) => write!(f, "{}", e.code()),
        }
    }
}

impl CellValue {
    /// Numeric view used by comparison fixtures (Excel coercion rules are
    /// handled by the engine; this is only for matching results).
    pub fn as_number(&self) -> Option<f64> {
        match self {
            CellValue::Number(n) => Some(*n),
            _ => None,
        }
    }
}

/// Provenance status of a computed cell in a Harbor-processed workbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecalcStatus {
    /// No formula: a literal input cell.
    Literal,
    /// Formula with only a cached value (not recomputed by Harbor).
    CachedOnly,
    /// Recomputed by the pinned engine; matches expectation when checked.
    Recalculated,
    /// Recomputed AND matching an independently-provided expected value.
    Verified,
    /// Depends on a function outside the qualified set; value preserved,
    /// never verified.
    UnsupportedDependency,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecalcCell {
    pub sheet: String,
    pub row: u32,
    pub col: u32,
    pub formula: Option<String>,
    pub cached: Option<CellValue>,
    pub recalculated: Option<CellValue>,
    pub status: RecalcStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_roundtrip() {
        for code in [
            "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A", "#NULL!",
        ] {
            assert_eq!(CellError::from_code(code).unwrap().code(), code);
        }
        assert!(CellError::from_code("#NOPE").is_none());
    }
}
