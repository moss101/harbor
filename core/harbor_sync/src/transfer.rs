//! Run transfer handshake (11_Sync_Protocol.md "Transfer handshake").
//!
//! Execution transfer requires an online coordinator plus a durable source
//! acknowledgement: the run STOPPED, its dispatch authority was revoked,
//! and its outgoing effects are settled or explicitly outcome_unknown.
//! The destination gets a new transfer generation only after that
//! acknowledgement commits. Crash/partition retries REUSE the transfer ID.

use chrono::{DateTime, Utc};

pub const TRANSFER_ACK_TIMEOUT_DAYS: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStatus {
    /// Source run stopped; dispatch authority revoked; effects settled or
    /// explicitly outcome_unknown. Persisted durably at the source.
    SourceAcknowledged,
    /// Destination acquired local authority + fresh receipts.
    DestinationReady,
    /// Both devices persisted transfer ID + generation.
    Completed,
}

#[derive(Debug, Clone)]
pub struct TransferRecord {
    pub transfer_id: String,
    pub run_id: String,
    pub generation: u64,
    pub state: TransferState,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferState {
    AwaitingSourceAck,
    SourceAcknowledged,
    DestinationReady,
    Completed,
}

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("source cannot acknowledge: run still active or effects unsettled")]
    SourceCannotAcknowledge,
    #[error("transfer {0} is completed and immutable")]
    Completed(String),
}

/// Coordinator persisted on BOTH devices (transfer ID + generation before
/// acknowledging completion). Retries after crash/partition reuse the
/// transfer ID.
pub struct TransferCoordinator {
    pub records: std::collections::BTreeMap<String, TransferRecord>,
}

impl Default for TransferCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferCoordinator {
    pub fn new() -> Self {
        TransferCoordinator {
            records: Default::default(),
        }
    }

    /// Begin (or re-begin after crash) a transfer. The transfer ID is
    /// stable across retries.
    pub fn begin(
        &mut self,
        transfer_id: &str,
        run_id: &str,
        now: DateTime<Utc>,
    ) -> Result<&TransferRecord, TransferError> {
        if let Some(existing) = self.records.get(transfer_id) {
            if existing.state == TransferState::Completed {
                return Err(TransferError::Completed(transfer_id.into()));
            }
            return Ok(self.records.get(transfer_id).unwrap());
        }
        self.records.insert(
            transfer_id.into(),
            TransferRecord {
                transfer_id: transfer_id.into(),
                run_id: run_id.into(),
                generation: 1,
                state: TransferState::AwaitingSourceAck,
                updated_at: now,
            },
        );
        Ok(self.records.get(transfer_id).unwrap())
    }

    /// Record the durable SOURCE acknowledgement. The source asserts: run
    /// stopped, dispatch authority revoked, effects settled or explicitly
    /// outcome_unknown. Any unsettled non-unknown effect blocks transfer.
    pub fn acknowledge_source(
        &mut self,
        transfer_id: &str,
        effects: &[EffectOutcome],
        now: DateTime<Utc>,
    ) -> Result<TransferStatus, TransferError> {
        let rec = self
            .records
            .get_mut(transfer_id)
            .ok_or(TransferError::SourceCannotAcknowledge)?;
        if rec.state == TransferState::Completed {
            return Err(TransferError::Completed(transfer_id.into()));
        }
        if effects.contains(&EffectOutcome::UnsettledInFlight) {
            return Err(TransferError::SourceCannotAcknowledge);
        }
        rec.state = TransferState::SourceAcknowledged;
        rec.updated_at = now;
        Ok(TransferStatus::SourceAcknowledged)
    }

    /// Destination received the run history and reacquired LOCAL authority
    /// with fresh receipts; generation increments for the destination.
    pub fn destination_ready(
        &mut self,
        transfer_id: &str,
        now: DateTime<Utc>,
    ) -> Result<TransferStatus, TransferError> {
        let rec = self
            .records
            .get_mut(transfer_id)
            .ok_or(TransferError::SourceCannotAcknowledge)?;
        if rec.state != TransferState::SourceAcknowledged {
            return Err(TransferError::SourceCannotAcknowledge);
        }
        rec.state = TransferState::DestinationReady;
        rec.generation += 1;
        rec.updated_at = now;
        Ok(TransferStatus::DestinationReady)
    }

    /// Both devices persisted transfer ID + generation: complete.
    pub fn complete(
        &mut self,
        transfer_id: &str,
        now: DateTime<Utc>,
    ) -> Result<TransferStatus, TransferError> {
        let rec = self
            .records
            .get_mut(transfer_id)
            .ok_or(TransferError::SourceCannotAcknowledge)?;
        if rec.state != TransferState::DestinationReady {
            return Err(TransferError::SourceCannotAcknowledge);
        }
        rec.state = TransferState::Completed;
        rec.updated_at = now;
        Ok(TransferStatus::Completed)
    }
}

/// Per-effect source-side outcome at transfer time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectOutcome {
    Settled,
    ExplicitlyOutcomeUnknown,
    UnsettledInFlight,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn acknowledgement_requires_settled_or_unknown_effects() {
        let mut c = TransferCoordinator::new();
        c.begin("t1", "run-1", now()).unwrap();
        // An in-flight unsettled effect blocks acknowledgement.
        assert!(matches!(
            c.acknowledge_source("t1", &[EffectOutcome::UnsettledInFlight], now()),
            Err(TransferError::SourceCannotAcknowledge)
        ));
        // Explicit outcome_unknown is acceptable (never silently retried).
        c.acknowledge_source(
            "t1",
            &[
                EffectOutcome::Settled,
                EffectOutcome::ExplicitlyOutcomeUnknown,
            ],
            now(),
        )
        .unwrap();
        assert_eq!(c.records["t1"].state, TransferState::SourceAcknowledged);
    }

    #[test]
    fn destination_generation_increments_and_completion_is_immutable() {
        let mut c = TransferCoordinator::new();
        c.begin("t2", "run-2", now()).unwrap();
        c.acknowledge_source("t2", &[], now()).unwrap();
        c.destination_ready("t2", now()).unwrap();
        let gen_at_ready = c.records["t2"].generation;
        assert_eq!(
            gen_at_ready, 2,
            "destination-ready increments the generation"
        );
        c.complete("t2", now()).unwrap();
        assert_eq!(c.records["t2"].state, TransferState::Completed);
        assert_eq!(
            c.records["t2"].generation, gen_at_ready,
            "completion persists, no further increment"
        );
        // Retry after completion reuses the ID but cannot mutate state.
        assert!(matches!(
            c.begin("t2", "run-2", now()),
            Err(TransferError::Completed(_))
        ));
    }

    #[test]
    fn crash_retry_reuses_transfer_id_and_generation() {
        let mut c = TransferCoordinator::new();
        c.begin("t3", "run-3", now()).unwrap();
        c.acknowledge_source("t3", &[], now()).unwrap();
        // "Crash": re-begin with the same ID before destination-ready.
        let rec = c.begin("t3", "run-3", now()).unwrap();
        assert_eq!(rec.state, TransferState::SourceAcknowledged);
        assert_eq!(rec.generation, 1, "generation unchanged on retry");
    }

    #[test]
    fn ack_timeout_bound_documented() {
        assert_eq!(TRANSFER_ACK_TIMEOUT_DAYS, 3);
        let _ = Duration::days(TRANSFER_ACK_TIMEOUT_DAYS);
    }
}
