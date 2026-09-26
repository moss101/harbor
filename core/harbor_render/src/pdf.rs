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
/// PDF Research skill and for Report to Slides: every claim names its page,
/// so `index` must be the page the text is really on.
///
/// This used to extract the whole document as one string and split it on
/// form feeds, on the belief that pdf-extract separates pages with them.
/// The pinned pdf-extract (0.12) does not: a multi-page PDF came back as a
/// single "page" holding every page's text — numbered 2, because the one
/// form feed it does emit is at the very start. Every page citation made
/// from it was wrong. Pages are now extracted one at a time; `page_count`
/// is the document's real page count, and pages with no extractable text
/// (scans) are omitted while the others keep their true numbers.
pub fn extract_pages(bytes: &[u8]) -> Result<PdfPreview, PdfError> {
    let by_page = pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map_err(|e| PdfError::Extract(e.to_string()))?;
    if by_page.is_empty() {
        // The per-page walk stops at the first page it cannot read; a
        // document whose first page fails still gets its text, unpaged.
        let text = pdf_extract::extract_text_from_mem(bytes)
            .map_err(|e| PdfError::Extract(e.to_string()))?;
        let text = text.trim().trim_matches('\u{0c}').trim().to_string();
        return Ok(PdfPreview {
            kind: "pdf".into(),
            page_count: usize::from(!text.is_empty()),
            pages: if text.is_empty() {
                Vec::new()
            } else {
                vec![PdfPage { index: 1, text }]
            },
        });
    }
    let page_count = by_page.len();
    let pages: Vec<PdfPage> = by_page
        .into_iter()
        .enumerate()
        .map(|(i, p)| (i, p.trim().trim_matches('\u{0c}').trim().to_string()))
        .filter(|(_, p)| !p.is_empty())
        .map(|(i, text)| PdfPage { index: i + 1, text })
        .collect();
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
    fn pages_keep_their_own_numbers() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/office/quarterly_report.pdf");
        let pdf = extract_pages(&std::fs::read(p).unwrap()).unwrap();
        assert_eq!(pdf.page_count, 3);
        let indexes: Vec<usize> = pdf.pages.iter().map(|p| p.index).collect();
        assert_eq!(indexes, vec![1, 2, 3]);
        assert!(pdf.pages[0].text.contains("Patient visits grew 14%"));
        assert!(pdf.pages[1].text.contains("Operating revenue was AED 6.4m"));
        assert!(!pdf.pages[1].text.contains("Patient visits"));
        assert!(pdf.pages[2].text.contains("Nurse turnover reached 11%"));
    }

    #[test]
    fn corrupt_bytes_rejected() {
        assert!(extract_pages(b"not a pdf at all").is_err());
    }
}
