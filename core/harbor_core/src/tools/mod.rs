//! Tool contract (`03_Architecture_Contracts.md` §3) and registry.
//!
//! A tool exposes a JSON Schema for its arguments, a risk class, capability
//! requirements, a timeout and an output limit. Arguments are validated and
//! canonicalized here — below the model — and the registry refuses any
//! call outside the run's allowlist before the tool sees it (SEC-005). The
//! model cannot mint capabilities or approvals: `Propose`-class tools only
//! ever return a proposal (a typed batch bound to content hashes); the
//! protected effect happens elsewhere, after an approval receipt.
//!
//! Host resources (artifacts, knowledge, model provider, workspace files,
//! host-supplied inputs such as clipboard text) reach tools through
//! [`ToolContext`] traits so the same tools run under the FFI, under the
//! eval harness with fixtures, and in unit tests.

pub mod builtin;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use harbor_inference::provider::{ModelProvider, ModelRef};

use crate::jsonschema;
use crate::pointer;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    /// Reads host state; no side effects.
    Read,
    /// Produces a proposal (typed batch) that requires approval before any
    /// effect; never mutates anything itself.
    Propose,
    /// A protected external effect. Not callable from graphs in v1; kept so
    /// the registry can describe host tools honestly.
    Effect,
}

impl RiskClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskClass::Read => "read",
            RiskClass::Propose => "propose",
            RiskClass::Effect => "effect",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSpec {
    pub id: String,
    pub description: String,
    /// JSON Schema for the canonical argument object.
    pub args_schema: Value,
    pub risk: RiskClass,
    /// Capability requirements (host-registered ids, e.g. `artifact.engine`).
    #[serde(default)]
    pub requires: Vec<String>,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool {0} is not registered")]
    Unknown(String),
    #[error("tool {0} is outside the run allowlist")]
    NotAllowed(String),
    #[error("tool {tool}: invalid arguments: {violations}")]
    InvalidArgs { tool: String, violations: String },
    #[error("tool {tool}: {message}")]
    Failed { tool: String, message: String },
    #[error("tool {0}: required host resource unavailable: {1}")]
    Unavailable(String, String),
    #[error("tool {0}: output {1} bytes exceeds limit {2}")]
    OutputTooLarge(String, usize, usize),
    #[error("tool {0}: exceeded timeout of {1} ms (took {2} ms)")]
    Timeout(String, u64, u64),
    #[error("tool {0}: cancelled")]
    Cancelled(String),
}

impl ToolError {
    pub fn failed(tool: &str, message: impl Into<String>) -> Self {
        ToolError::Failed {
            tool: tool.into(),
            message: message.into(),
        }
    }
}

/// Bytes of an artifact the run may read, with a display name.
#[derive(Debug, Clone)]
pub struct ArtifactBytes {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
}

/// Artifacts reachable by a run, keyed by the ids the run inputs carry.
pub trait ArtifactSource: Send + Sync {
    fn get(&self, artifact_id: &str) -> Option<ArtifactBytes>;
    fn ids(&self) -> Vec<String>;
}

/// In-memory artifact source (harness fixtures, FFI-supplied bytes).
#[derive(Default)]
pub struct MemoryArtifacts {
    map: BTreeMap<String, ArtifactBytes>,
}

impl MemoryArtifacts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: &str, name: &str, bytes: Vec<u8>) {
        self.map.insert(
            id.to_string(),
            ArtifactBytes {
                name: name.to_string(),
                bytes: Arc::new(bytes),
            },
        );
    }

    pub fn with(mut self, id: &str, name: &str, bytes: Vec<u8>) -> Self {
        self.insert(id, name, bytes);
        self
    }
}

impl ArtifactSource for MemoryArtifacts {
    fn get(&self, artifact_id: &str) -> Option<ArtifactBytes> {
        self.map.get(artifact_id).cloned()
    }

    fn ids(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }
}

/// Local Knowledge search as the run sees it (implemented by the FFI over
/// the workspace index; by fixtures in the harness).
pub trait KnowledgeSearch: Send + Sync {
    /// Returns `{"citations": [{source_id, title, chunk_id, score, state,
    /// content_hash, _text}]}` — the same shape the FFI exposes.
    fn search(&self, query: &str, top_k: usize) -> Result<Value, String>;
}

/// Everything a tool may touch. Built by the executor per run; the
/// registry derives a per-call copy carrying the call deadline.
#[derive(Clone)]
pub struct ToolContext<'a> {
    pub artifacts: &'a dyn ArtifactSource,
    pub knowledge: Option<&'a dyn KnowledgeSearch>,
    pub provider: Option<&'a dyn ModelProvider>,
    pub model: Option<ModelRef>,
    /// Root the `fs.*` tools are scoped to (user-granted workspace folder).
    pub workspace_root: Option<PathBuf>,
    /// Host-supplied inputs bound at `/host` (e.g. `clipboard`).
    pub host_inputs: &'a Value,
    pub cancel: &'a AtomicBool,
    /// Opaque trace key for model calls made by tools.
    pub trace_key: Option<String>,
    /// Cooperative deadline set by the registry for the current call.
    pub deadline: Option<Instant>,
}

impl<'a> ToolContext<'a> {
    /// Minimal context: artifacts + host inputs + cancel flag.
    pub fn new(
        artifacts: &'a dyn ArtifactSource,
        host_inputs: &'a Value,
        cancel: &'a AtomicBool,
    ) -> Self {
        ToolContext {
            artifacts,
            knowledge: None,
            provider: None,
            model: None,
            workspace_root: None,
            host_inputs,
            cancel,
            trace_key: None,
            deadline: None,
        }
    }

    /// Long-running tools call this inside their loops: run cancellation
    /// and the call deadline are both cooperative.
    pub fn check_alive(&self, tool: &str) -> Result<(), ToolError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(ToolError::Cancelled(tool.into()));
        }
        if let Some(d) = self.deadline {
            if Instant::now() > d {
                return Err(ToolError::Timeout(tool.into(), 0, 0));
            }
        }
        Ok(())
    }
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> &ToolSpec;
    fn call(&self, ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub tool: String,
    /// Canonical (sorted-key) hash of the validated argument object; the
    /// identity an approval would bind.
    pub canonical_args_hash: String,
    pub output: Value,
    pub output_bytes: usize,
    pub elapsed_ms: u64,
}

/// Closed, host-controlled registry of tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
    /// Capability requirement ids the host declares satisfiable.
    requirements: BTreeSet<String>,
}

impl ToolRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The Harbor built-in tool set over the closed capability catalog.
    pub fn builtin() -> Self {
        let mut r = Self::empty();
        for t in builtin::all() {
            r.register(t);
        }
        for req in crate::skills::CapabilityCatalog::default()
            .requirements
            .keys()
        {
            r.requirements.insert(req.clone());
        }
        r
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.spec().id.clone(), tool);
    }

    pub fn declare_requirement(&mut self, id: &str) {
        self.requirements.insert(id.to_string());
    }

    pub fn ids(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn spec(&self, id: &str) -> Option<&ToolSpec> {
        self.tools.get(id).map(|t| t.spec())
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec().clone()).collect()
    }

    /// The registry as a skill validation catalog (tools + requirements).
    pub fn catalog(&self) -> crate::skills::CapabilityCatalog {
        let mut c = crate::skills::CapabilityCatalog {
            tools: BTreeMap::new(),
            requirements: BTreeMap::new(),
        };
        for id in self.tools.keys() {
            c.tools.insert(id.clone(), ());
        }
        for r in &self.requirements {
            c.requirements.insert(r.clone(), ());
        }
        c
    }

    /// Requirements a graph's tools need that the host has not declared.
    pub fn missing_requirements(&self, tools: &BTreeSet<String>) -> Vec<String> {
        let mut out = BTreeSet::new();
        for t in tools {
            if let Some(spec) = self.spec(t) {
                for r in &spec.requires {
                    if !self.requirements.contains(r) {
                        out.insert(r.clone());
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    /// Validate, canonicalize and dispatch one call under `allowlist`.
    pub fn call(
        &self,
        ctx: &ToolContext<'_>,
        tool_id: &str,
        args: &Value,
        allowlist: &BTreeSet<String>,
    ) -> Result<ToolResult, ToolError> {
        // 1. Allowlist below the model: a graph names its tools; nothing
        //    the model emits can widen the set.
        if !allowlist.contains(tool_id) {
            return Err(ToolError::NotAllowed(tool_id.into()));
        }
        let tool = self
            .tools
            .get(tool_id)
            .ok_or_else(|| ToolError::Unknown(tool_id.into()))?;
        let spec = tool.spec();
        if ctx.cancel.load(Ordering::Relaxed) {
            return Err(ToolError::Cancelled(tool_id.into()));
        }
        // 2. Schema validation + canonicalization (sorted keys; nulls kept:
        //    a nullable field is data the tool's schema decides about).
        let args = canonicalize_args(args);
        let violations = jsonschema::validate(&spec.args_schema, &args);
        if !violations.is_empty() {
            return Err(ToolError::InvalidArgs {
                tool: tool_id.into(),
                violations: violations
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            });
        }
        let canonical_args_hash = pointer::stable_hash(&args);
        // 3. Dispatch under a cooperative deadline: tools that loop call
        //    `ctx.check_alive`; a call that returns late is discarded and
        //    reported as a timeout so budgets stay honest.
        let started = Instant::now();
        let call_ctx = ToolContext {
            deadline: Some(started + std::time::Duration::from_millis(spec.timeout_ms)),
            ..ctx.clone()
        };
        let output = match tool.call(&call_ctx, &args) {
            Err(ToolError::Timeout(t, _, _)) => {
                return Err(ToolError::Timeout(
                    t,
                    spec.timeout_ms,
                    started.elapsed().as_millis() as u64,
                ))
            }
            other => other?,
        };
        let elapsed_ms = started.elapsed().as_millis() as u64;
        if elapsed_ms > spec.timeout_ms {
            return Err(ToolError::Timeout(
                tool_id.into(),
                spec.timeout_ms,
                elapsed_ms,
            ));
        }
        // 4. Output limit.
        let output_bytes = serde_json::to_vec(&output)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if output_bytes > spec.max_output_bytes {
            return Err(ToolError::OutputTooLarge(
                tool_id.into(),
                output_bytes,
                spec.max_output_bytes,
            ));
        }
        Ok(ToolResult {
            tool: tool_id.into(),
            canonical_args_hash,
            output,
            output_bytes,
            elapsed_ms,
        })
    }
}

/// Sorted keys, recursively; arrays keep order and `null` members are
/// preserved (a nullable field is data — schemas decide nullability).
pub fn canonicalize_args(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = serde_json::Map::new();
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            for k in keys {
                out.insert(k.clone(), canonicalize_args(&m[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(canonicalize_args).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Echo {
        spec: ToolSpec,
    }

    impl Tool for Echo {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        fn call(&self, _ctx: &ToolContext<'_>, args: &Value) -> Result<Value, ToolError> {
            Ok(json!({"echo": args}))
        }
    }

    fn echo(max_output_bytes: usize) -> Arc<dyn Tool> {
        Arc::new(Echo {
            spec: ToolSpec {
                id: "test.echo".into(),
                description: "echo".into(),
                args_schema: json!({"type": "object", "properties": {"x": {"type": "integer"}}, "required": ["x"], "additionalProperties": false}),
                risk: RiskClass::Read,
                requires: vec![],
                timeout_ms: 1000,
                max_output_bytes,
            },
        })
    }

    fn ctx<'a>(
        host: &'a Value,
        cancel: &'a AtomicBool,
        arts: &'a MemoryArtifacts,
    ) -> ToolContext<'a> {
        ToolContext::new(arts, host, cancel)
    }

    #[test]
    fn allowlist_schema_and_limits_are_enforced_below_the_model() {
        let mut r = ToolRegistry::empty();
        r.register(echo(1 << 20));
        let host = json!({});
        let cancel = AtomicBool::new(false);
        let arts = MemoryArtifacts::new();
        let c = ctx(&host, &cancel, &arts);
        let allow: BTreeSet<String> = ["test.echo".to_string()].into_iter().collect();
        // Not in allowlist → refused before dispatch.
        assert!(matches!(
            r.call(&c, "test.echo", &json!({"x": 1}), &BTreeSet::new()),
            Err(ToolError::NotAllowed(_))
        ));
        // Unknown tool.
        assert!(matches!(
            r.call(
                &c,
                "nope",
                &json!({}),
                &["nope".to_string()].into_iter().collect()
            ),
            Err(ToolError::Unknown(_))
        ));
        // Schema.
        let err = r
            .call(&c, "test.echo", &json!({"x": "one"}), &allow)
            .unwrap_err();
        assert!(err.to_string().contains("invalid arguments"), "{err}");
        // Canonicalization: keys sorted, hash stable, nulls preserved (an
        // undeclared null member is still an unexpected property).
        let a = r.call(&c, "test.echo", &json!({"x": 1}), &allow).unwrap();
        let b = r.call(&c, "test.echo", &json!({"x": 1}), &allow).unwrap();
        assert_eq!(a.canonical_args_hash, b.canonical_args_hash);
        assert_eq!(a.output, json!({"echo": {"x": 1}}));
        assert!(r
            .call(&c, "test.echo", &json!({"x": 1, "y": null}), &allow)
            .is_err());
        // Output limit.
        let mut small = ToolRegistry::empty();
        small.register(echo(4));
        assert!(matches!(
            small.call(&c, "test.echo", &json!({"x": 1}), &allow),
            Err(ToolError::OutputTooLarge(_, _, 4))
        ));
        // Cancel flag short-circuits.
        cancel.store(true, Ordering::Relaxed);
        assert!(matches!(
            r.call(&c, "test.echo", &json!({"x": 1}), &allow),
            Err(ToolError::Cancelled(_))
        ));
    }

    #[test]
    fn builtin_registry_matches_the_closed_catalog() {
        let r = ToolRegistry::builtin();
        let catalog = crate::skills::CapabilityCatalog::default();
        for t in catalog.tools.keys() {
            assert!(
                r.spec(t).is_some(),
                "catalog tool {t} has no implementation"
            );
        }
        for spec in r.specs() {
            assert!(spec.args_schema.is_object(), "{}", spec.id);
            assert!(
                spec.timeout_ms > 0 && spec.max_output_bytes > 0,
                "{}",
                spec.id
            );
            for req in &spec.requires {
                assert!(
                    catalog.requirements.contains_key(req),
                    "{}: unknown requirement {req}",
                    spec.id
                );
            }
        }
    }
}
