//! PDF text extraction with page mapping (PDF Research skill input).
//!
//! Bounded scope per the Office Feature Matrix: text extraction and page
//! mapping for reading/research. Form fields, embedded media and
//! scanned-image OCR are separate capabilities (OCR requires a qualified
//! offline OCR engine).

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PdfPage {
    pub index: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PdfPreview {
    pub kind: String,
    pub page_count: usize,
    pub pages: Vec<PdfPage>,
}

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("pdf extract: {0}")]
    Extract(String),
}

/// Extract text page-by-page. Page mapping is the citation basis for the
/// PDF Research skill: every claim names its page.
pub fn extract_pages(bytes: &[u8]) -> Result<PdfPreview, PdfError> {
    let text =
        pdf_extract::extract_text_from_mem(bytes).map_err(|e| PdfError::Extract(e.to_string()))?;
    // pdf-extract emits form feeds (\x0c) between pages.
    let pages: Vec<PdfPage> = text
        .split('\u{0c}')
        .enumerate()
        .filter(|(_, p)| !p.trim().is_empty())
        .map(|(i, p)| PdfPage {
            index: i + 1,
            text: p.trim().to_string(),
        })
        .collect();
    let page_count = pages.len();
    Ok(PdfPreview {
        kind: "pdf".into(),
        page_count,
        pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/office/hello.pdf");
        std::fs::read(p).expect("pdf fixture")
    }

    #[test]
    fn extracts_text_and_pages() {
        let pdf = extract_pages(&fixture()).unwrap();
        assert_eq!(pdf.kind, "pdf");
        assert_eq!(pdf.page_count, 1);
        let joined = pdf
            .pages
            .iter()
            .map(|p| p.text.as_str())
            .collect::<String>();
        assert!(joined.contains("hello harbor"), "text: {joined}");
        let json = serde_json::to_string(&pdf).unwrap();
        assert!(json.contains("\"pages\""));
    }

    #[test]
    fn corrupt_bytes_rejected() {
        assert!(extract_pages(b"not a pdf at all").is_err());
    }
}
