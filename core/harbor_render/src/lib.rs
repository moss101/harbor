//! Harbor preview renderer: artifact bytes -> Work Canvas preview IR.
//!
//! Scope (21_Office_Feature_Matrix.csv, Read/render rows): XLSX grid with
//! values/formulas, PPTX slide titles + bullets. PRESERVE_ONLY parts are
//! reported, never dropped silently. The IR is plain JSON so it crosses
//! the FFI seam to Flutter unchanged.

pub mod pdf;
pub mod preview;

pub use pdf::{PdfError, PdfPage, PdfPreview};
pub use preview::{DeckPreview, DocxParagraphPreview, DocxPreview, SlidePreview, WorkbookPreview};
