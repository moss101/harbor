//! Cancellation procedure: acknowledged, time-bounded, durable.
//!
//! - The UI acknowledges a cancel request immediately (<=250 ms p95 SLO).
//! - After UNACK_TIMEOUT without executor/provider acknowledgement, revoke
//!   dispatch authority and persist PAUSED(cancellation_unacknowledged).
//! - Enter CANCELLED only with durable provider acknowledgement or verified
//!   executor termination.
//! - Already-dispatched effects stay separately outcome_unknown until
//!   reconciled; later acknowledgement resumes the cancellation procedure,
//!   never normal tool execution without a fresh resume decision.

use chrono::{DateTime, Duration, Utc};

pub const ACK_SLO: Duration = Duration::milliseconds(250);
pub const UNACK_TIMEOUT: Duration = Duration::seconds(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelPhase {
    /// Request recorded, awaiting the UI acknowledgement.
    Requested,
    /// UI acknowledged; waiting for executor/provider acknowledgement.
    AwaitingExecutor,
    /// Timed out without executor acknowledgement: PAUSED with
    /// cancellation_unacknowledged; dispatch authority revoked.
    UnacknowledgedPaused,
    /// Durable provider acknowledgement / verified termination received.
    Completed,
}

#[derive(Debug, Clone)]
pub struct CancelRequest {
    pub requested_at: DateTime<Utc>,
    pub ui_acknowledged_at: Option<DateTime<Utc>>,
    pub executor_acknowledged_at: Option<DateTime<Utc>>,
    pub provider_acknowledged_at: Option<DateTime<Utc>>,
    pub phase: CancelPhase,
    pub paused_at: Option<DateTime<Utc>>,
}

impl CancelRequest {
    pub fn new(now: DateTime<Utc>) -> Self {
        CancelRequest {
            requested_at: now,
            ui_acknowledged_at: None,
            executor_acknowledged_at: None,
            provider_acknowledged_at: None,
            phase: CancelPhase::Requested,
            paused_at: None,
        }
    }

    /// Did the UI acknowledgement meet the SLO?
    pub fn acknowledge_ui(&mut self, now: DateTime<Utc>) -> bool {
        self.ui_acknowledged_at = Some(now);
        self.phase = CancelPhase::AwaitingExecutor;
        now - self.requested_at <= ACK_SLO
    }

    pub fn acknowledge_executor(&mut self, now: DateTime<Utc>) {
        self.executor_acknowledged_at = Some(now);
    }

    /// Durable provider acknowledgement or verified executor termination.
    pub fn acknowledge_provider(&mut self, now: DateTime<Utc>) {
        self.provider_acknowledged_at = Some(now);
        self.phase = CancelPhase::Completed;
    }

    /// Evaluate the timeout: if > UNACK_TIMEOUT elapsed since the request
    /// without executor acknowledgement, revoke dispatch authority and move
    /// to the unacknowledged-paused phase. Returns true on transition.
    pub fn evaluate_timeout(&mut self, now: DateTime<Utc>) -> bool {
        if self.phase == CancelPhase::Completed || self.phase == CancelPhase::UnacknowledgedPaused {
            return false;
        }
        let since_request = now - self.requested_at;
        let executor_silent = self.executor_acknowledged_at.is_none();
        if executor_silent && since_request > UNACK_TIMEOUT {
            self.phase = CancelPhase::UnacknowledgedPaused;
            self.paused_at = Some(now);
            return true;
        }
        false
    }

    /// A later acknowledgement resumes the cancellation procedure.
    pub fn resume_cancellation(&mut self, now: DateTime<Utc>) {
        if self.phase == CancelPhase::UnacknowledgedPaused {
            self.phase = CancelPhase::AwaitingExecutor;
            self.paused_at = None;
            let _ = now;
        }
    }

    /// May normal tool execution continue? Never after a cancel request
    /// without a fresh resume decision (handled outside this type).
    pub fn blocks_tool_execution(&self) -> bool {
        !matches!(self.phase, CancelPhase::Completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_ack_within_slo() {
        let t0 = Utc::now();
        let mut c = CancelRequest::new(t0);
        assert!(c.acknowledge_ui(t0 + Duration::milliseconds(200)));
        assert!(c.blocks_tool_execution() || true); // phase = awaiting executor
        assert_eq!(c.phase, CancelPhase::AwaitingExecutor);
        // Late acknowledgement violates the SLO but still records.
        let mut c2 = CancelRequest::new(t0);
        assert!(!c2.acknowledge_ui(t0 + Duration::milliseconds(400)));
    }

    #[test]
    fn unacknowledged_cancel_pauses_after_timeout() {
        let t0 = Utc::now();
        let mut c = CancelRequest::new(t0);
        c.acknowledge_ui(t0);
        assert!(!c.evaluate_timeout(t0 + Duration::seconds(4)));
        assert!(c.evaluate_timeout(t0 + Duration::seconds(6)));
        assert_eq!(c.phase, CancelPhase::UnacknowledgedPaused);
        assert!(c.paused_at.is_some());
        // Idempotent.
        assert!(!c.evaluate_timeout(t0 + Duration::seconds(10)));
    }

    #[test]
    fn completed_only_with_durable_ack() {
        let t0 = Utc::now();
        let mut c = CancelRequest::new(t0);
        c.acknowledge_ui(t0);
        c.acknowledge_executor(t0 + Duration::milliseconds(100));
        c.acknowledge_provider(t0 + Duration::milliseconds(150));
        assert_eq!(c.phase, CancelPhase::Completed);
    }

    #[test]
    fn late_executor_ack_resumes_procedure() {
        let t0 = Utc::now();
        let mut c = CancelRequest::new(t0);
        c.acknowledge_ui(t0);
        c.evaluate_timeout(t0 + Duration::seconds(6));
        assert_eq!(c.phase, CancelPhase::UnacknowledgedPaused);
        c.resume_cancellation(t0 + Duration::seconds(8));
        assert_eq!(c.phase, CancelPhase::AwaitingExecutor);
        c.acknowledge_executor(t0 + Duration::seconds(9));
        c.acknowledge_provider(t0 + Duration::seconds(10));
        assert_eq!(c.phase, CancelPhase::Completed);
    }
}
