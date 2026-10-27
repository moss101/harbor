//! Harbor artifact engine: typed operations over Office artifacts with
//! version-bound, all-or-nothing, conflict-detecting commit.
//!
//! Authority: `02_Runtime_Effect_and_Artifact_Contracts.md` (artifact
//! batches and safe save), `schemas/artifact_batch.schema.json`,
//! `schemas/artifact_commit.schema.json`,
//! `27_Artifact_Commit_Qualification.md`, `21_Office_Feature_Matrix.csv`.
//!
//! The LLM decides *what* should change; this engine decides *how the file
//! changes*: every mutation is an operation batch bound to
//! `artifact_id + base_version_id + base_content_hash`; the proposed
//! output is staged, hashed, approved, revalidated against the live file
//! immediately before publication, and committed through a qualified
//! safe-save mode or a no-overwrite new copy.

pub mod batch;
pub mod chart_xml;
pub mod commit;
pub mod convert;
pub mod deck;
pub mod diff;
pub mod docx;
pub mod office_matrix;
pub mod package;
pub mod pptx;
pub mod workbook;

pub use batch::{ArtifactBatch, OpKind, Operation, Precondition, EMPTY_CONTENT_HASH};
pub use chart_xml::{styled_chart_space_xml, ChartLegend, ChartStyle, ChartVariant};
pub use commit::{CommitJournal, CommitMode, CommitOutcome, SafeCommitError, SafeCommitter};
pub use convert::{markdown_to_blocks, markdown_to_deck, markdown_to_docx, markdown_to_pptx};
pub use deck::{Block, Design, DesignedDeck, DesignedSlide, LayoutKind, MotionPreset};
pub use diff::{ArtifactDiff, DiffEntry};
pub use docx::{create_docx, BlockStyle, DocxBlock, DocxDocument, DocxError, DocxOp};
pub use office_matrix::{
    classify_part, compatibility_report, Classification, CompatibilityReport, MatrixClass,
    OfficeFormat,
};
pub use package::package_integrity;
pub use pptx::{
    apply_slide_text_edit, ChartKind, ChartSpec, DeckStyle, PptxDeck, PptxError, PptxOp,
    SlideContent, SlideImage,
};
pub use workbook::{
    CfOperator, PreservationReport, SheetData, WorkbookDoc, WorkbookOp, XlsxChartKind,
};

/// Linear pre-scan that rejects element nesting deeper than `max`
/// *before* the recursive XML parser sees the text. Understands comments,
/// CDATA, processing instructions and quoted attribute values; it does not
/// validate well-formedness (the parser does that).
pub fn check_xml_depth(xml: &str, max: usize) -> Result<(), String> {
    let b = xml.as_bytes();
    let mut i = 0;
    let mut depth = 0usize;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &b[i..];
        let skip_to = |pat: &[u8]| -> usize {
            rest.windows(pat.len())
                .position(|w| w == pat)
                .map_or(b.len(), |p| i + p + pat.len())
        };
        if rest.starts_with(b"<!--") {
            i = skip_to(b"-->");
        } else if rest.starts_with(b"<![CDATA[") {
            i = skip_to(b"]]>");
        } else if rest.starts_with(b"<?") {
            i = skip_to(b"?>");
        } else if rest.starts_with(b"<!") {
            i = skip_to(b">");
        } else if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            i = skip_to(b">");
        } else {
            // Start tag: find its closing '>' outside attribute quotes.
            let mut j = i + 1;
            let mut quote = 0u8;
            while j < b.len() {
                match (quote, b[j]) {
                    (0, b'"') | (0, b'\'') => quote = b[j],
                    (q, c) if q != 0 && c == q => quote = 0,
                    (0, b'>') => break,
                    _ => {}
                }
                j += 1;
            }
            let self_closing = j > i + 1 && j <= b.len() && b.get(j - 1) == Some(&b'/');
            if !self_closing {
                depth += 1;
                if depth > max {
                    return Err(format!("chart: XML nesting deeper than {max} levels"));
                }
            }
            i = j + 1;
        }
    }
    Ok(())
}

/// Deepest element nesting any Harbor reader accepts. roxmltree recurses
/// per level, and a few hundred nested elements overflow a thread stack.
pub const MAX_XML_DEPTH: usize = 64;

/// Probe every entry of an OOXML package once before any upstream reader
/// sees it: inflate each entry (a corrupt compressed stream is a typed
/// error) and require every XML part to be well-formed (fuzzing found the
/// upstream workbook reader aborting on both a corrupt deflate stream and
/// an unclosed attribute in `[Content_Types].xml`). Bounded by the package
/// itself: non-XML entries are streamed to a sink, never buffered.
pub(crate) fn inflate_probe<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("corrupt entry {i}: {e}"))?;
        let name = entry.name().to_string();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".xml") || lower.ends_with(".rels") {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text)
                .map_err(|e| format!("corrupt entry {name}: {e}"))?;
            check_xml_depth(&text, MAX_XML_DEPTH)
                .map_err(|e| format!("malformed XML part {name}: {e}"))?;
            roxmltree::Document::parse(&text)
                .map_err(|e| format!("malformed XML part {name}: {e}"))?;
        } else {
            std::io::copy(&mut entry, &mut std::io::sink())
                .map_err(|e| format!("corrupt entry {name}: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod depth_tests {
    use super::*;

    #[test]
    fn hostile_nesting_is_refused_before_the_recursive_parser_runs() {
        let deep = format!("{}{}", "<a>".repeat(5000), "</a>".repeat(5000));
        assert!(check_xml_depth(&deep, MAX_XML_DEPTH).is_err());
        let ok = format!("{}{}", "<a>".repeat(60), "</a>".repeat(60));
        assert!(check_xml_depth(&ok, MAX_XML_DEPTH).is_ok());
        // A package carrying such a part is rejected by the shared probe, so
        // every reader that probes first (pptx, docx, xlsx) is covered.
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("ppt/slides/slide1.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, deep.as_bytes()).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(inflate_probe(&mut archive).is_err());
    }
}
