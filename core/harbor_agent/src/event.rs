//! Run event envelope (`harbor.run_event/v3`) with typed payloads.
//!
//! Envelope fields mirror `schemas/run_event.schema.json` so an event is
//! byte-representable in the cross-tool canonical form; the hash chain is
//! `prev_event_hash = sha256(canonical(previous_envelope))`, matching
//! `contracts.py::validate_event_append`.

use chrono::{DateTime, Utc};

use harbor_canonical::{CanonicalError, JsonValue};

use crate::state::{PauseReason, RunState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Actor {
    Executor,
    User,
    System,
    Provider,
    Recovery,
}

impl Actor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Actor::Executor => "executor",
            Actor::User => "user",
            Actor::System => "system",
            Actor::Provider => "provider",
            Actor::Recovery => "recovery",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplaySemantics {
    StateAffecting,
    AuthorityAffecting,
    IgnorableDisplay,
}

impl ReplaySemantics {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReplaySemantics::StateAffecting => "state_affecting",
            ReplaySemantics::AuthorityAffecting => "authority_affecting",
            ReplaySemantics::IgnorableDisplay => "ignorable_display",
        }
    }

    pub fn is_authoritative(self) -> bool {
        !matches!(self, ReplaySemantics::IgnorableDisplay)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    RunCreated,
    RunTransition,
    RunStepStarted,
    RunStepCompleted,
    RunApprovalRequested,
    RunApprovalDecided,
    RunEffectPrepared,
    RunEffectDispatched,
    RunEffectResolved,
    RunLeaseAcquired,
    RunLeaseLost,
    RunCancelRequested,
    RunBudgetUpdated,
    RunNote,
    /// An event type this build does not know. It is never appended by the
    /// runtime; it exists so replay can classify unknown streams per the
    /// authority: authority/state semantics halt, display may be skipped.
    Unknown(String),
}

impl EventType {
    pub fn as_str(&self) -> &str {
        match self {
            EventType::RunCreated => "run.created",
            EventType::RunTransition => "run.transition",
            EventType::RunStepStarted => "run.step_started",
            EventType::RunStepCompleted => "run.step_completed",
            EventType::RunApprovalRequested => "run.approval_requested",
            EventType::RunApprovalDecided => "run.approval_decided",
            EventType::RunEffectPrepared => "run.effect_prepared",
            EventType::RunEffectDispatched => "run.effect_dispatched",
            EventType::RunEffectResolved => "run.effect_resolved",
            EventType::RunLeaseAcquired => "run.lease_acquired",
            EventType::RunLeaseLost => "run.lease_lost",
            EventType::RunCancelRequested => "run.cancel_requested",
            EventType::RunBudgetUpdated => "run.budget_updated",
            EventType::RunNote => "run.note",
            EventType::Unknown(name) => name,
        }
    }

    pub fn parse(s: &str) -> EventType {
        match s {
            "run.created" => EventType::RunCreated,
            "run.transition" => EventType::RunTransition,
            "run.step_started" => EventType::RunStepStarted,
            "run.step_completed" => EventType::RunStepCompleted,
            "run.approval_requested" => EventType::RunApprovalRequested,
            "run.approval_decided" => EventType::RunApprovalDecided,
            "run.effect_prepared" => EventType::RunEffectPrepared,
            "run.effect_dispatched" => EventType::RunEffectDispatched,
            "run.effect_resolved" => EventType::RunEffectResolved,
            "run.lease_acquired" => EventType::RunLeaseAcquired,
            "run.lease_lost" => EventType::RunLeaseLost,
            "run.cancel_requested" => EventType::RunCancelRequested,
            "run.budget_updated" => EventType::RunBudgetUpdated,
            "run.note" => EventType::RunNote,
            other => EventType::Unknown(other.to_string()),
        }
    }

    /// True when this build has a typed decoder for the event.
    pub fn is_known(&self) -> bool {
        !matches!(self, EventType::Unknown(_))
    }

    /// Replay classification: which semantics may this event type carry?
    pub fn allowed_semantics(&self) -> &'static [ReplaySemantics] {
        if let EventType::Unknown(_) = self {
            // Unknown types accept any recorded semantics; the replay
            // driver, not the validator, decides halt vs skip.
            return &[
                ReplaySemantics::StateAffecting,
                ReplaySemantics::AuthorityAffecting,
                ReplaySemantics::IgnorableDisplay,
            ];
        }
        match self {
            EventType::RunCreated | EventType::RunTransition => &[ReplaySemantics::StateAffecting],
            EventType::RunApprovalRequested
            | EventType::RunApprovalDecided
            | EventType::RunEffectPrepared
            | EventType::RunEffectDispatched
            | EventType::RunEffectResolved
            | EventType::RunLeaseAcquired
            | EventType::RunLeaseLost
            | EventType::RunCancelRequested
            | EventType::RunBudgetUpdated => &[ReplaySemantics::AuthorityAffecting],
            EventType::RunStepStarted | EventType::RunStepCompleted | EventType::RunNote => &[
                ReplaySemantics::IgnorableDisplay,
                ReplaySemantics::StateAffecting,
            ],
            EventType::Unknown(_) => unreachable!("handled above"),
        }
    }
}

/// Typed payload for known events; arbitrary-but-canonical for display
/// notes. Unknown authority/state events halt replay (they parse as
/// `Unknown` and the replay driver decides).
#[derive(Debug, Clone, PartialEq)]
pub enum EventPayload {
    Created,
    Transition {
        from_state: RunState,
        to_state: RunState,
        reason: Option<PauseReason>,
    },
    StepStarted {
        step_id: String,
        description: String,
        /// Graph node identity (decision 0006). Absent for prose runs.
        node_id: Option<String>,
        /// Stable hash of the node's resolved input (blackboard slice).
        input_hash: Option<String>,
    },
    StepCompleted {
        step_id: String,
        summary: String,
        node_id: Option<String>,
        /// Stable hash of the value the node wrote to the blackboard.
        output_hash: Option<String>,
        /// Tool id when the step was a tool call.
        tool: Option<String>,
    },
    ApprovalRequested {
        effect_id: String,
        receipt_id: String,
    },
    ApprovalDecided {
        effect_id: String,
        approved: bool,
    },
    EffectPrepared {
        effect_id: String,
        canonical_args_hash: String,
    },
    EffectDispatched {
        effect_id: String,
        attempt_id: String,
    },
    EffectResolved {
        effect_id: String,
        outcome: String,
    },
    LeaseAcquired {
        generation: u64,
    },
    LeaseLost {
        generation: u64,
    },
    CancelRequested {
        requested_by: String,
    },
    BudgetUpdated {
        active_compute_ms_budget: Option<u64>,
        tool_calls_budget: Option<u64>,
    },
    Note {
        text: String,
    },
    /// Unparsed canonical payload for unknown event types.
    Raw(JsonValue),
}

impl EventPayload {
    pub fn to_json(&self) -> JsonValue {
        use JsonValue as V;
        match self {
            EventPayload::Created => V::Object(std::collections::BTreeMap::new()),
            EventPayload::Transition {
                from_state,
                to_state,
                reason,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("from_state".to_string(), V::str(from_state.as_str()));
                m.insert("to_state".to_string(), V::str(to_state.as_str()));
                m.insert(
                    "reason".to_string(),
                    reason.map(|r| V::str(r.as_str())).unwrap_or(V::Null),
                );
                V::Object(m)
            }
            EventPayload::StepStarted {
                step_id,
                description,
                node_id,
                input_hash,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("step_id".to_string(), V::str(step_id.clone()));
                m.insert("description".to_string(), V::str(description.clone()));
                if let Some(n) = node_id {
                    m.insert("node_id".to_string(), V::str(n.clone()));
                }
                if let Some(h) = input_hash {
                    m.insert("input_hash".to_string(), V::str(h.clone()));
                }
                V::Object(m)
            }
            EventPayload::StepCompleted {
                step_id,
                summary,
                node_id,
                output_hash,
                tool,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("step_id".to_string(), V::str(step_id.clone()));
                m.insert("summary".to_string(), V::str(summary.clone()));
                if let Some(n) = node_id {
                    m.insert("node_id".to_string(), V::str(n.clone()));
                }
                if let Some(h) = output_hash {
                    m.insert("output_hash".to_string(), V::str(h.clone()));
                }
                if let Some(t) = tool {
                    m.insert("tool".to_string(), V::str(t.clone()));
                }
                V::Object(m)
            }
            EventPayload::ApprovalRequested {
                effect_id,
                receipt_id,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("effect_id".to_string(), V::str(effect_id.clone()));
                m.insert("receipt_id".to_string(), V::str(receipt_id.clone()));
                V::Object(m)
            }
            EventPayload::ApprovalDecided {
                effect_id,
                approved,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("effect_id".to_string(), V::str(effect_id.clone()));
                m.insert("approved".to_string(), V::Bool(*approved));
                V::Object(m)
            }
            EventPayload::EffectPrepared {
                effect_id,
                canonical_args_hash,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("effect_id".to_string(), V::str(effect_id.clone()));
                m.insert(
                    "canonical_args_hash".to_string(),
                    V::str(canonical_args_hash.clone()),
                );
                V::Object(m)
            }
            EventPayload::EffectDispatched {
                effect_id,
                attempt_id,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("effect_id".to_string(), V::str(effect_id.clone()));
                m.insert("attempt_id".to_string(), V::str(attempt_id.clone()));
                V::Object(m)
            }
            EventPayload::EffectResolved { effect_id, outcome } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("effect_id".to_string(), V::str(effect_id.clone()));
                m.insert("outcome".to_string(), V::str(outcome.clone()));
                V::Object(m)
            }
            EventPayload::LeaseAcquired { generation } | EventPayload::LeaseLost { generation } => {
                let mut m = std::collections::BTreeMap::new();
                let key = "generation";
                m.insert(
                    key.to_string(),
                    JsonValue::int(*generation as i64).unwrap_or(V::Null),
                );
                V::Object(m)
            }
            EventPayload::CancelRequested { requested_by } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("requested_by".to_string(), V::str(requested_by.clone()));
                V::Object(m)
            }
            EventPayload::BudgetUpdated {
                active_compute_ms_budget,
                tool_calls_budget,
            } => {
                let mut m = std::collections::BTreeMap::new();
                let a = match active_compute_ms_budget {
                    Some(v) => JsonValue::int(*v as i64).unwrap_or(V::Null),
                    None => V::Null,
                };
                let t = match tool_calls_budget {
                    Some(v) => JsonValue::int(*v as i64).unwrap_or(V::Null),
                    None => V::Null,
                };
                m.insert("active_compute_ms_budget".to_string(), a);
                m.insert("tool_calls_budget".to_string(), t);
                V::Object(m)
            }
            EventPayload::Note { text } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("text".to_string(), V::str(text.clone()));
                V::Object(m)
            }
            EventPayload::Raw(v) => v.clone(),
        }
    }

    pub fn from_json(
        event_type: EventType,
        payload: &JsonValue,
    ) -> Result<EventPayload, PayloadError> {
        let obj = match payload {
            JsonValue::Object(m) => m,
            _ => return Err(PayloadError::NotAnObject),
        };
        let get = |k: &str| obj.get(k);
        // Envelope integrity for typed events: unknown/extra fields in a
        // known payload are rejected so stored-payload tampering cannot
        // hide behind a valid decode.
        let allowed: &[&str] = match event_type {
            EventType::RunCreated => &[],
            EventType::RunTransition => &["from_state", "to_state", "reason"],
            EventType::RunStepStarted => &["step_id", "description", "node_id", "input_hash"],
            EventType::RunStepCompleted => {
                &["step_id", "summary", "node_id", "output_hash", "tool"]
            }
            EventType::RunApprovalRequested => &["effect_id", "receipt_id"],
            EventType::RunApprovalDecided => &["effect_id", "approved"],
            EventType::RunEffectPrepared => &["effect_id", "canonical_args_hash"],
            EventType::RunEffectDispatched => &["effect_id", "attempt_id"],
            EventType::RunEffectResolved => &["effect_id", "outcome"],
            EventType::RunLeaseAcquired | EventType::RunLeaseLost => &["generation"],
            EventType::RunCancelRequested => &["requested_by"],
            EventType::RunBudgetUpdated => &["active_compute_ms_budget", "tool_calls_budget"],
            EventType::RunNote => &["text"],
            EventType::Unknown(_) => &[
                "from_state",
                "to_state",
                "reason",
                "step_id",
                "description",
                "summary",
                "effect_id",
                "receipt_id",
                "approved",
                "canonical_args_hash",
                "attempt_id",
                "outcome",
                "generation",
                "requested_by",
                "active_compute_ms_budget",
                "tool_calls_budget",
                "text",
            ],
        };
        if event_type.is_known() {
            for key in obj.keys() {
                if !allowed.contains(&key.as_str()) {
                    return Err(PayloadError::UnknownField(key.clone()));
                }
            }
        }
        Ok(match event_type {
            EventType::RunCreated => EventPayload::Created,
            EventType::RunTransition => {
                let from = get("from_state")
                    .and_then(|v| v.as_str())
                    .and_then(RunState::parse)
                    .ok_or(PayloadError::MissingField("from_state"))?;
                let to = get("to_state")
                    .and_then(|v| v.as_str())
                    .and_then(RunState::parse)
                    .ok_or(PayloadError::MissingField("to_state"))?;
                let reason: Option<PauseReason> = match get("reason") {
                    None | Some(JsonValue::Null) => None,
                    Some(v) => Some(
                        v.as_str()
                            .and_then(PauseReason::parse)
                            .ok_or(PayloadError::InvalidField("reason".to_string()))?,
                    ),
                };
                EventPayload::Transition {
                    from_state: from,
                    to_state: to,
                    reason,
                }
            }
            EventType::RunStepStarted => EventPayload::StepStarted {
                step_id: get("step_id")
                    .and_then(|v| v.as_str())
                    .ok_or(PayloadError::MissingField("step_id"))?
                    .into(),
                description: get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                node_id: get("node_id").and_then(|v| v.as_str()).map(str::to_string),
                input_hash: get("input_hash")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            },
            EventType::RunStepCompleted => EventPayload::StepCompleted {
                step_id: get("step_id")
                    .and_then(|v| v.as_str())
                    .ok_or(PayloadError::MissingField("step_id"))?
                    .into(),
                summary: get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                node_id: get("node_id").and_then(|v| v.as_str()).map(str::to_string),
                output_hash: get("output_hash")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                tool: get("tool").and_then(|v| v.as_str()).map(str::to_string),
            },
            EventType::RunApprovalRequested => EventPayload::ApprovalRequested {
                effect_id: get("effect_id")
                    .and_then(|v| v.as_str())
                    .ok_or(PayloadError::MissingField("effect_id"))?
                    .into(),
                receipt_id: get("receipt_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
            },
            EventType::RunApprovalDecided => EventPayload::ApprovalDecided {
                effect_id: get("effect_id")
                    .and_then(|v| v.as_str())
                    .ok_or(PayloadError::MissingField("effect_id"))?
                    .into(),
                approved: get("approved")
                    .and_then(|v| v.as_bool())
                    .ok_or(PayloadError::MissingField("approved"))?,
            },
            EventType::RunEffectPrepared => EventPayload::EffectPrepared {
                effect_id: get("effect_id")
                    .and_then(|v| v.as_str())
                    .ok_or(PayloadError::MissingField("effect_id"))?
                    .into(),
                canonical_args_hash: get("canonical_args_hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
            },
            EventType::RunEffectDispatched => EventPayload::EffectDispatched {
                effect_id: get("effect_id")
                    .and_then(|v| v.as_str())
                    .ok_or(PayloadError::MissingField("effect_id"))?
                    .into(),
                attempt_id: get("attempt_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
            },
            EventType::RunEffectResolved => EventPayload::EffectResolved {
                effect_id: get("effect_id")
                    .and_then(|v| v.as_str())
                    .ok_or(PayloadError::MissingField("effect_id"))?
                    .into(),
                outcome: get("outcome")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
            },
            EventType::RunLeaseAcquired | EventType::RunLeaseLost => {
                let generation = get("generation")
                    .and_then(|v| v.as_int())
                    .ok_or(PayloadError::MissingField("generation"))?;
                if generation < 0 {
                    return Err(PayloadError::InvalidField("generation".to_string()));
                }
                if event_type == EventType::RunLeaseAcquired {
                    EventPayload::LeaseAcquired {
                        generation: generation as u64,
                    }
                } else {
                    EventPayload::LeaseLost {
                        generation: generation as u64,
                    }
                }
            }
            EventType::RunCancelRequested => EventPayload::CancelRequested {
                requested_by: get("requested_by")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
            },
            EventType::RunBudgetUpdated => {
                let parse_opt = |k: &str| -> Result<Option<u64>, PayloadError> {
                    match get(k) {
                        None | Some(JsonValue::Null) => Ok(None),
                        Some(v) => {
                            let i = v
                                .as_int()
                                .ok_or_else(|| PayloadError::InvalidField(k.to_string()))?;
                            if i < 0 {
                                return Err(PayloadError::InvalidField(k.to_string()));
                            }
                            Ok(Some(i as u64))
                        }
                    }
                };
                EventPayload::BudgetUpdated {
                    active_compute_ms_budget: parse_opt("active_compute_ms_budget")?,
                    tool_calls_budget: parse_opt("tool_calls_budget")?,
                }
            }
            EventType::RunNote => EventPayload::Note {
                text: get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
            },
            EventType::Unknown(_) => EventPayload::Raw(payload.clone()),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("payload must be a JSON object")]
    NotAnObject,
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid value for field: {0}")]
    InvalidField(String),
    #[error("unknown field in typed payload: {0}")]
    UnknownField(String),
    #[error("canonical error: {0}")]
    Canonical(#[from] CanonicalError),
}

/// Durable monotonic counters carried on every event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    pub active_compute_ms_total: u64,
    pub step_count_total: u64,
    pub tool_count_total: u64,
    pub context_tokens_total: u64,
}

impl Counters {
    fn to_json(self) -> JsonValue {
        use JsonValue as V;
        let mut m = std::collections::BTreeMap::new();
        let put = |m: &mut std::collections::BTreeMap<String, JsonValue>, k: &str, v: u64| {
            m.insert(k.to_string(), V::int(v as i64).unwrap_or(V::Null));
        };
        put(
            &mut m,
            "active_compute_ms_total",
            self.active_compute_ms_total,
        );
        put(&mut m, "step_count_total", self.step_count_total);
        put(&mut m, "tool_count_total", self.tool_count_total);
        put(&mut m, "context_tokens_total", self.context_tokens_total);
        V::Object(m)
    }
}

/// The event envelope. Serialize with [`RunEvent::canonical_value`] for
/// hashing; the canonical form is the cross-tool representation.
#[derive(Debug, Clone)]
pub struct RunEvent {
    pub run_id: String,
    pub event_id: String,
    pub seq: u64,
    pub event_type: EventType,
    pub replay_semantics: ReplaySemantics,
    pub actor: Actor,
    pub lease_generation: u64,
    pub counters: Counters,
    pub payload: EventPayload,
    pub created_at: DateTime<Utc>,
    pub prev_event_hash: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("semantics {semantics} not allowed for event type {event_type}")]
    DisallowedSemantics {
        event_type: String,
        semantics: String,
    },
    #[error("actor {0} may not emit authoritative events without lease")]
    UnauthorizedAuthoritative(&'static str),
    #[error("canonical encoding error: {0}")]
    Canonical(#[from] CanonicalError),
}

impl RunEvent {
    /// Validate semantic constraints independent of chain state.
    pub fn validate(&self) -> Result<(), EventError> {
        if !self
            .event_type
            .allowed_semantics()
            .contains(&self.replay_semantics)
        {
            return Err(EventError::DisallowedSemantics {
                event_type: self.event_type.as_str().to_string(),
                semantics: self.replay_semantics.as_str().to_string(),
            });
        }
        Ok(())
    }

    /// Canonical JSON value of the envelope (schema-shaped).
    pub fn canonical_value(&self) -> Result<JsonValue, CanonicalError> {
        use JsonValue as V;
        let mut m = std::collections::BTreeMap::new();
        m.insert("schema".to_string(), V::str("harbor.run_event/v3"));
        m.insert("run_id".to_string(), V::str(self.run_id.clone()));
        m.insert("event_id".to_string(), V::str(self.event_id.clone()));
        m.insert("seq".to_string(), V::int(self.seq as i64)?);
        m.insert("event_type".to_string(), V::str(self.event_type.as_str()));
        m.insert(
            "replay_semantics".to_string(),
            V::str(self.replay_semantics.as_str()),
        );
        m.insert("actor".to_string(), V::str(self.actor.as_str()));
        m.insert(
            "lease_generation".to_string(),
            V::int(self.lease_generation as i64)?,
        );
        let counters = self.counters.to_json();
        if let V::Object(cm) = counters {
            for (k, v) in cm {
                m.insert(k, v);
            }
        }
        m.insert("payload".to_string(), self.payload.to_json());
        m.insert(
            "created_at".to_string(),
            V::str(
                self.created_at
                    .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            ),
        );
        m.insert(
            "prev_event_hash".to_string(),
            self.prev_event_hash.clone().map(V::Str).unwrap_or(V::Null),
        );
        Ok(V::Object(m))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        self.canonical_value()?.to_canonical_bytes()
    }

    pub fn hash(&self) -> Result<String, CanonicalError> {
        Ok(harbor_canonical::sha256_hex(&self.canonical_bytes()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_event(seq: u64, event_type: EventType, payload: EventPayload) -> RunEvent {
        let display = matches!(event_type, EventType::RunNote);
        let created = matches!(event_type, EventType::RunCreated);
        RunEvent {
            run_id: "run-1".into(),
            event_id: format!("evt-{seq}"),
            seq,
            event_type,
            replay_semantics: if display {
                ReplaySemantics::IgnorableDisplay
            } else {
                ReplaySemantics::StateAffecting
            },
            actor: if created { Actor::System } else { Actor::User },
            lease_generation: 0,
            counters: Counters::default(),
            payload,
            created_at: Utc::now(),
            prev_event_hash: None,
        }
    }

    #[test]
    fn created_requires_state_affecting() {
        let mut e = base_event(0, EventType::RunCreated, EventPayload::Created);
        e.replay_semantics = ReplaySemantics::IgnorableDisplay;
        assert!(e.validate().is_err());
        e.replay_semantics = ReplaySemantics::StateAffecting;
        assert!(e.validate().is_ok());
    }

    #[test]
    fn hash_chain_is_deterministic() {
        let a = base_event(0, EventType::RunCreated, EventPayload::Created);
        let h1 = a.hash().unwrap();
        let h2 = a.hash().unwrap();
        assert_eq!(h1, h2);
        let mut b = base_event(
            1,
            EventType::RunTransition,
            EventPayload::Transition {
                from_state: RunState::Created,
                to_state: RunState::Planning,
                reason: None,
            },
        );
        b.prev_event_hash = Some(h1.clone());
        assert_ne!(b.hash().unwrap(), h1);
    }

    #[test]
    fn transition_payload_roundtrip() {
        let p = EventPayload::Transition {
            from_state: RunState::Cancelling,
            to_state: RunState::Paused,
            reason: Some(PauseReason::CancellationUnacknowledged),
        };
        let j = p.to_json();
        let back = EventPayload::from_json(EventType::RunTransition, &j).unwrap();
        assert_eq!(p, back);
    }
}
