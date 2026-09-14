//! Office Feature Matrix compatibility classifier (21_Office_Feature_Matrix.csv).
//!
//! Rows 8/16/21/23: every package part and document-level feature is
//! classified as SUPPORTED_GA, PRESERVE_ONLY, PRESERVE_NO_EXECUTE,
//! REQUIRED_UNQUALIFIED (charts) or UNKNOWN. Row 23 rule: features absent
//! from the matrix classify as UNKNOWN and MUST be surfaced as
//! reject-or-preserve-only — Harbor never silently claims support.

use std::io::{Cursor, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatrixClass {
    /// Row class SUPPORTED_GA: read/render/edit within qualified scope.
    SupportedGa,
    /// Row class PRESERVE_ONLY: kept byte-identical, never rendered/edited.
    PreserveOnly,
    /// Row class PRESERVE_NO_EXECUTE: preserved; never executed; warn
    /// before export.
    PreserveNoExecute,
    /// Row class REQUIRED_UNQUALIFIED: chart machinery exists pending
    /// qualification PASS.
    RequiredUnqualified,
    /// Row 23: feature absent from the matrix — reject or preserve-only,
    /// never silently claimed.
    Unknown,
}

impl MatrixClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            MatrixClass::SupportedGa => "SUPPORTED_GA",
            MatrixClass::PreserveOnly => "PRESERVE_ONLY",
            MatrixClass::PreserveNoExecute => "PRESERVE_NO_EXECUTE",
            MatrixClass::RequiredUnqualified => "REQUIRED_UNQUALIFIED",
            MatrixClass::Unknown => "UNKNOWN_REJECT_OR_PRESERVE_ONLY",
        }
    }
}

/// One classified package part or document feature.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Classification {
    /// Package part name (or document feature marker like
    /// "document.xml#floatingDrawing").
    pub part: String,
    pub class: MatrixClass,
    /// Human-readable reason referencing the matrix row family.
    pub reason: &'static str,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CompatibilityReport {
    pub format: OfficeFormat,
    pub entries: Vec<Classification>,
    /// Row 23: parts absent from the matrix. Treat as
    /// REJECT_OR_PRESERVE_ONLY and show the compatibility banner.
    pub unknown_parts: Vec<String>,
}

impl CompatibilityReport {
    /// A compatibility banner must be shown when anything beyond
    /// SUPPORTED_GA scope is present.
    pub fn banner_required(&self) -> bool {
        !self.unknown_parts.is_empty()
            || self.entries.iter().any(|e| {
                !matches!(e.class, MatrixClass::SupportedGa)
                    && !matches!(e.class, MatrixClass::RequiredUnqualified)
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OfficeFormat {
    #[default]
    Docx,
    Xlsx,
    Pptx,
}

/// Classify a single package part name for `format`.
pub fn classify_part(format: OfficeFormat, part: &str) -> Classification {
    let p = part;
    let class_reason = |class: MatrixClass, reason: &'static str| Classification {
        part: part.to_string(),
        class,
        reason,
    };
    // Active/execution content is PRESERVE_NO_EXECUTE in every format
    // (matrix rows 8/16/21).
    if p.ends_with("vbaProject.bin")
        || p.contains("activeX/")
        || p.contains("embeddings/oleObject")
        || p.ends_with(".bin")
    {
        return class_reason(
            MatrixClass::PreserveNoExecute,
            "macros/OLE/active content — never execute; warn before export",
        );
    }
    match format {
        OfficeFormat::Docx => classify_docx_part(p, class_reason),
        OfficeFormat::Xlsx => classify_xlsx_part(p, class_reason),
        OfficeFormat::Pptx => classify_pptx_part(p, class_reason),
    }
}

fn classify_docx_part(
    p: &str,
    class_reason: impl Fn(MatrixClass, &'static str) -> Classification,
) -> Classification {
    // Matrix row 5: headers/footers/settings/styles are supported scope.
    if p == "word/document.xml"
        || p.starts_with("word/media/")
        || p.starts_with("word/header")
        || p.starts_with("word/footer")
        || p.starts_with("word/styles")
        || p.starts_with("word/numbering")
        || p.starts_with("word/settings")
        || p.starts_with("word/_rels/")
        || p.starts_with("word/theme/")
    {
        return class_reason(
            MatrixClass::SupportedGa,
            "core document, media, headers/footers, styles (rows 1-5)",
        );
    }
    // Row 7: fields/TOC live inside document.xml (classified at feature
    // level by classify_docx_features); no dedicated part in the matrix.
    unknown_part(p)
}

fn classify_xlsx_part(
    p: &str,
    class_reason: impl Fn(MatrixClass, &'static str) -> Classification,
) -> Classification {
    if p == "xl/workbook.xml"
        || p.starts_with("xl/worksheets/")
        || p.starts_with("xl/styles")
        || p.starts_with("xl/sharedStrings")
        || p.starts_with("xl/theme/")
        || p.starts_with("xl/media/")
        || p.starts_with("xl/tables/")
        || p.starts_with("xl/_rels/")
    {
        return class_reason(
            MatrixClass::SupportedGa,
            "values, formulas, styles, merges, dimensions (rows 9-10)",
        );
    }
    if p.starts_with("xl/charts/") || p.starts_with("xl/drawings/") {
        return class_reason(
            MatrixClass::RequiredUnqualified,
            "charts: bar/column/line/pie/scatter basic series pending qualification (row 13)",
        );
    }
    if p.starts_with("xl/pivotTables/")
        || p.starts_with("xl/pivotCache/")
        || p == "xl/connections.xml"
        || p.starts_with("xl/customXml")
    {
        return class_reason(
            MatrixClass::PreserveOnly,
            "pivot tables / slicers / Power Query — never recalc; carried verbatim (row 14)",
        );
    }
    if p.starts_with("xl/externalLinks/") {
        return class_reason(
            MatrixClass::PreserveOnly,
            "external workbook/data links — no automatic fetch (row 15)",
        );
    }
    unknown_part(p)
}

fn classify_pptx_part(
    p: &str,
    class_reason: impl Fn(MatrixClass, &'static str) -> Classification,
) -> Classification {
    if p == "ppt/presentation.xml"
        || p.starts_with("ppt/slides/")
        || p.starts_with("ppt/notesSlides/")
        || p.starts_with("ppt/slideMasters/")
        || p.starts_with("ppt/slideLayouts/")
        || p.starts_with("ppt/theme/")
        || p.starts_with("ppt/notesMasters/")
        || p.starts_with("ppt/media/")
        || p.starts_with("docProps/")
        || p.starts_with("ppt/_rels/")
    {
        return class_reason(
            MatrixClass::SupportedGa,
            "slides, shapes, images, themes, speaker notes (rows 17-18)",
        );
    }
    if p.starts_with("ppt/charts/") || p.starts_with("ppt/embeddings/") {
        return class_reason(
            MatrixClass::RequiredUnqualified,
            "charts from Harbor IR pending qualification (row 19)",
        );
    }
    unknown_part(p)
}

fn unknown_part(p: &str) -> Classification {
    Classification {
        part: p.to_string(),
        class: MatrixClass::Unknown,
        reason: "absent from 21_Office_Feature_Matrix — reject or preserve-only; never claim support (row 23)",
    }
}

/// Feature-level markers found inside a document part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocxFeature {
    FloatingDrawing,
    FieldOrToc,
    Equation,
    OleObject,
    AlternateContent,
}

pub fn classify_docx_feature(f: DocxFeature) -> Classification {
    let (marker, class, reason) = match f {
        DocxFeature::FloatingDrawing => (
            "document.xml#floatingDrawing",
            MatrixClass::PreserveOnly,
            "floating drawings / advanced anchoring — preserve-only (row 6)",
        ),
        DocxFeature::FieldOrToc => (
            "document.xml#fieldOrToc",
            MatrixClass::PreserveOnly,
            "fields, TOC — preserved not rendered (row 7)",
        ),
        DocxFeature::Equation => (
            "document.xml#equation",
            MatrixClass::PreserveOnly,
            "equations — preserved not rendered (row 7)",
        ),
        DocxFeature::OleObject => (
            "document.xml#oleObject",
            MatrixClass::PreserveNoExecute,
            "OLE objects — never execute (row 8)",
        ),
        DocxFeature::AlternateContent => (
            "document.xml#alternateContent",
            MatrixClass::PreserveOnly,
            "alternate content — preserved, mode not resolved (rows 6/7/23)",
        ),
    };
    Classification {
        part: marker.to_string(),
        class,
        reason,
    }
}

/// Build a compatibility report for a DOCX/XLSX/PPTX package: classify
/// every part, scan document XML for feature markers, and collect
/// unknown parts (row 23).
pub fn compatibility_report(
    format: OfficeFormat,
    bytes: &[u8],
) -> Result<CompatibilityReport, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("bad package: {e}"))?;
    let mut report = CompatibilityReport {
        format,
        ..Default::default()
    };
    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        names.push(
            archive
                .by_index(i)
                .map_err(|e| format!("bad package: {e}"))?
                .name()
                .to_string(),
        );
    }
    for name in &names {
        // Relationship/content-type plumbing is package infrastructure,
        // not a matrix feature.
        // Package infrastructure (content types, relationships, document
        // metadata) is not a matrix feature.
        if name == "[Content_Types].xml"
            || name == "_rels/.rels"
            || name.ends_with(".rels")
            || name.starts_with("docProps/")
        {
            continue;
        }
        report.entries.push(classify_part(format, name));
    }
    // Feature markers inside the main parts.
    let main_part = match format {
        OfficeFormat::Docx => Some("word/document.xml"),
        OfficeFormat::Xlsx => None,
        OfficeFormat::Pptx => None,
    };
    if let Some(part) = main_part {
        if let Ok(mut f) = archive.by_name(part) {
            let mut xml = String::new();
            let _ = f.read_to_string(&mut xml);
            if xml.contains("<wp:anchor") {
                report
                    .entries
                    .push(classify_docx_feature(DocxFeature::FloatingDrawing));
            }
            if xml.contains("fldChar") || xml.contains("instrText") {
                report
                    .entries
                    .push(classify_docx_feature(DocxFeature::FieldOrToc));
            }
            if xml.contains("oMath") {
                report
                    .entries
                    .push(classify_docx_feature(DocxFeature::Equation));
            }
            if xml.contains("w:object") || xml.contains("oleObject") {
                report
                    .entries
                    .push(classify_docx_feature(DocxFeature::OleObject));
            }
            if xml.contains("mc:AlternateContent") {
                report
                    .entries
                    .push(classify_docx_feature(DocxFeature::AlternateContent));
            }
        }
    }
    // Chart parts are REQUIRED_UNQUALIFIED; already classified by part.
    report.unknown_parts = report
        .entries
        .iter()
        .filter(|e| e.class == MatrixClass::Unknown)
        .map(|e| e.part.clone())
        .collect();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_content_never_executes_in_any_format() {
        for fmt in [OfficeFormat::Docx, OfficeFormat::Xlsx, OfficeFormat::Pptx] {
            let c = classify_part(fmt, "xl/vbaProject.bin");
            assert_eq!(c.class, MatrixClass::PreserveNoExecute);
            let c = classify_part(fmt, "ppt/embeddings/oleObject1.bin");
            assert_eq!(c.class, MatrixClass::PreserveNoExecute);
            let c = classify_part(fmt, "word/activeX/activeX1.xml");
            assert_eq!(c.class, MatrixClass::PreserveNoExecute);
        }
    }

    #[test]
    fn preserve_only_rows_classified() {
        let c = classify_part(OfficeFormat::Xlsx, "xl/pivotTables/pivotTable1.xml");
        assert_eq!(c.class, MatrixClass::PreserveOnly);
        let c = classify_part(OfficeFormat::Xlsx, "xl/externalLinks/externalLink1.xml");
        assert_eq!(c.class, MatrixClass::PreserveOnly);
        let c = classify_part(OfficeFormat::Xlsx, "xl/charts/chart1.xml");
        assert_eq!(c.class, MatrixClass::RequiredUnqualified);
    }

    #[test]
    fn unknown_part_is_reject_or_preserve_only() {
        let c = classify_part(
            OfficeFormat::Docx,
            "word/exoticWidget.bin".to_string().as_str(),
        );
        assert_eq!(c.class, MatrixClass::PreserveNoExecute); // .bin → security row
        let c = classify_part(OfficeFormat::Docx, "word/exoticWidget.xml");
        assert_eq!(c.class, MatrixClass::Unknown);
        let c = classify_part(OfficeFormat::Xlsx, "xl/timeMachine/pack1.xml");
        assert_eq!(c.class, MatrixClass::Unknown);
    }

    #[test]
    fn docx_feature_markers_classified() {
        assert_eq!(
            classify_docx_feature(DocxFeature::FloatingDrawing).class,
            MatrixClass::PreserveOnly
        );
        assert_eq!(
            classify_docx_feature(DocxFeature::FieldOrToc).class,
            MatrixClass::PreserveOnly
        );
        assert_eq!(
            classify_docx_feature(DocxFeature::OleObject).class,
            MatrixClass::PreserveNoExecute
        );
    }
}
