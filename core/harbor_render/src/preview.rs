//! Preview IR construction from artifact bytes.

use serde::Serialize;

use harbor_artifacts::workbook::WorkbookDoc;
use harbor_artifacts::PptxDeck;

#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("workbook: {0}")]
    Workbook(#[from] harbor_artifacts::workbook::WorkbookError),
    #[error("pptx: {0}")]
    Pptx(#[from] harbor_artifacts::PptxError),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PreviewCell {
    pub row: u32,
    pub col: u32,
    pub formula: Option<String>,
    /// Cached value as stored; never presented as verified by itself.
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkbookPreview {
    pub kind: String,
    pub sheet: String,
    pub sheets: Vec<String>,
    pub cells: Vec<PreviewCell>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SlidePreview {
    pub index: usize,
    pub title: String,
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeckPreview {
    pub kind: String,
    pub title: String,
    pub slides: Vec<SlidePreview>,
}

impl WorkbookPreview {
    /// Build the grid preview from workbook bytes (first sheet focused).
    pub fn from_xlsx(bytes: &[u8]) -> Result<Self, PreviewError> {
        let doc = WorkbookDoc::load(bytes)?;
        let sheets = doc.sheet_names();
        let first = sheets.first().cloned().unwrap_or_default();
        let data = doc.sheet(&first)?;
        let mut cells = Vec::new();
        for ((col, row), cell) in data.cells.iter() {
            cells.push(PreviewCell {
                row: *row,
                col: *col,
                formula: cell.formula.clone(),
                value: cell.cached.as_ref().map(|v| v.to_string()),
            });
        }
        cells.sort_by_key(|c| (c.row, c.col));
        Ok(WorkbookPreview {
            kind: "workbook".into(),
            sheet: first,
            sheets,
            cells,
        })
    }
}

impl DeckPreview {
    pub fn from_pptx(bytes: &[u8]) -> Result<Self, PreviewError> {
        let deck = PptxDeck::from_pptx_bytes(bytes)?;
        Ok(DeckPreview {
            kind: "deck".into(),
            title: deck.title,
            slides: deck
                .slides
                .iter()
                .enumerate()
                .map(|(i, s)| SlidePreview {
                    index: i + 1,
                    title: s.title.clone(),
                    bullets: s.bullets.clone(),
                })
                .collect(),
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("preview serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_artifacts::SlideContent;
    use harbor_formula::engine::HarborWorkbook;
    use harbor_formula::value::CellValue;

    #[test]
    fn workbook_preview_reflects_values_and_formulas() {
        let mut wb = HarborWorkbook::new();
        wb.set_value("Sheet1", 1, 1, CellValue::Number(10.0));
        wb.set_formula("Sheet1", 2, 1, "=A1*3");
        let mut doc = harbor_artifacts::WorkbookDoc::load(&wb.to_xlsx_bytes()).unwrap();
        doc.recalculate_all().unwrap();
        let bytes = doc.to_bytes().unwrap();
        let preview = WorkbookPreview::from_xlsx(&bytes).unwrap();
        assert_eq!(preview.kind, "workbook");
        assert_eq!(preview.sheet, "Sheet1");
        let a2 = preview.cells.iter().find(|c| c.row == 2 && c.col == 1).unwrap();
        assert!(a2.formula.as_deref().unwrap_or_default().contains("A1*3"));
        assert_eq!(a2.value.as_deref(), Some("30"));
        let json = serde_json::to_string(&preview).unwrap();
        assert!(json.contains("\"formula\""));
    }

    #[test]
    fn deck_preview_lists_slides() {
        let deck = PptxDeck {
            title: "Board".into(),
            slides: vec![SlideContent {
                title: "Totals".into(),
                bullets: vec!["Q1 3600".into()],
                notes: None,
            }],
        };
        let bytes = deck.to_pptx_bytes().unwrap();
        let preview = DeckPreview::from_pptx(&bytes).unwrap();
        assert_eq!(preview.kind, "deck");
        assert_eq!(preview.slides.len(), 1);
        assert_eq!(preview.slides[0].title, "Totals");
        assert_eq!(preview.slides[0].bullets, vec!["Q1 3600".to_string()]);
        assert!(preview.to_json().contains("\"bullets\""));
    }
}
