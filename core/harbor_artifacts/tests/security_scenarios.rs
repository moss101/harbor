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

/// SEC-032: a provider outcome that could not be established enters
/// OUTCOME_UNKNOWN and automatic retry is PROHIBITED — commit_external
/// refuses the batch until an operator/tool reconciles via recover().
#[test]
fn sec_032_outcome_unknown_prohibits_automatic_retry() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.xlsx");
    std::fs::write(&dest, b"base").unwrap();
    let base_hash = harbor_canonical::sha256_hex(b"base");
    let committer = SafeCommitter::new(dir.path().join("journal.db")).unwrap();
    // A crash between replace and finalize left the journal in
    // outcome_unknown (injected the way a crash would have written it).
    use harbor_artifacts::commit::CommitJournal;
    let journal = CommitJournal {
        batch_id: "batch-ou".into(),
        artifact_id: "a".into(),
        state: harbor_artifacts::commit::JournalState::OutcomeUnknown,
        mode: harbor_artifacts::commit::CommitMode::ProviderCompareAndSwap,
        base_content_hash: base_hash.clone(),
        proposed_output_hash: harbor_canonical::sha256_hex(b"out"),
        staging_path: None,
        target_identity: dest.to_string_lossy().to_string(),
        destination_identity: dest.to_string_lossy().to_string(),
        committed_version_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    committer.journal.upsert(&journal).unwrap();

    // Recovery says exactly what happened: unknown, human decision.
    let action = committer.recover("batch-ou").unwrap();
    assert_eq!(
        action,
        harbor_artifacts::commit::RecoveryAction::OutcomeUnknown
    );
    // Automatic retry is refused.
    let err = committer
        .commit_external(
            "batch-ou",
            "a",
            &dest,
            &base_hash,
            &harbor_canonical::sha256_hex(b"out"),
            b"out",
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            harbor_artifacts::commit::SafeCommitError::OutcomeUnknown
        ),
        "unknown outcomes must not retry automatically: {err}"
    );
}

/// SEC-042: macros/OLE/active content are classified PRESERVE_NO_EXECUTE
/// in every format — Harbor's preview and engine treat them as opaque
/// parts to preserve or warn about, and there is no execution path.
#[test]
fn sec_042_active_content_is_preserved_never_executed() {
    use harbor_artifacts::office_matrix::{classify_part, MatrixClass, OfficeFormat};
    for (format, part) in [
        (OfficeFormat::Xlsx, "xl/vbaProject.bin"),
        (OfficeFormat::Docx, "word/embeddings/oleObject1.bin"),
        (OfficeFormat::Pptx, "ppt/activeX/ax1.bin"),
        (OfficeFormat::Xlsx, "xl/bin/thing.bin"),
    ] {
        let c = classify_part(format, part);
        assert_eq!(
            c.class,
            MatrixClass::PreserveNoExecute,
            "{part} must classify as preserve-no-execute: {:?}",
            c.reason
        );
        assert!(
            c.reason.contains("never execute"),
            "the matrix says it outright: {}",
            c.reason
        );
    }
}

/// SEC-004 (external OOXML relationships): a package with an External
/// relationship (a linked image, a linked workbook) passes integrity
/// with the relationship MARKED external and its target never required
/// as a part — and Harbor has no fetch path for it (SEC-035's transport
/// scan forbids transport construction outside harbor_net, which the
/// artifact engine never touches).
#[test]
fn sec_004_external_relationships_are_marked_never_fetched() {
    // Build a minimal OOXML-shaped package: root rels with one internal
    // officeDocument target and one EXTERNAL target.
    let mut pkg = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    use std::io::Write;
    let rels = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://attacker.example/pixel.gif" TargetMode="External"/>
</Relationships>"#;
    pkg.start_file("_rels/.rels", zip::write::SimpleFileOptions::default())
        .unwrap();
    pkg.write_all(rels.as_bytes()).unwrap();
    pkg.start_file(
        "word/document.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    pkg.write_all(
        br#"<document xmlns="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#,
    )
    .unwrap();
    pkg.start_file(
        "[Content_Types].xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    pkg.write_all(
        br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
</Types>"#,
    )
    .unwrap();
    let bytes = pkg.finish().unwrap().into_inner();

    // The external target does NOT exist as a part, and integrity does
    // not demand it: external relationships are marked, never fetched.
    let problems = harbor_artifacts::package::package_integrity(&bytes);
    assert!(
        !problems.iter().any(|p| p.contains("attacker.example")),
        "an external target must not be treated as a missing part: {problems:?}"
    );
    assert!(
        problems.is_empty(),
        "the well-formed package with an external rel passes: {problems:?}"
    );

    // And there is no fetch path: the artifact engine constructs no
    // transport anywhere (SEC-035 enforces this core-wide; assert it
    // directly for this crate's sources too).
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let code = std::fs::read_to_string(&path).unwrap();
        let code = &code[..code.find("#[cfg(test)]").unwrap_or(code.len())];
        assert!(
            !code.contains("ureq::") && !code.contains("reqwest::"),
            "the artifact engine must not fetch: {}",
            path.display()
        );
    }
}

/// SEC-023 (unsafe overwrite): new-copy is the default and REFUSES an
/// existing destination; overwriting an original is a protected effect
/// that requires the exact approved base hash.
#[test]
fn sec_023_overwrite_is_protected_new_copy_is_default() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.xlsx");
    std::fs::write(&dest, b"precious-user-bytes").unwrap();
    let committer = SafeCommitter::new(dir.path().join("journal.db")).unwrap();
    let output = b"harbor-output".to_vec();
    let out_hash = harbor_canonical::sha256_hex(&output);

    // New-copy mode refuses to touch an existing destination.
    let err = committer
        .commit_new_copy("batch-nc", "a", &dest, &out_hash, &output)
        .unwrap_err();
    assert!(
        matches!(err, harbor_artifacts::commit::SafeCommitError::Conflict),
        "new-copy must refuse an existing destination: {err}"
    );
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        b"precious-user-bytes",
        "the original is untouched"
    );

    // The overwrite path is protected: only the exact approved base
    // hash opens it (a mismatched guess is refused).
    let wrong_base = harbor_canonical::sha256_hex(b"guess");
    let err = committer
        .commit_external("batch-ow", "a", &dest, &wrong_base, &out_hash, &output)
        .unwrap_err();
    assert!(matches!(
        err,
        harbor_artifacts::commit::SafeCommitError::BaseChanged { .. }
    ));
}

/// SEC-003 (OOXML path traversal): entry names that are absolute or
/// climb with `..` are rejected by integrity outright.
#[test]
fn sec_003_traversal_entry_names_rejected() {
    fn pkg_with_entry(name: &str) -> Vec<u8> {
        let mut pkg = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        use std::io::Write;
        pkg.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        pkg.write_all(b"x").unwrap();
        pkg.finish().unwrap().into_inner()
    }
    for bad in ["../escape.xml", "a/../../escape.xml", "/absolute.xml"] {
        let problems = harbor_artifacts::package::package_integrity(&pkg_with_entry(bad));
        assert!(
            problems.iter().any(|p| p.contains("traversal")),
            "{bad} must be rejected: {problems:?}"
        );
    }
}

/// SEC-002 (ZIP bomb): a part expanding beyond the compression-ratio
/// ceiling is refused before parsing (1 MiB of zeros Deflated to ~1 KiB
/// is >200x; a Stored-method package of the same body is 1x and passes).
#[test]
fn sec_002_zip_bomb_ratio_ceiling_enforced() {
    let mut pkg = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    use std::io::Write;
    // Control case: Stored method is 1x expansion.
    pkg.start_file(
        "bomb.xml",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    pkg.write_all(&vec![b'x'; 4096]).unwrap();
    let bytes = pkg.finish().unwrap().into_inner();
    // The bomb: 1 MiB of zeros, Deflated.
    let mut bomb = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    bomb.start_file(
        "zeros.xml",
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    bomb.write_all(&vec![0u8; 1024 * 1024]).unwrap();
    let bomb_bytes = bomb.finish().unwrap().into_inner();
    let problems = harbor_artifacts::package::package_integrity(&bomb_bytes);
    assert!(
        problems.iter().any(|p| p.contains("SEC-002")),
        "a >200x expansion must be refused: {problems:?}"
    );
    // The non-bomb package is fine on this axis.
    let ok_problems = harbor_artifacts::package::package_integrity(&bytes);
    assert!(!ok_problems.iter().any(|p| p.contains("SEC-002")));
}
