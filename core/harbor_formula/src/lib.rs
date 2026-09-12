//! harbor_formula — the pinned Harbor spreadsheet calculation adapter.
//!
//! Authority: `22_Formula_Coverage.json`, `03_Architecture_Contracts.md` §9,
//! `21_Office_Feature_Matrix.csv` (XLSX Calculate rows).
//!
//! - Engine: Formualizer, pinned at 0.9.3 (license review: MIT OR Apache-2.0 —
//!   approved). The engine revision and integrity identity are recorded in
//!   [`engine_identity`] and stamped on every qualification result.
//! - Cached workbook values are never trusted for verified numerical
//!   conclusions: a value is "verified" only when the pinned engine
//!   recomputes it from inputs within this adapter's qualified function set.
//! - Unsupported functions are preserved and reported, never silently
//!   treated as verified.

pub mod corpus;
pub mod engine;
pub mod fixtures;
pub mod qualify;
pub mod value;

pub use corpus::{FixtureCase, FixtureEdit, FixtureExpectation, FixtureValue};
pub use engine::{EngineIdentity, HarborWorkbook};
pub use value::{RecalcCell, RecalcStatus};
pub use qualify::{QualificationReport, run_qualification};
pub use value::{CellError, CellValue};
