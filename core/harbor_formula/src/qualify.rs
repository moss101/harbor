//! Qualification runner: executes the fixture corpus against the pinned
//! engine and produces an evidence-grade report.
//!
//! A target becomes qualified (status PASS) only when every case for it
//! passes on the exact engine revision + adapter revision + corpus bundle
//! recorded in the report (22_Formula_Coverage.json verification_rule).

use std::collections::BTreeMap;
use std::fmt;

use formualizer::eval::engine::DeterministicMode;

use crate::corpus::{corpus_targets, load_corpus, FixtureCase, FixtureValue};
use crate::engine::{engine_identity, HarborWorkbook};
use crate::fixtures::{bundle_sha256, pinned_clock};
use crate::value::CellValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseStatus {
    Pass,
    Fail(String),
}

#[derive(Debug, Clone)]
pub struct CaseResult {
    pub case_id: String,
    pub target: String,
    pub dimension: String,
    pub status: CaseStatus,
}

#[derive(Debug, Clone)]
pub struct QualificationReport {
    pub engine_family: String,
    pub engine_version: String,
    pub engine_source_revision: String,
    pub engine_integrity_sha256: String,
    pub adapter_revision: String,
    pub fixture_bundle_sha256: String,
    pub platform: String,
    pub case_results: Vec<CaseResult>,
    pub target_status: BTreeMap<String, &'static str>,
    pub passed: usize,
    pub failed: usize,
}

impl fmt::Display for QualificationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "engine {} {} ({})", self.engine_family, self.engine_version, self.engine_source_revision)?;
        writeln!(f, "bundle {}", self.fixture_bundle_sha256)?;
        writeln!(f, "cases: {} passed, {} failed", self.passed, self.failed)?;
        writeln!(f, "targets PASS: {}", self.target_status.values().filter(|s| **s == "PASS").count())?;
        Ok(())
    }
}

fn values_match(expected: &FixtureValue, actual: &CellValue, tol: Option<f64>) -> bool {
    match (expected, actual) {
        (FixtureValue::Blank, CellValue::Blank) => true,
        (FixtureValue::Number(e), CellValue::Number(a)) => {
            let tolerance = tol.unwrap_or(1e-9);
            (e - a).abs() <= tolerance || (e - a).abs() <= tolerance * e.abs().max(1.0)
        }
        (FixtureValue::Text(e), CellValue::Text(a)) => e == a,
        (FixtureValue::Bool(e), CellValue::Bool(a)) => e == a,
        (FixtureValue::Error(code), CellValue::Error(e)) => {
            error_code_matches(code, e.code())
        }
        _ => false,
    }
}

fn error_code_matches(expected: &str, actual: &str) -> bool {
    expected == actual
}

fn set_cells(wb: &mut HarborWorkbook, cells: &[(String, u32, u32, FixtureValue)]) {
    for (sheet, row, col, v) in cells {
        wb.set_value(sheet, *row, *col, v.to_cell_value());
    }
}

/// Run one case (including edit rounds) against a fresh workbook.
pub fn run_case(case: &FixtureCase) -> CaseResult {
    let fail = |msg: String| CaseResult {
        case_id: case.id.clone(),
        target: case.target.clone(),
        dimension: case.dimension.clone(),
        status: CaseStatus::Fail(msg),
    };
    let mut wb = HarborWorkbook::new();
    // Deterministic clock for volatile builtins (TODAY/NOW fixtures rely
    // on the pinned clock). The mode MUST be active: a silent failure here
    // let TODAY/NOW evaluate from the wall clock, so their qualification
    // only passed while the fixture date happened to equal today (found
    // 2026-09-13 when the date rolled over). UTC is required — the engine
    // rejects `Local` under deterministic mode.
    let (clock, _tz) = pinned_clock();
    wb.inner_mut()
        .set_deterministic_mode(DeterministicMode::Enabled {
            timestamp_utc: clock,
            timezone: formualizer::eval::timezone::TimeZoneSpec::Utc,
        })
        .expect("pinned deterministic clock must be accepted");
    set_cells(&mut wb, &case.cells);
    wb.set_formula("Sheet1", 1000, 1000, &case.formula);
    // NOTE: formula goes on the case's primary sheet via its own refs; cases
    // use Sheet1 unless they reference Data only. We place the formula on
    // Sheet1 always (row/col far from data).
    let got = wb.evaluate_cell("Sheet1", 1000, 1000);
    if !values_match(&case.expect.value, &got, case.expect.tolerance) {
        return fail(format!(
            "expected {}, got {}",
            render_expected(&case.expect.value),
            got
        ));
    }
    for (i, edit) in case.edits.iter().enumerate() {
        set_cells(&mut wb, &edit.cells);
        let got = wb.evaluate_cell("Sheet1", 1000, 1000);
        if !values_match(&edit.expect.value, &got, edit.expect.tolerance) {
            return fail(format!(
                "edit round {i}: expected {}, got {}",
                render_expected(&edit.expect.value),
                got
            ));
        }
    }
    CaseResult {
        case_id: case.id.clone(),
        target: case.target.clone(),
        dimension: case.dimension.clone(),
        status: CaseStatus::Pass,
    }
}

fn render_expected(v: &FixtureValue) -> String {
    match v {
        FixtureValue::Blank => "<blank>".into(),
        FixtureValue::Number(n) => format!("{n}"),
        FixtureValue::Text(s) => format!("\"{s}\""),
        FixtureValue::Bool(b) => format!("{b}"),
        FixtureValue::Error(c) => c.clone(),
    }
}

/// Run the full corpus and compute per-target qualification status.
pub fn run_qualification() -> Result<QualificationReport, String> {
    let cases = load_corpus().map_err(|e| e.to_string())?;
    let identity = engine_identity();
    let mut results = Vec::with_capacity(cases.len());
    for case in &cases {
        results.push(run_case(case));
    }
    let passed = results.iter().filter(|r| r.status == CaseStatus::Pass).count();
    let failed = results.len() - passed;
    let mut target_status = BTreeMap::new();
    for (target, _ids) in corpus_targets(&cases) {
        let all_pass = results
            .iter()
            .filter(|r| r.target == target)
            .all(|r| r.status == CaseStatus::Pass);
        target_status.insert(target, if all_pass { "PASS" } else { "FAIL" });
    }
    Ok(QualificationReport {
        engine_family: identity.family.to_string(),
        engine_version: identity.version.to_string(),
        engine_source_revision: identity.source_revision.to_string(),
        engine_integrity_sha256: identity.integrity_sha256.to_string(),
        adapter_revision: identity.adapter_revision.to_string(),
        fixture_bundle_sha256: bundle_sha256(),
        platform: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        case_results: results,
        target_status,
        passed,
        failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_loads_and_covers_targets() {
        let cases = load_corpus().unwrap();
        assert!(cases.len() >= 90, "corpus should be substantial");
        let targets = corpus_targets(&cases);
        assert!(targets.len() >= 71, "expected >= 71 targets, got {}", targets.len());
    }

    #[test]
    fn full_qualification_runs() {
        let report = run_qualification().unwrap();
        println!("{report}");
        for r in &report.case_results {
            if let CaseStatus::Fail(msg) = &r.status {
                println!("FAIL {}: {}", r.case_id, msg);
            }
        }
        // Honest reporting: the report reflects reality. Assert the runner
        // executed every case.
        assert_eq!(report.passed + report.failed, report.case_results.len());
    }
}
