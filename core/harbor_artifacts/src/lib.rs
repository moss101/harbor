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
pub mod pptx;
pub mod workbook;

pub use batch::{ArtifactBatch, OpKind, Operation, Precondition};
pub use commit::{
    CommitJournal, CommitMode, CommitOutcome, SafeCommitError, SafeCommitter,
};
pub use diff::{ArtifactDiff, DiffEntry};
pub use docx::{DocxDocument, DocxOp};
pub use pptx::{PptxDeck, PptxOp, SlideContent};
pub use workbook::{SheetData, WorkbookDoc, WorkbookOp};
