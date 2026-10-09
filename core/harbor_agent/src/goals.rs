//! Scheduled goals (decision 0011): durable, user-authorized proactive
//! work.
//!
//! A goal is DATA — a fixed request (a free prompt or a skill reference),
//! a schedule, and its execution receipts. The request is pinned at
//! creation: a broad goal never becomes unlimited future permission, and
//! every run still passes the executor's own admission and approval
//! machinery. The core hosts NO timers: the app drives due goals in the
//! foreground (the only scheduling iOS guarantees); a driver that cannot
//! run simply leaves the goal due for the next launch.
//!
//! Execution is claimed WRITE-AHEAD against a deterministic slot id, so
//! a restart, a crash, or a second driver can never double-run a slot —
//! at-most-once for side-effect-bearing work, the conservative failure.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What a goal may run. Fixed at creation — the executor never widens it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalRequest {
    /// A free-form Ask prompt (grounded generation path).
    Prompt { text: String },
    /// A skill run: the skill's own graph, budgets and approvals apply.
    Skill { skill_id: String, input: String },
}

/// When a goal comes due. `Once` fires at/after a wall-clock time;
/// `EveryMinutes` fires on fixed period boundaries (UTC-epoch grid), so
/// the slot id is stable across restarts and devices.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalSchedule {
    Once { at: DateTime<Utc> },
    EveryMinutes { minutes: u32 },
}

impl GoalSchedule {
    /// The deterministic execution slot covering `now`, or None when the
    /// schedule is not due. `Once` has exactly one slot, claimable from
    /// `at` onward; `EveryMinutes` slots are `floor(epoch / period)`.
    pub fn due_slot(&self, now: DateTime<Utc>) -> Option<String> {
        match self {
            GoalSchedule::Once { at } => {
                if now >= *at {
                    Some("once".to_string())
                } else {
                    None
                }
            }
            GoalSchedule::EveryMinutes { minutes } => {
                let period = (*minutes as i64) * 60;
                let slot = now.timestamp().div_euclid(period);
                Some(format!("every-{minutes}-{slot}"))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalState {
    Active,
    Paused,
    /// Terminal: max_runs reached.
    Done,
    /// Terminal: user cancelled.
    Cancelled,
}

/// One write-ahead execution claim (and, later, its outcome).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalExecution {
    pub slot: String,
    pub run_id: String,
    pub claimed_at: DateTime<Utc>,
    /// None until the driver records the outcome (crash ⇒ claimed,
    /// outcomeless — still at-most-once).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalSpec {
    pub id: String,
    pub title: String,
    pub request: GoalRequest,
    pub schedule: GoalSchedule,
    pub state: GoalState,
    pub created_at: DateTime<Utc>,
    /// None = runs until paused or cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runs: Option<u32>,
    #[serde(default)]
    pub run_count: u32,
    #[serde(default)]
    pub executions: Vec<GoalExecution>,
}

#[derive(Debug, thiserror::Error)]
pub enum GoalError {
    #[error("goal not found: {0}")]
    NotFound(String),
    #[error("invalid goal: {0}")]
    Invalid(String),
    #[error("goal {0}: execution slot already claimed ({1})")]
    SlotAlreadyClaimed(String, String),
    #[error("goal {0} is {1}; not claimable")]
    NotClaimable(String, String),
    #[error("store: {0}")]
    Store(String),
}

/// Upper bound on the period a goal may request (30 days): a typo like
/// `minutes: 100000` must be refused, not silently accepted.
const MAX_PERIOD_MINUTES: u32 = 30 * 24 * 60;
const MAX_TITLE: usize = 200;

impl GoalSpec {
    fn validate(&self) -> Result<(), GoalError> {
        if self.id.is_empty() {
            return Err(GoalError::Invalid("id is required".into()));
        }
        if self.title.trim().is_empty() || self.title.len() > MAX_TITLE {
            return Err(GoalError::Invalid("title must be 1..=200 chars".into()));
        }
        match &self.request {
            GoalRequest::Prompt { text } => {
                if text.trim().is_empty() {
                    return Err(GoalError::Invalid("prompt text is required".into()));
                }
            }
            GoalRequest::Skill { skill_id, input } => {
                if skill_id.trim().is_empty() {
                    return Err(GoalError::Invalid("skill_id is required".into()));
                }
                if input.trim().is_empty() {
                    return Err(GoalError::Invalid("skill input is required".into()));
                }
            }
        }
        match &self.schedule {
            GoalSchedule::Once { .. } => {}
            GoalSchedule::EveryMinutes { minutes } => {
                if *minutes == 0 || *minutes > MAX_PERIOD_MINUTES {
                    return Err(GoalError::Invalid(format!(
                        "minutes must be 1..={MAX_PERIOD_MINUTES}"
                    )));
                }
            }
        }
        if let Some(max) = self.max_runs {
            if max == 0 {
                return Err(GoalError::Invalid("max_runs must be >= 1".into()));
            }
        }
        Ok(())
    }
}

/// Storage marker for an AEAD-sealed goal store:
/// `enc.v1:<hex nonce><hex ciphertext>` — the run event log's payload
/// convention.
const SEALED_PREFIX: &str = "enc.v1:";
/// AEAD domain separation for the goal envelope.
const GOAL_AAD: &[u8] = b"harbor.agent.goals/v1";

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

/// Durable goal store: one file, atomic tmp+rename per mutation (the
/// FileStateStore pattern). Goals are few; whole-store rewrite is
/// simpler than a schema and crash-safe. Goal prompts are private
/// workspace content (policy 13, like run-event payloads):
/// [`GoalStore::open_with_key`] seals the whole envelope with AEAD
/// under a workspace-derived key that is never stored raw on disk.
pub struct GoalStore {
    path: PathBuf,
    key: Option<harbor_store::keys::KeyMaterial>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct GoalFile {
    goals: Vec<GoalSpec>,
}

impl GoalStore {
    /// Plaintext store — harness, tests and tools only. The product
    /// path is [`GoalStore::open_with_key`].
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, GoalError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GoalError::Store(format!("{}: {e}", parent.display())))?;
        }
        Ok(GoalStore { path, key: None })
    }

    /// Sealed store: goals sit on disk only as AEAD ciphertext.
    pub fn open_with_key(
        path: impl Into<PathBuf>,
        key: harbor_store::keys::KeyMaterial,
    ) -> Result<Self, GoalError> {
        let mut store = Self::open(path)?;
        store.key = Some(key);
        Ok(store)
    }

    fn read(&self) -> Result<GoalFile, GoalError> {
        if !self.path.exists() {
            return Ok(GoalFile::default());
        }
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| GoalError::Store(format!("{}: {e}", self.path.display())))?;
        let plain: Vec<u8> = if let Some(body) = text.strip_prefix(SEALED_PREFIX) {
            let sealed = hex_decode(body)
                .ok_or_else(|| GoalError::Store("corrupt goal store: bad hex".into()))?;
            if sealed.len() < 12 {
                return Err(GoalError::Store("corrupt goal store: short nonce".into()));
            }
            let (nonce, ct) = sealed.split_at(12);
            let nonce: [u8; 12] = nonce.try_into().unwrap();
            let key = self
                .key
                .as_ref()
                .ok_or_else(|| GoalError::Store("sealed goal store opened without a key".into()))?;
            harbor_store::keys::aead_open(key, &nonce, ct, GOAL_AAD).map_err(|_| {
                GoalError::Store("sealed goal store failed to open (wrong key?)".into())
            })?
        } else if self.key.is_some() {
            return Err(GoalError::Store(
                "plaintext goal store opened with a key; refusing to mix".into(),
            ));
        } else {
            text.into_bytes()
        };
        serde_json::from_slice(&plain)
            .map_err(|e| GoalError::Store(format!("corrupt goal store: {e}")))
    }

    fn write(&self, file: &GoalFile) -> Result<(), GoalError> {
        use rand::RngCore;
        let tmp = self.path.with_extension("json.tmp");
        let plain =
            serde_json::to_vec(file).map_err(|e| GoalError::Store(format!("serialize: {e}")))?;
        let bytes = match &self.key {
            None => plain,
            Some(key) => {
                let mut nonce = [0u8; 12];
                rand::rngs::OsRng.fill_bytes(&mut nonce);
                let ct = harbor_store::keys::aead_seal(key, &nonce, &plain, GOAL_AAD)
                    .map_err(|_| GoalError::Store("seal failed".into()))?;
                format!("{SEALED_PREFIX}{}{}", hex_encode(&nonce), hex_encode(&ct)).into_bytes()
            }
        };
        std::fs::write(&tmp, bytes)
            .map_err(|e| GoalError::Store(format!("{}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| GoalError::Store(format!("{}: {e}", self.path.display())))?;
        Ok(())
    }

    pub fn create(&self, mut spec: GoalSpec, now: DateTime<Utc>) -> Result<GoalSpec, GoalError> {
        if spec.created_at > now {
            return Err(GoalError::Invalid("created_at is in the future".into()));
        }
        spec.validate()?;
        spec.state = GoalState::Active;
        spec.run_count = 0;
        spec.executions.clear();
        let mut file = self.read()?;
        if file.goals.iter().any(|g| g.id == spec.id) {
            return Err(GoalError::Invalid(format!("goal {} exists", spec.id)));
        }
        let created = spec.clone();
        file.goals.push(spec);
        self.write(&file)?;
        Ok(created)
    }

    pub fn list(&self) -> Result<Vec<GoalSpec>, GoalError> {
        Ok(self.read()?.goals)
    }

    pub fn get(&self, id: &str) -> Result<Option<GoalSpec>, GoalError> {
        Ok(self.read()?.goals.into_iter().find(|g| g.id == id))
    }

    fn update<F>(&self, id: &str, f: F) -> Result<GoalSpec, GoalError>
    where
        F: FnOnce(GoalSpec) -> Result<GoalSpec, GoalError>,
    {
        let mut file = self.read()?;
        let goal = file
            .goals
            .iter()
            .position(|g| g.id == id)
            .ok_or_else(|| GoalError::NotFound(id.to_string()))?;
        let updated = f(file.goals[goal].clone())?;
        file.goals[goal] = updated.clone();
        self.write(&file)?;
        Ok(updated)
    }

    pub fn set_state(&self, id: &str, state: GoalState) -> Result<GoalSpec, GoalError> {
        self.update(id, |mut g| {
            // Terminal states have no outgoing transitions.
            if matches!(g.state, GoalState::Done | GoalState::Cancelled) {
                return Err(GoalError::NotClaimable(
                    id.to_string(),
                    format!("{:?}", g.state).to_lowercase(),
                ));
            }
            g.state = state;
            Ok(g)
        })
    }

    /// Active goals whose slot covers `now`, oldest id first. A paused
    /// or terminal goal is never due; its slot elapses (the interval
    /// grid simply moves on).
    pub fn due(&self, now: DateTime<Utc>) -> Result<Vec<(GoalSpec, String)>, GoalError> {
        let file = self.read()?;
        let mut out = Vec::new();
        for g in file.goals.iter().filter(|g| g.state == GoalState::Active) {
            if let Some(slot) = g.schedule.due_slot(now) {
                if !g.executions.iter().any(|e| e.slot == slot) {
                    out.push((g.clone(), slot));
                }
            }
        }
        out.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        Ok(out)
    }

    /// Write-ahead claim: record the execution BEFORE the work starts.
    /// Refuses a second claim for the same slot (restart / duplicate
    /// driver) and a goal beyond its run budget (auto-Done). The caller
    /// then starts the run the goal's request describes.
    pub fn claim(
        &self,
        id: &str,
        slot: &str,
        run_id: &str,
        now: DateTime<Utc>,
    ) -> Result<GoalSpec, GoalError> {
        self.update(id, |mut g| {
            if g.state != GoalState::Active {
                return Err(GoalError::NotClaimable(
                    id.to_string(),
                    format!("{:?}", g.state).to_lowercase(),
                ));
            }
            if let Some(max) = g.max_runs {
                if g.run_count >= max {
                    return Err(GoalError::NotClaimable(id.to_string(), "done".into()));
                }
            }
            if g.executions.iter().any(|e| e.slot == slot) {
                return Err(GoalError::SlotAlreadyClaimed(id.to_string(), slot.into()));
            }
            g.executions.push(GoalExecution {
                slot: slot.to_string(),
                run_id: run_id.to_string(),
                claimed_at: now,
                outcome: None,
                completed_at: None,
            });
            g.run_count += 1;
            if let Some(max) = g.max_runs {
                if g.run_count >= max {
                    g.state = GoalState::Done;
                }
            }
            Ok(g)
        })
    }

    /// Fill in the outcome of a claimed execution (observability only —
    /// the claim already prevented duplicate runs).
    pub fn record_outcome(
        &self,
        id: &str,
        run_id: &str,
        outcome: &str,
        now: DateTime<Utc>,
    ) -> Result<GoalSpec, GoalError> {
        self.update(id, |mut g| {
            let exec = g
                .executions
                .iter_mut()
                .rev()
                .find(|e| e.run_id == run_id)
                .ok_or_else(|| GoalError::NotFound(format!("{id}/{run_id}")))?;
            exec.outcome = Some(outcome.to_string());
            exec.completed_at = Some(now);
            Ok(g)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> chrono::DateTime<Utc> {
        chrono::Utc::now()
    }

    fn prompt_goal(id: &str, schedule: GoalSchedule) -> GoalSpec {
        GoalSpec {
            id: id.into(),
            title: "Weekly digest".into(),
            request: GoalRequest::Prompt {
                text: "summarize my documents".into(),
            },
            schedule,
            state: GoalState::Active,
            created_at: base(),
            max_runs: None,
            run_count: 0,
            executions: vec![],
        }
    }

    #[test]
    fn create_validates_and_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("goals.json");
        {
            let store = GoalStore::open(&path).unwrap();
            let g = store
                .create(
                    prompt_goal("goal-1", GoalSchedule::EveryMinutes { minutes: 15 }),
                    base(),
                )
                .unwrap();
            assert_eq!(g.state, GoalState::Active);
            assert!(store
                .create(
                    prompt_goal("goal-2", GoalSchedule::EveryMinutes { minutes: 0 }),
                    base()
                )
                .is_err());
            assert!(store
                .create(
                    GoalSpec {
                        request: GoalRequest::Prompt { text: "  ".into() },
                        ..prompt_goal("goal-3", GoalSchedule::Once { at: base() })
                    },
                    base()
                )
                .is_err());
            // duplicate id refused
            assert!(store
                .create(
                    prompt_goal("goal-1", GoalSchedule::Once { at: base() }),
                    base()
                )
                .is_err());
        }
        let store = GoalStore::open(&path).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.get("goal-1").unwrap().unwrap().id, "goal-1");
    }

    #[test]
    fn interval_slots_are_stable_and_claimed_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = GoalStore::open(dir.path().join("g.json")).unwrap();
        store
            .create(
                prompt_goal("g", GoalSchedule::EveryMinutes { minutes: 10 }),
                base(),
            )
            .unwrap();
        let now = chrono::Utc::now();
        let due = store.due(now).unwrap();
        assert_eq!(due.len(), 1);
        let (goal, slot) = due.into_iter().next().unwrap();
        // Same instant, second driver: identical slot.
        assert_eq!(goal.schedule.due_slot(now), Some(slot.clone()));
        store.claim("g", &slot, "run-1", now).unwrap();
        // Same slot claimed again (restart) is refused.
        assert!(matches!(
            store.claim("g", &slot, "run-2", now),
            Err(GoalError::SlotAlreadyClaimed(_, _))
        ));
        assert!(store.due(now).unwrap().is_empty());
        // A later slot is claimable.
        let later = now + chrono::Duration::minutes(11);
        assert_eq!(store.due(later).unwrap().len(), 1);
    }

    #[test]
    fn once_goal_fires_at_and_only_after_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let store = GoalStore::open(dir.path().join("g.json")).unwrap();
        let at = base() + chrono::Duration::hours(1);
        store
            .create(prompt_goal("g", GoalSchedule::Once { at }), base())
            .unwrap();
        assert!(store.due(base()).unwrap().is_empty());
        let due = store.due(at + chrono::Duration::seconds(1)).unwrap();
        assert_eq!(due.len(), 1);
        let (_, slot) = &due[0];
        assert_eq!(slot, "once");
        store.claim("g", slot, "run-1", at).unwrap();
        // A once goal never becomes due again.
        assert!(store
            .due(at + chrono::Duration::hours(2))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn pause_stops_due_and_terminal_states_hold() {
        let dir = tempfile::tempdir().unwrap();
        let store = GoalStore::open(dir.path().join("g.json")).unwrap();
        store
            .create(
                prompt_goal("g", GoalSchedule::EveryMinutes { minutes: 5 }),
                base(),
            )
            .unwrap();
        store.set_state("g", GoalState::Paused).unwrap();
        assert!(store.due(base()).unwrap().is_empty());
        store.set_state("g", GoalState::Active).unwrap();
        assert_eq!(store.due(base()).unwrap().len(), 1);
        store.set_state("g", GoalState::Cancelled).unwrap();
        assert!(store.due(base()).unwrap().is_empty());
        // Terminal states are final.
        assert!(store.set_state("g", GoalState::Active).is_err());
        assert!(matches!(
            store.claim("g", "every-5-0", "run-x", base()),
            Err(GoalError::NotClaimable(_, _))
        ));
    }

    #[test]
    fn max_runs_auto_completes_after_the_last_claim() {
        let dir = tempfile::tempdir().unwrap();
        let store = GoalStore::open(dir.path().join("g.json")).unwrap();
        let mut g = prompt_goal("g", GoalSchedule::EveryMinutes { minutes: 1 });
        g.max_runs = Some(2);
        store.create(g, base()).unwrap();
        let now = base();
        let (_, slot1) = store.due(now).unwrap().remove(0);
        store.claim("g", &slot1, "run-1", now).unwrap();
        assert_eq!(store.get("g").unwrap().unwrap().state, GoalState::Active);
        let later = now + chrono::Duration::minutes(2);
        let (_, slot2) = store.due(later).unwrap().remove(0);
        store.claim("g", &slot2, "run-2", later).unwrap();
        assert_eq!(store.get("g").unwrap().unwrap().state, GoalState::Done);
        // Done goals are not claimable, even on a fresh slot.
        let even_later = later + chrono::Duration::minutes(2);
        assert!(store.due(even_later).unwrap().is_empty());
        assert!(matches!(
            store.claim("g", "every-1-999", "run-3", even_later),
            Err(GoalError::NotClaimable(_, _))
        ));
    }

    #[test]
    fn outcomes_attach_to_their_claim() {
        let dir = tempfile::tempdir().unwrap();
        let store = GoalStore::open(dir.path().join("g.json")).unwrap();
        store
            .create(prompt_goal("g", GoalSchedule::Once { at: base() }), base())
            .unwrap();
        let (_, slot) = store.due(base()).unwrap().remove(0);
        store.claim("g", &slot, "run-9", base()).unwrap();
        // A crash between claim and outcome leaves the execution present
        // but outcomeless — reopening shows the claim, never re-runs.
        let reopened = GoalStore::open(&store.path).unwrap();
        let g = reopened.get("g").unwrap().unwrap();
        assert_eq!(g.executions.len(), 1);
        assert_eq!(g.executions[0].outcome, None);
        reopened
            .record_outcome("g", "run-9", "completed", base())
            .unwrap();
        let g = reopened.get("g").unwrap().unwrap();
        assert_eq!(g.executions[0].outcome.as_deref(), Some("completed"));
        assert!(reopened
            .record_outcome("g", "run-unknown", "x", base())
            .is_err());
    }

    #[test]
    fn sealed_store_round_trips_and_refuses_to_mix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("goals.json");
        let key = harbor_store::keys::KeyMaterial(*b"0123456789abcdef0123456789abcdef");
        {
            let store = GoalStore::open_with_key(&path, key.clone()).unwrap();
            store
                .create(
                    prompt_goal("g", GoalSchedule::EveryMinutes { minutes: 5 }),
                    base(),
                )
                .unwrap();
        }
        // At rest: the AEAD marker, no plaintext of the prompt.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.starts_with("enc.v1:"));
        assert!(!on_disk.contains("summarize my documents"));
        // Reopen with the right key: everything works.
        let store = GoalStore::open_with_key(&path, key).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        store
            .create(prompt_goal("g2", GoalSchedule::Once { at: base() }), base())
            .unwrap();
        assert_eq!(store.list().unwrap().len(), 2);
        // A plaintext store opened WITH a key is a hard refusal —
        // silently accepting it would mean unsealed private content is
        // being read as if it were protected.
        let plain_path = dir.path().join("plain.json");
        GoalStore::open(&plain_path)
            .unwrap()
            .create(prompt_goal("p", GoalSchedule::Once { at: base() }), base())
            .unwrap();
        assert!(GoalStore::open_with_key(
            &plain_path,
            harbor_store::keys::KeyMaterial(*b"ffffffffffffffffffffffffffffffff"),
        )
        .unwrap()
        .list()
        .is_err());
    }
}
