//! Run states and the legal transition graph.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunState {
    Created,
    Planning,
    Running,
    WaitingApproval,
    Paused,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl RunState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunState::Created => "CREATED",
            RunState::Planning => "PLANNING",
            RunState::Running => "RUNNING",
            RunState::WaitingApproval => "WAITING_APPROVAL",
            RunState::Paused => "PAUSED",
            RunState::Cancelling => "CANCELLING",
            RunState::Completed => "COMPLETED",
            RunState::Failed => "FAILED",
            RunState::Cancelled => "CANCELLED",
        }
    }

    pub fn parse(s: &str) -> Option<RunState> {
        Some(match s {
            "CREATED" => RunState::Created,
            "PLANNING" => RunState::Planning,
            "RUNNING" => RunState::Running,
            "WAITING_APPROVAL" => RunState::WaitingApproval,
            "PAUSED" => RunState::Paused,
            "CANCELLING" => RunState::Cancelling,
            "COMPLETED" => RunState::Completed,
            "FAILED" => RunState::Failed,
            "CANCELLED" => RunState::Cancelled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, RunState::Completed | RunState::Failed | RunState::Cancelled)
    }

    /// Closed transition graph from the authority.
    pub fn can_transition_to(self, next: RunState) -> bool {
        use RunState::*;
        matches!(
            (self, next),
            (Created, Planning)
                | (Created, Cancelling)
                | (Created, Failed)
                | (Planning, Running)
                | (Planning, Paused)
                | (Planning, Cancelling)
                | (Planning, Failed)
                | (Running, WaitingApproval)
                | (Running, Paused)
                | (Running, Cancelling)
                | (Running, Completed)
                | (Running, Failed)
                | (WaitingApproval, Running)
                | (WaitingApproval, Paused)
                | (WaitingApproval, Cancelling)
                | (WaitingApproval, Failed)
                | (Paused, Running)
                | (Paused, Cancelling)
                | (Cancelling, Cancelled)
                | (Cancelling, Paused)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    User,
    Background,
    Thermal,
    ResourcePressure,
    SourceUnavailable,
    NetworkPolicy,
    EffectOutcomeUnknown,
    CancellationUnacknowledged,
}

impl PauseReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            PauseReason::User => "user",
            PauseReason::Background => "background",
            PauseReason::Thermal => "thermal",
            PauseReason::ResourcePressure => "resource_pressure",
            PauseReason::SourceUnavailable => "source_unavailable",
            PauseReason::NetworkPolicy => "network_policy",
            PauseReason::EffectOutcomeUnknown => "effect_outcome_unknown",
            PauseReason::CancellationUnacknowledged => "cancellation_unacknowledged",
        }
    }

    pub fn parse(s: &str) -> Option<PauseReason> {
        Some(match s {
            "user" => PauseReason::User,
            "background" => PauseReason::Background,
            "thermal" => PauseReason::Thermal,
            "resource_pressure" => PauseReason::ResourcePressure,
            "source_unavailable" => PauseReason::SourceUnavailable,
            "network_policy" => PauseReason::NetworkPolicy,
            "effect_outcome_unknown" => PauseReason::EffectOutcomeUnknown,
            "cancellation_unacknowledged" => PauseReason::CancellationUnacknowledged,
            _ => return None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("illegal run transition {from} -> {to}")]
    IllegalTransition { from: &'static str, to: &'static str },
    #[error("pause requires a persisted reason; {from} -> PAUSED missing/invalid reason")]
    MissingPauseReason { from: &'static str },
    #[error("CANCELLING -> PAUSED requires reason cancellation_unacknowledged, got {0}")]
    CancellationPauseRequiresUnacknowledged(String),
    #[error("budget admission failed: {0}")]
    Budget(String),
    #[error("run {0} is terminal and cannot transition")]
    Terminal(&'static str),
}
