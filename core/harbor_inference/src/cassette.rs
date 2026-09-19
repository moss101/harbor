//! Record/replay provider (`harbor.cassette/v1`).
//!
//! Skill graphs confine the model to `model.*` nodes with declared output
//! schemas, so a run can be exercised end to end against *recorded* model
//! outputs: the tool layer, approvals, budgets, cancellation and replay
//! are then tested deterministically with no weights present (CI tier).
//! The same graphs run unchanged against a live provider on the
//! qualification tier, where `Record` mode captures new cassettes.
//!
//! Matching order on replay: `trace_key` (the executor sets
//! `graph_id/node_id#iteration`, so hand-authored fixtures survive prompt
//! wording changes), then the canonical request hash (recorded cassettes
//! bind the exact prompt). A miss is a typed error that carries the key
//! and the request so authors can add the entry; it is never silently
//! answered.

use std::sync::{Arc, Mutex};

use harbor_canonical::JsonValue;
use serde::{Deserialize, Serialize};

use crate::provider::{
    Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError, Usage,
};

pub const SCHEMA: &str = "harbor.cassette/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CassetteResponse {
    pub content: String,
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default = "default_executed_on")]
    pub executed_on: String,
}

fn default_executed_on() -> String {
    "cassette".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CassetteEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_key: Option<String>,
    /// Canonical hash of (messages, response_schema); empty for
    /// hand-authored entries that match on `trace_key` only.
    #[serde(default)]
    pub request_hash: String,
    /// Canonical JSON of the request as recorded (inspection only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    pub response: CassetteResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cassette {
    pub schema: String,
    #[serde(default)]
    pub entries: Vec<CassetteEntry>,
}

impl Default for Cassette {
    fn default() -> Self {
        Cassette {
            schema: SCHEMA.into(),
            entries: Vec::new(),
        }
    }
}

impl Cassette {
    pub fn parse(json: &str) -> Result<Cassette, ProviderError> {
        let c: Cassette = serde_json::from_str(json)
            .map_err(|e| ProviderError::Backend(format!("cassette: {e}")))?;
        if c.schema != SCHEMA {
            return Err(ProviderError::Backend(format!(
                "cassette schema must be {SCHEMA}, got {}",
                c.schema
            )));
        }
        Ok(c)
    }

    pub fn load(path: &std::path::Path) -> Result<Cassette, ProviderError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ProviderError::Backend(format!("cassette {}: {e}", path.display())))?;
        Self::parse(&text)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), ProviderError> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| ProviderError::Backend(e.to_string()))?;
        std::fs::write(path, text + "\n")
            .map_err(|e| ProviderError::Backend(format!("cassette {}: {e}", path.display())))
    }

    /// Hand-authoring helper: an entry answered by `trace_key`.
    pub fn with_entry(mut self, trace_key: &str, content: &str) -> Self {
        self.entries.push(CassetteEntry {
            trace_key: Some(trace_key.into()),
            request_hash: String::new(),
            request: None,
            response: CassetteResponse {
                content: content.into(),
                prompt_tokens: 0,
                completion_tokens: 0,
                executed_on: default_executed_on(),
            },
        });
        self
    }
}

/// Canonical identity of a request's model-visible content.
pub fn request_hash(req: &ChatRequest) -> Result<String, ProviderError> {
    let v = JsonValue::object([
        ("messages", JsonValue::Array(req.messages.clone())),
        (
            "response_schema",
            req.response_schema.clone().unwrap_or(JsonValue::Null),
        ),
    ]);
    v.canonical_sha256()
        .map_err(|e| ProviderError::Backend(format!("canonical: {e}")))
}

fn request_canonical(req: &ChatRequest) -> String {
    let v = JsonValue::object([
        ("messages", JsonValue::Array(req.messages.clone())),
        (
            "response_schema",
            req.response_schema.clone().unwrap_or(JsonValue::Null),
        ),
        (
            "max_tokens",
            JsonValue::int(req.max_tokens as i64).unwrap_or(JsonValue::Null),
        ),
        (
            "trace_key",
            req.trace_key
                .clone()
                .map(JsonValue::str)
                .unwrap_or(JsonValue::Null),
        ),
    ]);
    v.to_canonical_bytes()
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CassetteMode {
    /// Serve recorded responses; a miss is an error.
    Replay,
    /// Delegate to the inner provider and record every exchange.
    Record,
}

/// A miss on replay, kept for authoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CassetteMiss {
    pub trace_key: Option<String>,
    pub request_hash: String,
    pub request: String,
}

pub struct RecordReplayProvider {
    mode: CassetteMode,
    inner: Option<Arc<dyn ModelProvider>>,
    cassette: Mutex<Cassette>,
    misses: Mutex<Vec<CassetteMiss>>,
    /// Number of replayed answers (test observability).
    hits: Mutex<u64>,
}

impl RecordReplayProvider {
    pub fn replay(cassette: Cassette) -> Self {
        RecordReplayProvider {
            mode: CassetteMode::Replay,
            inner: None,
            cassette: Mutex::new(cassette),
            misses: Mutex::new(Vec::new()),
            hits: Mutex::new(0),
        }
    }

    pub fn record(inner: Arc<dyn ModelProvider>, seed: Cassette) -> Self {
        RecordReplayProvider {
            mode: CassetteMode::Record,
            inner: Some(inner),
            cassette: Mutex::new(seed),
            misses: Mutex::new(Vec::new()),
            hits: Mutex::new(0),
        }
    }

    pub fn mode(&self) -> CassetteMode {
        self.mode
    }

    pub fn cassette(&self) -> Cassette {
        self.cassette.lock().unwrap().clone()
    }

    pub fn misses(&self) -> Vec<CassetteMiss> {
        self.misses.lock().unwrap().clone()
    }

    pub fn hits(&self) -> u64 {
        *self.hits.lock().unwrap()
    }

    fn lookup(&self, req: &ChatRequest, hash: &str) -> Option<CassetteResponse> {
        let c = self.cassette.lock().unwrap();
        if let Some(k) = &req.trace_key {
            if let Some(e) = c
                .entries
                .iter()
                .find(|e| e.trace_key.as_deref() == Some(k.as_str()))
            {
                return Some(e.response.clone());
            }
        }
        c.entries
            .iter()
            .find(|e| !e.request_hash.is_empty() && e.request_hash == hash)
            .map(|e| e.response.clone())
    }
}

impl ModelProvider for RecordReplayProvider {
    fn id(&self) -> &str {
        match self.mode {
            CassetteMode::Replay => "cassette.replay",
            CassetteMode::Record => "cassette.record",
        }
    }

    fn capabilities(&self) -> &'static [Capabilities] {
        // Replay answers any structured request from the cassette; the
        // executor still validates the parsed output against the schema.
        &[Capabilities::Chat, Capabilities::StructuredOutput]
    }

    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        match (&self.mode, &self.inner) {
            (CassetteMode::Record, Some(inner)) => inner.supports(model, need),
            _ => self.capabilities().contains(need),
        }
    }

    fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
        match (&self.mode, &self.inner) {
            (CassetteMode::Record, Some(inner)) => inner.load(model),
            _ => Ok(()),
        }
    }

    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
        match (&self.mode, &self.inner) {
            (CassetteMode::Record, Some(inner)) => inner.unload(model),
            _ => Ok(()),
        }
    }

    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let hash = request_hash(&req)?;
        match self.mode {
            CassetteMode::Replay => match self.lookup(&req, &hash) {
                Some(r) => {
                    *self.hits.lock().unwrap() += 1;
                    Ok(ChatResponse {
                        content: r.content,
                        usage: Usage {
                            prompt_tokens: r.prompt_tokens,
                            completion_tokens: r.completion_tokens,
                        },
                        executed_on: r.executed_on,
                        execution_location: harbor_security::policy::ExecutionLocation::OnDevice,
                    })
                }
                None => {
                    let miss = CassetteMiss {
                        trace_key: req.trace_key.clone(),
                        request_hash: hash.clone(),
                        request: request_canonical(&req),
                    };
                    self.misses.lock().unwrap().push(miss);
                    Err(ProviderError::Backend(format!(
                        "cassette miss: trace_key={} request_hash={}",
                        req.trace_key.as_deref().unwrap_or("-"),
                        hash
                    )))
                }
            },
            CassetteMode::Record => {
                let inner = self.inner.as_ref().ok_or(ProviderError::Backend(
                    "record mode without inner provider".into(),
                ))?;
                let trace_key = req.trace_key.clone();
                let canonical = request_canonical(&req);
                let resp = inner.generate(req)?;
                self.cassette.lock().unwrap().entries.push(CassetteEntry {
                    trace_key,
                    request_hash: hash,
                    request: Some(canonical),
                    response: CassetteResponse {
                        content: resp.content.clone(),
                        prompt_tokens: resp.usage.prompt_tokens,
                        completion_tokens: resp.usage.completion_tokens,
                        executed_on: resp.executed_on.clone(),
                    },
                });
                Ok(resp)
            }
        }
    }

    fn embed(&self, model: &ModelRef, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        match (&self.mode, &self.inner) {
            (CassetteMode::Record, Some(inner)) => inner.embed(model, texts),
            _ => Err(ProviderError::UnsupportedCapability("embeddings")),
        }
    }

    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        match (&self.mode, &self.inner) {
            (CassetteMode::Record, Some(inner)) => inner.execution_location(),
            _ => harbor_security::policy::ExecutionLocation::OnDevice,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TestBackend;

    fn req(trace: Option<&str>, content: &str) -> ChatRequest {
        ChatRequest {
            model: ModelRef::InstalledPackage {
                package_id: "pkg-1".into(),
            },
            messages: vec![JsonValue::object([
                ("role", JsonValue::str("user")),
                ("content", JsonValue::str(content)),
            ])],
            max_tokens: 16,
            temperature: 0.0,
            requires: vec![Capabilities::Chat],
            response_schema: None,
            trace_key: trace.map(str::to_string),
        }
    }

    #[test]
    fn replay_matches_trace_key_then_hash_and_reports_misses() {
        let p =
            RecordReplayProvider::replay(Cassette::default().with_entry("g/n1#1", "{\"ok\":true}"));
        let r = p.generate(req(Some("g/n1#1"), "anything")).unwrap();
        assert_eq!(r.content, "{\"ok\":true}");
        assert_eq!(r.executed_on, "cassette");
        let err = p.generate(req(Some("g/n2#1"), "x")).unwrap_err();
        assert!(err.to_string().contains("cassette miss"), "{err}");
        assert_eq!(p.misses().len(), 1);
        assert_eq!(p.misses()[0].trace_key.as_deref(), Some("g/n2#1"));
        assert_eq!(p.hits(), 1);
    }

    #[test]
    fn record_then_replay_by_request_hash() {
        let inner: Arc<dyn ModelProvider> =
            Arc::new(TestBackend::default().with_packages(&["pkg-1"]));
        let rec = RecordReplayProvider::record(inner, Cassette::default());
        rec.load(&ModelRef::InstalledPackage {
            package_id: "pkg-1".into(),
        })
        .unwrap();
        let live = rec.generate(req(None, "hello")).unwrap();
        let cassette = rec.cassette();
        assert_eq!(cassette.entries.len(), 1);
        assert!(!cassette.entries[0].request_hash.is_empty());
        let text = serde_json::to_string(&cassette).unwrap();
        let replay = RecordReplayProvider::replay(Cassette::parse(&text).unwrap());
        let again = replay.generate(req(None, "hello")).unwrap();
        assert_eq!(again.content, live.content);
        // A different prompt is a miss on hash-only entries.
        assert!(replay.generate(req(None, "other")).is_err());
    }

    #[test]
    fn schema_participates_in_the_hash() {
        let a = request_hash(&req(None, "x")).unwrap();
        let mut r = req(None, "x");
        r.response_schema = Some(JsonValue::object([("type", JsonValue::str("object"))]));
        assert_ne!(a, request_hash(&r).unwrap());
    }
}
