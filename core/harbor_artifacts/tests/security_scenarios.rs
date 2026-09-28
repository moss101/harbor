//! Security scenarios SEC-033, SEC-043 and SEC-041
//! (09_Security_Test_Matrix.csv) as executable controls:
//! - `security.sec_033` — artifact TOCTOU overwrite: an external write
//!   after preview, inside the check-to-replace interval, is rejected by
//!   the content-hash fence; an uncoordinated destination is never
//!   overwritten (a new copy is created instead).
//! - `security.sec_043` — artifact commit interruption: replaying a
//!   committed batch is idempotent (same version, no duplicate copy),
//!   and recovery reconciles recorded identity without overwriting a
//!   later external edit.
//! - `security.sec_041` — a workbook carrying deliberately STALE cached
//!   formula values cannot serve a verified numerical claim: Harbor's
//!   recalculation path recomputes through the pinned engine and the
//!   engine's values are what comes out.

use harbor_artifacts::commit::SafeCommitter;
use harbor_artifacts::workbook::WorkbookDoc;

/// SEC-033 part 1: the destination changed between preview (approval
/// bound base_content_hash) and the final check-to-replace — the
/// conditional replace refuses; the external edit stands.
#[test]
fn sec_033_external_write_in_commit_window_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("report.xlsx");
    let original = b"original-approved-bytes".to_vec();
    std::fs::write(&dest, &original).unwrap();
    let base_hash = harbor_canonical::sha256_hex(&original);

    let committer = SafeCommitter::new(dir.path().join("journal.db")).unwrap();
    let output = b"harbor-rendered-output".to_vec();
    let out_hash = harbor_canonical::sha256_hex(&output);

    // The external write lands INSIDE the window: after the approval
    // bound the base hash, before the replace.
    std::fs::write(&dest, b"externally-rewritten").unwrap();

    let err = committer
        .commit_external(
            "batch-1",
            "artifact-1",
            &dest,
            &base_hash,
            &out_hash,
            &output,
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            harbor_artifacts::commit::SafeCommitError::BaseChanged { .. }
        ),
        "a stale target must be refused, never replaced: {err}"
    );
    // The external edit stands untouched.
    assert_eq!(std::fs::read(&dest).unwrap(), b"externally-rewritten");
}

/// SEC-033 part 2: the uncoordinated case lands as a NEW COPY — the
/// original is never overwritten outside the fenced path.
#[test]
fn sec_033_uncoordinated_change_becomes_a_new_copy() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("report.xlsx");
    let original = b"original-approved-bytes".to_vec();
    std::fs::write(&dest, &original).unwrap();
    let base_hash = harbor_canonical::sha256_hex(&original);

    let committer = SafeCommitter::new(dir.path().join("journal.db")).unwrap();
    let output = b"harbor-rendered-output".to_vec();
    let out_hash = harbor_canonical::sha256_hex(&output);
    // The external write happens first; Harbor saves a NEW copy beside
    // it (the fenced overwrite path is for a matching base only).
    std::fs::write(&dest, b"externally-rewritten").unwrap();
    let new_dest = dir.path().join("report (Harbor copy).xlsx");
    let outcome = committer
        .commit_new_copy("batch-2", "artifact-2", &new_dest, &out_hash, &output)
        .unwrap();
    match outcome {
        harbor_artifacts::commit::CommitOutcome::CopiedNew { destination, .. } => {
            assert_eq!(destination, new_dest);
            assert_eq!(std::fs::read(&destination).unwrap(), output);
        }
        other => panic!("new-copy mode must copy, got {other:?}"),
    }
    // The externally edited original stands.
    assert_eq!(std::fs::read(&dest).unwrap(), b"externally-rewritten");
    // And the fenced path against the rewritten original still refuses.
    let err = committer
        .commit_external(
            "batch-2b",
            "artifact-2",
            &dest,
            &base_hash,
            &out_hash,
            &output,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        harbor_artifacts::commit::SafeCommitError::BaseChanged { .. }
    ));
}

/// SEC-043 part 1: replaying a committed batch (the crash-after-commit
/// retry) is idempotent — same version id, no second write, and a
/// destination that later changed again is a CONFLICT, never a silent
/// success.
#[test]
fn sec_043_replayed_commit_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sheet.xlsx");
    let original = b"base-bytes".to_vec();
    std::fs::write(&dest, &original).unwrap();
    let base_hash = harbor_canonical::sha256_hex(&original);

    let committer = SafeCommitter::new(dir.path().join("journal.db")).unwrap();
    let output = b"approved-output".to_vec();
    let out_hash = harbor_canonical::sha256_hex(&output);
    let first = committer
        .commit_external(
            "batch-3",
            "artifact-3",
            &dest,
            &base_hash,
            &out_hash,
            &output,
        )
        .unwrap();
    let version = match &first {
        harbor_artifacts::commit::CommitOutcome::Committed { version_id, .. } => version_id.clone(),
        other => panic!("expected Committed, got {other:?}"),
    };

    // Crash-retry of the same batch: idempotent while the destination
    // still holds the approved output.
    let replay = committer
        .commit_external(
            "batch-3",
            "artifact-3",
            &dest,
            &base_hash,
            &out_hash,
            &output,
        )
        .unwrap();
    match replay {
        harbor_artifacts::commit::CommitOutcome::Committed {
            version_id,
            bytes_written,
        } => {
            assert_eq!(version_id, version, "same version on replay");
            assert_eq!(bytes_written, 0, "no bytes written twice");
        }
        other => panic!("replay must be idempotent, got {other:?}"),
    }

    // A LATER external edit after the commit: the replay is a conflict,
    // never a silent overwrite of someone else's newer work.
    std::fs::write(&dest, b"user-edited-after-commit").unwrap();
    let err = committer
        .commit_external(
            "batch-3",
            "artifact-3",
            &dest,
            &base_hash,
            &out_hash,
            &output,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        harbor_artifacts::commit::SafeCommitError::Conflict
    ));
}

/// SEC-043 part 2: recovery reconciles a committed batch's identity
/// without duplicating or overwriting.
#[test]
fn sec_043_recovery_reconciles_committed_identity() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sheet.xlsx");
    let original = b"base-bytes".to_vec();
    std::fs::write(&dest, &original).unwrap();
    let base_hash = harbor_canonical::sha256_hex(&original);
    let committer = SafeCommitter::new(dir.path().join("journal.db")).unwrap();
    let output = b"approved-output".to_vec();
    let out_hash = harbor_canonical::sha256_hex(&output);
    committer
        .commit_external(
            "batch-4",
            "artifact-4",
            &dest,
            &base_hash,
            &out_hash,
            &output,
        )
        .unwrap();
    let action = committer.recover("batch-4").unwrap();
    match action {
        harbor_artifacts::commit::RecoveryAction::AlreadyCommitted { version_id } => {
            assert!(!version_id.is_empty());
        }
        other => panic!("recovery of a committed batch must reconcile, got {other:?}"),
    }
}

/// SEC-041: a workbook with a deliberately STALE cached formula value
/// (the hostile-file shape) loads with the stale cache visible — and
/// Harbor's recalculation recomputes through the pinned engine, so the
/// verified value is the engine's, never the cache's.
#[test]
fn sec_041_stale_cached_values_are_recalculated_not_trusted() {
    // Build a real workbook: A1=10, A2=32, A3=SUM(A1:A2) (cached 42).
    let mut doc = WorkbookDoc::new_empty();
    doc.add_sheet("Sheet1").unwrap();
    use harbor_formula::value::CellValue;
    doc.put_cell(
        "Sheet1",
        1,
        1,
        harbor_artifacts::workbook::CellSet::Value(CellValue::Number(10.0)),
    )
    .unwrap();
    doc.put_cell(
        "Sheet1",
        2,
        1,
        harbor_artifacts::workbook::CellSet::Value(CellValue::Number(32.0)),
    )
    .unwrap();
    doc.put_cell(
        "Sheet1",
        3,
        1,
        harbor_artifacts::workbook::CellSet::Formula("SUM(A1:A2)".into()),
    )
    .unwrap();
    doc.recalculate_all().unwrap();
    let mut bytes = doc.to_bytes().unwrap();

    // Corrupt the cached value inside the package: the formula cell's
    // <v> becomes 999 — exactly what a stale or hostile file looks like.
    bytes = corrupt_formula_cache(&bytes, "SUM(A1:A2)", "999");

    // Load: the reader is honest about what the file says — the stale
    // cached 999 IS visible...
    let stale = WorkbookDoc::load(&bytes).unwrap();
    let sheet = stale.sheet("Sheet1").unwrap();
    let a3 = &sheet.cells[&(1, 3)];
    assert!(a3.formula.is_some(), "the formula is present");
    assert_eq!(
        a3.cached.clone().expect("cache present"),
        CellValue::Number(999.0),
        "corruption landed"
    );

    // The verified numerical value comes from recalculation through the
    // pinned engine: 42, never the stale 999 the package carries.
    let mut verified = WorkbookDoc::load(&bytes).unwrap();
    let values = verified.recalculate_all().unwrap();
    let a3_verified = values
        .get(&("Sheet1".to_string(), 3, 1))
        .expect("recalculated A3");
    assert_eq!(*a3_verified, CellValue::Number(42.0));
}

/// Rewrite the xlsx package so the cell carrying `formula_text` has
/// `<v>stale</v>` as its cached value.
fn corrupt_formula_cache(bytes: &[u8], formula_text: &str, stale: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut reader = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..reader.len() {
        let mut file = reader.by_index(i).unwrap();
        let name = file.name().to_string();
        if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") {
            let mut xml = String::new();
            file.read_to_string(&mut xml).unwrap();
            if xml.contains(formula_text) {
                // The formula cell: replace its cached <v>…</v>.
                let start = xml.find("<f>").unwrap();
                let v_start = xml[start..].find("<v>").unwrap() + start;
                let v_end = xml[start..].find("</v>").unwrap() + start;
                xml.replace_range(v_start..v_end + 4, &format!("<v>{stale}</v>"));
            }
            writer
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(xml.as_bytes()).unwrap();
        } else {
            writer
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            let mut content = Vec::new();
            file.read_to_end(&mut content).unwrap();
            writer.write_all(&content).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}
