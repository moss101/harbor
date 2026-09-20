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
pub mod commit;
pub mod diff;
pub mod docx;
pub mod office_matrix;
pub mod pptx;
pub mod workbook;

pub use batch::{ArtifactBatch, OpKind, Operation, Precondition};
pub use commit::{CommitJournal, CommitMode, CommitOutcome, SafeCommitError, SafeCommitter};
pub use diff::{ArtifactDiff, DiffEntry};
pub use docx::{DocxDocument, DocxError, DocxOp};
pub use office_matrix::{
    classify_part, compatibility_report, Classification, CompatibilityReport, MatrixClass,
    OfficeFormat,
};
pub use pptx::{ChartKind, ChartSpec, PptxDeck, PptxError, PptxOp, SlideContent, SlideImage};
pub use workbook::{PreservationReport, SheetData, WorkbookDoc, WorkbookOp, XlsxChartKind};

/// Inflate every entry of an OOXML package once, without keeping it, so a
/// corrupt compressed stream is a typed error before any reader that
/// might panic on it runs (fuzzing found the upstream workbook reader
/// aborting on a corrupt deflate stream). Bounded by the package itself:
/// each entry is streamed to a sink, never buffered.
pub(crate) fn inflate_probe<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("corrupt entry {i}: {e}"))?;
        let name = entry.name().to_string();
        std::io::copy(&mut entry, &mut std::io::sink())
            .map_err(|e| format!("corrupt entry {name}: {e}"))?;
    }
    Ok(())
}
