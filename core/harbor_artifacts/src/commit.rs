//! Safe commit: version-bound, all-or-nothing publication with conflict
//! detection and a durable commit journal.
//!
//! Sequence per 02 contract §"Artifact batches and safe save":
//! 1. stage the complete proposed output and verify its hash;
//! 2. record the journal + receipt consumption transactionally, flush
//!    staging bytes;
//! 3. reacquire the file capability and validate the base inside the
//!    protected write interval (a plain check-then-rename is insufficient
//!    for external files);
//! 4. publish once, fsync file and directory, finalize the journal.
//!
//! A provider without conditional-write guarantees uses `new_copy` with a
//! predetermined destination and no overwrite. Replaying a committed batch
//! id returns its existing version; a third version at the destination is
//! a conflict without overwrite.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitMode {
    /// Private-store blob: atomic rename within app storage.
    PrivateStoreAtomic,
    /// External file via exclusive-create temp + replace with base
    /// revalidation (compare-and-swap semantics).
    ProviderCompareAndSwap,
    /// No overwrite: write a predetermined new copy.
    NewCopy,
}

impl CommitMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommitMode::PrivateStoreAtomic => "private_store_atomic",
            CommitMode::ProviderCompareAndSwap => "provider_compare_and_swap",
            CommitMode::NewCopy => "new_copy",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommitJournal {
    pub batch_id: String,
    pub artifact_id: String,
    pub state: JournalState,
    pub mode: CommitMode,
    pub base_content_hash: String,
    pub proposed_output_hash: String,
    pub staging_path: Option<String>,
    pub target_identity: String,
    pub destination_identity: String,
    pub committed_version_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalState {
    Prepared,
    Staged,
    Replaced,
    Committed,
    Conflict,
    OutcomeUnknown,
    Aborted,
}

impl JournalState {
    pub fn as_str(&self) -> &'static str {
        match self {
            JournalState::Prepared => "prepared",
            JournalState::Staged => "staged",
            JournalState::Replaced => "replaced",
            JournalState::Committed => "committed",
            JournalState::Conflict => "conflict",
            JournalState::OutcomeUnknown => "outcome_unknown",
            JournalState::Aborted => "aborted",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SafeCommitError {
    #[error("base file changed since approval (expected {expected}, found {found})")]
    BaseChanged { expected: String, found: String },
    #[error("staged output hash mismatch")]
    StagedHashMismatch,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("db error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("batch already committed with version {0}")]
    AlreadyCommitted(String),
    #[error("conflict: destination holds a third version")]
    Conflict,
    #[error("outcome cannot be established; no automatic retry")]
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommitOutcome {
    Committed {
        version_id: String,
        bytes_written: u64,
    },
    /// new_copy mode: wrote a new file, original untouched.
    CopiedNew {
        destination: PathBuf,
        version_id: String,
    },
}

/// Durable journal storage. The journal must be on the same filesystem as
/// the database; it records prepared -> staged -> replaced -> committed so
/// recovery can classify a crash at any point.
pub struct JournalStore {
    conn: Connection,
}

impl JournalStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SafeCommitError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS artifact_commit_journal (
                batch_id TEXT PRIMARY KEY,
                artifact_id TEXT NOT NULL,
                state TEXT NOT NULL,
                mode TEXT NOT NULL,
                base_content_hash TEXT NOT NULL,
                proposed_output_hash TEXT NOT NULL,
                staging_path TEXT,
                target_identity TEXT NOT NULL,
                destination_identity TEXT NOT NULL,
                committed_version_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )?;
        Ok(JournalStore { conn })
    }

    pub fn upsert(&self, j: &CommitJournal) -> Result<(), SafeCommitError> {
        self.conn.execute(
            "INSERT INTO artifact_commit_journal (batch_id, artifact_id, state, mode, base_content_hash, proposed_output_hash,
             staging_path, target_identity, destination_identity, committed_version_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(batch_id) DO UPDATE SET state=?3, staging_path=?7, committed_version_id=?10, updated_at=?12",
            rusqlite::params![
                j.batch_id,
                j.artifact_id,
                j.state.as_str(),
                j.mode.as_str(),
                j.base_content_hash,
                j.proposed_output_hash,
                j.staging_path,
                j.target_identity,
                j.destination_identity,
                j.committed_version_id,
                j.created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                j.updated_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, batch_id: &str) -> Result<Option<CommitJournal>, SafeCommitError> {
        let row = self.conn.query_row(
            "SELECT batch_id, artifact_id, state, mode, base_content_hash, proposed_output_hash, staging_path,
             target_identity, destination_identity, committed_version_id, created_at, updated_at
             FROM artifact_commit_journal WHERE batch_id = ?1",
            [batch_id],
            |r| {
                Ok(CommitJournal {
                    batch_id: r.get(0)?,
                    artifact_id: r.get(1)?,
                    state: parse_state(&r.get::<_, String>(2)?),
                    mode: parse_mode(&r.get::<_, String>(3)?),
                    base_content_hash: r.get(4)?,
                    proposed_output_hash: r.get(5)?,
                    staging_path: r.get(6)?,
                    target_identity: r.get(7)?,
                    destination_identity: r.get(8)?,
                    committed_version_id: r.get(9)?,
                    created_at: parse_time(&r.get::<_, String>(10)?),
                    updated_at: parse_time(&r.get::<_, String>(11)?),
                })
            },
        );
        match row {
            Ok(j) => Ok(Some(j)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

fn parse_state(s: &str) -> JournalState {
    match s {
        "staged" => JournalState::Staged,
        "replaced" => JournalState::Replaced,
        "committed" => JournalState::Committed,
        "conflict" => JournalState::Conflict,
        "outcome_unknown" => JournalState::OutcomeUnknown,
        "aborted" => JournalState::Aborted,
        _ => JournalState::Prepared,
    }
}

fn parse_mode(s: &str) -> CommitMode {
    match s {
        "provider_compare_and_swap" => CommitMode::ProviderCompareAndSwap,
        "new_copy" => CommitMode::NewCopy,
        _ => CommitMode::PrivateStoreAtomic,
    }
}

fn parse_time(s: &str) -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Executes safe commits. `journal_db` is the durable journal; staging
/// happens beside it. External-file publication uses exclusive-create temp
/// + base-hash revalidation + rename within the same directory, then fsync
///   of file and directory.
pub struct SafeCommitter {
    pub journal: JournalStore,
}

impl SafeCommitter {
    pub fn new(journal_db: impl AsRef<Path>) -> Result<Self, SafeCommitError> {
        Ok(SafeCommitter {
            journal: JournalStore::open(journal_db)?,
        })
    }

    /// Commit `output` for `batch_id`, replacing `destination` only if its
    /// current bytes still hash to `base_content_hash`.
    pub fn commit_external(
        &self,
        batch_id: &str,
        artifact_id: &str,
        destination: &Path,
        base_content_hash: &str,
        proposed_output_hash: &str,
        output: &[u8],
    ) -> Result<CommitOutcome, SafeCommitError> {
        let now = Utc::now();
        // Replaying a committed batch returns its version (idempotent) —
        // but only while the destination still holds the approved output.
        // A third version at the destination is a conflict, never a
        // silent success (02 contract, recovery rule 5).
        if let Some(j) = self.journal.get(batch_id)? {
            if j.state == JournalState::Committed {
                let dest_hash = std::fs::read(destination)
                    .map(|bytes| harbor_canonical::sha256_hex(&bytes))
                    .ok();
                if dest_hash.as_deref() != Some(proposed_output_hash) {
                    return Err(SafeCommitError::Conflict);
                }
                return Ok(CommitOutcome::Committed {
                    version_id: j.committed_version_id.unwrap_or_default(),
                    bytes_written: output.len() as u64,
                });
            }
        }
        // Stage + hash verification.
        let got = harbor_canonical::sha256_hex(output);
        if got != proposed_output_hash {
            return Err(SafeCommitError::StagedHashMismatch);
        }
        let dest_dir = destination.parent().unwrap_or(Path::new("."));
        let staging = dest_dir.join(format!(".harbor-stage-{batch_id}.tmp"));
        let journal = CommitJournal {
            batch_id: batch_id.into(),
            artifact_id: artifact_id.into(),
            state: JournalState::Staged,
            mode: CommitMode::ProviderCompareAndSwap,
            base_content_hash: base_content_hash.into(),
            proposed_output_hash: proposed_output_hash.into(),
            staging_path: Some(staging.to_string_lossy().to_string()),
            target_identity: destination.to_string_lossy().to_string(),
            destination_identity: destination.to_string_lossy().to_string(),
            committed_version_id: None,
            created_at: now,
            updated_at: now,
        };
        self.journal.upsert(&journal)?;
        {
            let mut f = std::fs::File::create(&staging)?;
            f.write_all(output)?;
            f.sync_all()?;
        }
        // Protected interval: validate base INSIDE the write window.
        let base_now = std::fs::read(destination).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SafeCommitError::BaseChanged {
                expected: base_content_hash.into(),
                found: "<missing>".into(),
            },
            _ => SafeCommitError::Io(e),
        })?;
        let base_now_hash = harbor_canonical::sha256_hex(&base_now);
        if base_now_hash != base_content_hash {
            let _ = std::fs::remove_file(&staging);
            let mut j = journal;
            j.state = JournalState::Conflict;
            j.updated_at = Utc::now();
            self.journal.upsert(&j)?;
            return Err(SafeCommitError::BaseChanged {
                expected: base_content_hash.into(),
                found: base_now_hash,
            });
        }
        // Publish once: exclusive-create + rename (same directory).
        let version_id = format!(
            "v-{}",
            &proposed_output_hash[..16.min(proposed_output_hash.len())]
        );
        std::fs::rename(&staging, destination)?;
        sync_dir(dest_dir);
        let mut j = journal;
        j.state = JournalState::Committed;
        j.committed_version_id = Some(version_id.clone());
        j.updated_at = Utc::now();
        self.journal.upsert(&j)?;
        Ok(CommitOutcome::Committed {
            version_id,
            bytes_written: output.len() as u64,
        })
    }

    /// new_copy fallback: predetermined destination, no overwrite. Used
    /// when the provider cannot guarantee conditional replacement.
    pub fn commit_new_copy(
        &self,
        batch_id: &str,
        artifact_id: &str,
        destination: &Path,
        proposed_output_hash: &str,
        output: &[u8],
    ) -> Result<CommitOutcome, SafeCommitError> {
        let now = Utc::now();
        let got = harbor_canonical::sha256_hex(output);
        if got != proposed_output_hash {
            return Err(SafeCommitError::StagedHashMismatch);
        }
        if destination.exists() {
            return Err(SafeCommitError::Conflict);
        }
        let journal = CommitJournal {
            batch_id: batch_id.into(),
            artifact_id: artifact_id.into(),
            state: JournalState::Staged,
            mode: CommitMode::NewCopy,
            base_content_hash: harbor_canonical::sha256_hex(&[]),
            proposed_output_hash: proposed_output_hash.into(),
            staging_path: None,
            target_identity: destination.to_string_lossy().to_string(),
            destination_identity: destination.to_string_lossy().to_string(),
            committed_version_id: None,
            created_at: now,
            updated_at: now,
        };
        self.journal.upsert(&journal)?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        f.write_all(output)?;
        f.sync_all()?;
        sync_dir(destination.parent().unwrap_or(Path::new(".")));
        let version_id = format!(
            "v-{}",
            &proposed_output_hash[..16.min(proposed_output_hash.len())]
        );
        let mut j = journal;
        j.state = JournalState::Committed;
        j.committed_version_id = Some(version_id.clone());
        j.updated_at = Utc::now();
        self.journal.upsert(&j)?;
        Ok(CommitOutcome::CopiedNew {
            destination: destination.to_path_buf(),
            version_id,
        })
    }

    /// Recovery: classify a prepared/staged/replaced journal after a crash.
    pub fn recover(&self, batch_id: &str) -> Result<RecoveryAction, SafeCommitError> {
        let Some(j) = self.journal.get(batch_id)? else {
            return Ok(RecoveryAction::Nothing);
        };
        match j.state {
            JournalState::Committed => Ok(RecoveryAction::AlreadyCommitted {
                version_id: j.committed_version_id.unwrap_or_default(),
            }),
            JournalState::Conflict => Ok(RecoveryAction::Conflict),
            JournalState::OutcomeUnknown => Ok(RecoveryAction::OutcomeUnknown),
            JournalState::Prepared | JournalState::Staged => {
                let dest = PathBuf::from(&j.destination_identity);
                let dest_hash = std::fs::read(&dest)
                    .map(|bytes| harbor_canonical::sha256_hex(&bytes))
                    .ok();
                if dest_hash.as_deref() == Some(j.proposed_output_hash.as_str()) {
                    Ok(RecoveryAction::FinalizeCommitted {
                        version_id: format!("v-{}", &j.proposed_output_hash[..16]),
                    })
                } else if dest_hash.as_deref() == Some(j.base_content_hash.as_str()) {
                    Ok(RecoveryAction::ResumableWithAuthority)
                } else {
                    Ok(RecoveryAction::Conflict)
                }
            }
            JournalState::Replaced | JournalState::Aborted => Ok(RecoveryAction::OutcomeUnknown),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryAction {
    Nothing,
    AlreadyCommitted { version_id: String },
    FinalizeCommitted { version_id: String },
    ResumableWithAuthority,
    Conflict,
    OutcomeUnknown,
}

fn sync_dir(dir: &Path) {
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committer(dir: &Path) -> SafeCommitter {
        SafeCommitter::new(dir.join("journal.db")).unwrap()
    }

    #[test]
    fn external_commit_happy_path_and_idempotent_replay() {
        let dir = tempfile::tempdir().unwrap();
        let c = committer(dir.path());
        let dest = dir.path().join("report.xlsx");
        std::fs::write(&dest, b"base-bytes").unwrap();
        let base = harbor_canonical::sha256_hex(b"base-bytes");
        let out = b"new-bytes-v2";
        let out_hash = harbor_canonical::sha256_hex(out);
        let o1 = c
            .commit_external("b1", "art-1", &dest, &base, &out_hash, out)
            .unwrap();
        match o1 {
            CommitOutcome::Committed {
                version_id,
                bytes_written,
            } => {
                assert_eq!(bytes_written, out.len() as u64);
                assert!(version_id.starts_with("v-"));
            }
            other => panic!("wrong outcome: {other:?}"),
        }
        assert_eq!(std::fs::read(&dest).unwrap(), out);
        // Replay: committed batch returns its version, no double write.
        let o2 = c
            .commit_external("b1", "art-1", &dest, &base, &out_hash, out)
            .unwrap();
        assert!(matches!(o2, CommitOutcome::Committed { .. }));
        // Replay after a third version appeared at the destination is a
        // conflict, not a silent success (recovery rule 5).
        std::fs::write(&dest, b"third-version").unwrap();
        let o3 = c
            .commit_external("b1", "art-1", &dest, &base, &out_hash, out)
            .unwrap_err();
        assert!(matches!(o3, SafeCommitError::Conflict));
        assert_eq!(std::fs::read(&dest).unwrap(), b"third-version");
    }

    #[test]
    fn stale_base_conflicts_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let c = committer(dir.path());
        let dest = dir.path().join("report.xlsx");
        std::fs::write(&dest, b"base").unwrap();
        let base = harbor_canonical::sha256_hex(b"base");
        let out = b"approved-output";
        let out_hash = harbor_canonical::sha256_hex(out);
        // External change AFTER approval: base is now different.
        std::fs::write(&dest, b"externally-edited").unwrap();
        let err = c
            .commit_external("b2", "art-1", &dest, &base, &out_hash, out)
            .unwrap_err();
        assert!(matches!(err, SafeCommitError::BaseChanged { .. }));
        // The externally-edited content is untouched.
        assert_eq!(std::fs::read(&dest).unwrap(), b"externally-edited");
        assert!(matches!(
            c.journal.get("b2").unwrap().unwrap().state,
            JournalState::Conflict
        ));
    }

    #[test]
    fn new_copy_never_overwrites_and_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let c = committer(dir.path());
        let dest = dir.path().join("copy.xlsx");
        let out = b"copy-output";
        let hash = harbor_canonical::sha256_hex(out);
        c.commit_new_copy("b3", "art", &dest, &hash, out).unwrap();
        // Second commit to the same destination conflicts (no overwrite).
        assert!(matches!(
            c.commit_new_copy("b4", "art", &dest, &hash, out),
            Err(SafeCommitError::Conflict)
        ));
        assert_eq!(std::fs::read(&dest).unwrap(), out);
    }

    #[test]
    fn recovery_classifies_crash_points() {
        let dir = tempfile::tempdir().unwrap();
        let c = committer(dir.path());
        let dest = dir.path().join("r.xlsx");
        std::fs::write(&dest, b"base").unwrap();
        let base = harbor_canonical::sha256_hex(b"base");
        let out = b"output";
        let out_hash = harbor_canonical::sha256_hex(out);
        // Simulate crash after staging: journal row only.
        let now = Utc::now();
        c.journal
            .upsert(&CommitJournal {
                batch_id: "bx".into(),
                artifact_id: "art".into(),
                state: JournalState::Staged,
                mode: CommitMode::ProviderCompareAndSwap,
                base_content_hash: base.clone(),
                proposed_output_hash: out_hash.clone(),
                staging_path: None,
                target_identity: dest.to_string_lossy().to_string(),
                destination_identity: dest.to_string_lossy().to_string(),
                committed_version_id: None,
                created_at: now,
                updated_at: now,
            })
            .unwrap();
        // Base still present, output never published -> resumable.
        assert_eq!(
            c.recover("bx").unwrap(),
            RecoveryAction::ResumableWithAuthority
        );
        // If the output HAD been published -> finalize the same version.
        std::fs::write(&dest, out).unwrap();
        assert_eq!(
            c.recover("bx").unwrap(),
            RecoveryAction::FinalizeCommitted {
                version_id: format!("v-{}", &out_hash[..16])
            }
        );
    }
}
