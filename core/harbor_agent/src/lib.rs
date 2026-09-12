//! Harbor durable agent runtime.
//!
//! Authority: `02_Runtime_Effect_and_Artifact_Contracts.md`,
//! `schemas/run_event.schema.json`, `03_Architecture_Contracts.md` §4.
//!
//! - Durable run lifecycle: CREATED → PLANNING → RUNNING → COMPLETED with
//!   formal transitions through WAITING_APPROVAL, PAUSED, CANCELLING,
//!   CANCELLED, FAILED. Terminal states have no outgoing transitions.
//! - Append-only, hash-chained run events; counters are durable monotonic
//!   totals; replay halts on unknown authority/state events and skips only
//!   envelope-valid `ignorable_display` events.
//! - Exactly one generation-fenced executor lease owns a run; stale
//!   generations cannot authorize events, dispatch or commits.
//! - Cancellation is acknowledged state: an unacknowledged cancel becomes
//!   PAUSED(cancellation_unacknowledged), never a silent CANCELLED.

pub mod budgets;
pub mod cancellation;
pub mod event;
pub mod lease;
pub mod log;
pub mod state;

pub use budgets::Budgets;
pub use cancellation::{CancelPhase, CancelRequest, ACK_SLO, UNACK_TIMEOUT};
pub use event::{Actor, Counters, EventPayload, EventType, ReplaySemantics, RunEvent};
pub use lease::{ExecutorLease, LeaseError, LeaseManager};
pub use log::{EventLog, LogError};
pub use state::{PauseReason, RunState, StateError};
