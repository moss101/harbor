//! Skill graphs (`harbor.graph/v1`): control flow as data.
//!
//! Authority: `schemas/graph.schema.json`, `03_Architecture_Contracts.md`
//! §3/§4/§5/§6 and decision 0006. A graph is the executable spec of a
//! skill: nodes are typed steps, edges are declared transitions, and the
//! model only fills typed slots inside `model.*` nodes. Consequences the
//! validator enforces structurally:
//!
//! - the tool allowlist is exactly the set of tools named by `tool.call`
//!   nodes (the model never emits a tool name — SEC-005 by construction);
//! - cycles exist only through edges that declare `max_iterations` and an
//!   `exhausted` continuation, so every run is bounded before it starts;
//! - `/input` and `/host` are read-only roots of the blackboard;
//! - every edge target exists and a `map` body is private to its map.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pointer;

pub const SCHEMA: &str = "harbor.graph/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Graph {
    pub schema: String,
    pub id: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the run inputs (bound at `/input`).
    pub inputs: Value,
    pub entry: String,
    pub budgets: GraphBudgets,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphBudgets {
    pub max_steps: u32,
    pub max_tool_calls: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u64>,
}

/// A forward edge is a node id; a back-edge must carry its bound.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Edge {
    To(String),
    Bounded {
        to: String,
        max_iterations: u32,
        exhausted: String,
    },
}

impl Edge {
    pub fn target(&self) -> &str {
        match self {
            Edge::To(t) => t,
            Edge::Bounded { to, .. } => to,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextItem {
    pub label: String,
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Predicate {
    pub from: String,
    pub op: PredicateOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PredicateOp {
    Exists,
    Missing,
    Nonempty,
    Empty,
    Truthy,
    Falsy,
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchCase {
    pub when: Predicate,
    pub next: Edge,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectClass {
    #[serde(rename = "artifact.commit")]
    ArtifactCommit,
    #[serde(rename = "connector.send")]
    ConnectorSend,
    #[serde(rename = "file.mutate")]
    FileMutate,
}

impl EffectClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            EffectClass::ArtifactCommit => "artifact.commit",
            EffectClass::ConnectorSend => "connector.send",
            EffectClass::FileMutate => "file.mutate",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    NeedsInput,
    Abstained,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Completed => "completed",
            Outcome::NeedsInput => "needs_input",
            Outcome::Abstained => "abstained",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum Node {
    #[serde(rename = "tool.call")]
    ToolCall {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        tool: String,
        args: Value,
        out: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<Edge>,
    },
    #[serde(rename = "model.structured")]
    ModelStructured {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        instructions: String,
        #[serde(default)]
        context: Vec<ContextItem>,
        output_schema: Value,
        out: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_retries: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<Edge>,
    },
    #[serde(rename = "model.text")]
    ModelText {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        instructions: String,
        #[serde(default)]
        context: Vec<ContextItem>,
        out: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<Edge>,
    },
    #[serde(rename = "branch")]
    Branch {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        cases: Vec<BranchCase>,
        default: Edge,
    },
    #[serde(rename = "map")]
    Map {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        over: String,
        item: String,
        body: String,
        collect: String,
        max_items: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<Edge>,
    },
    #[serde(rename = "approval")]
    Approval {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        effect_class: EffectClass,
        batch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_approved: Option<Edge>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_rejected: Option<Edge>,
    },
    /// Write a literal value to the blackboard (fixed reasons, defaults,
    /// templates) without consulting a model or a tool.
    #[serde(rename = "const")]
    Const {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        value: Value,
        out: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<Edge>,
    },
    #[serde(rename = "end")]
    End {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        outcome: Outcome,
        #[serde(default)]
        outputs: Vec<String>,
    },
}

impl Node {
    pub fn id(&self) -> &str {
        match self {
            Node::ToolCall { id, .. }
            | Node::ModelStructured { id, .. }
            | Node::ModelText { id, .. }
            | Node::Branch { id, .. }
            | Node::Map { id, .. }
            | Node::Approval { id, .. }
            | Node::Const { id, .. }
            | Node::End { id, .. } => id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Node::ToolCall { .. } => "tool.call",
            Node::ModelStructured { .. } => "model.structured",
            Node::ModelText { .. } => "model.text",
            Node::Branch { .. } => "branch",
            Node::Map { .. } => "map",
            Node::Approval { .. } => "approval",
            Node::Const { .. } => "const",
            Node::End { .. } => "end",
        }
    }

    /// Every outgoing edge of this node (forward and bounded).
    pub fn edges(&self) -> Vec<&Edge> {
        match self {
            Node::ToolCall { next, .. }
            | Node::ModelStructured { next, .. }
            | Node::ModelText { next, .. } => next.iter().collect(),
            Node::Branch { cases, default, .. } => {
                let mut v: Vec<&Edge> = cases.iter().map(|c| &c.next).collect();
                v.push(default);
                v
            }
            Node::Map { next, .. } | Node::Const { next, .. } => next.iter().collect(),
            Node::Approval {
                next_approved,
                next_rejected,
                ..
            } => next_approved.iter().chain(next_rejected.iter()).collect(),
            Node::End { .. } => Vec::new(),
        }
    }

    /// The blackboard pointer this node writes, if any.
    pub fn out_pointer(&self) -> Option<&str> {
        match self {
            Node::ToolCall { out, .. }
            | Node::ModelStructured { out, .. }
            | Node::ModelText { out, .. } => Some(out),
            Node::Map { collect, .. } => Some(collect),
            Node::Const { out, .. } => Some(out),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("invalid graph: {0}")]
    Invalid(String),
    #[error("graph {graph}: {message}")]
    Structure { graph: String, message: String },
}

fn ident_ok(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    s.len() <= 128 && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

impl Graph {
    pub fn parse(json: &str) -> Result<Graph, GraphError> {
        let g: Graph =
            serde_json::from_str(json).map_err(|e| GraphError::Invalid(e.to_string()))?;
        g.validate()?;
        Ok(g)
    }

    pub fn from_value(value: &Value) -> Result<Graph, GraphError> {
        let g: Graph = serde_json::from_value(value.clone())
            .map_err(|e| GraphError::Invalid(e.to_string()))?;
        g.validate()?;
        Ok(g)
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id() == id)
    }

    /// The derived tool allowlist: every tool named by a `tool.call` node.
    pub fn tools(&self) -> BTreeSet<String> {
        self.nodes
            .iter()
            .filter_map(|n| match n {
                Node::ToolCall { tool, .. } => Some(tool.clone()),
                _ => None,
            })
            .collect()
    }

    /// Ids of nodes that call the model.
    pub fn model_nodes(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|n| matches!(n, Node::ModelStructured { .. } | Node::ModelText { .. }))
            .map(Node::id)
            .collect()
    }

    fn err(&self, message: impl Into<String>) -> GraphError {
        GraphError::Structure {
            graph: self.id.clone(),
            message: message.into(),
        }
    }

    /// Structural validation (see module docs). Independent of any host
    /// catalog: tool *existence* is checked by the registry at bind time.
    pub fn validate(&self) -> Result<(), GraphError> {
        if self.schema != SCHEMA {
            return Err(GraphError::Invalid(format!(
                "schema must be {SCHEMA}, got {}",
                self.schema
            )));
        }
        if !ident_ok(&self.id) {
            return Err(GraphError::Invalid(format!("bad graph id {:?}", self.id)));
        }
        if self.version == 0 {
            return Err(self.err("version must be >= 1"));
        }
        if !self.inputs.is_object() {
            return Err(self.err("inputs must be a JSON Schema object"));
        }
        if self.budgets.max_steps == 0 {
            return Err(self.err("budgets.max_steps must be >= 1"));
        }
        if self.nodes.is_empty() {
            return Err(self.err("graph has no nodes"));
        }
        let mut ids = BTreeSet::new();
        for n in &self.nodes {
            if !ident_ok(n.id()) {
                return Err(self.err(format!("bad node id {:?}", n.id())));
            }
            if !ids.insert(n.id().to_string()) {
                return Err(self.err(format!("duplicate node id {}", n.id())));
            }
        }
        if !ids.contains(&self.entry) {
            return Err(self.err(format!("entry node {} does not exist", self.entry)));
        }
        // Per-node checks.
        let mut map_bodies: BTreeSet<&str> = BTreeSet::new();
        for n in &self.nodes {
            self.validate_node(n, &ids)?;
            if let Node::Map { body, .. } = n {
                map_bodies.insert(body);
            }
        }
        if map_bodies.contains(self.entry.as_str()) {
            return Err(self.err("entry node cannot be a map body"));
        }
        // Edge targets exist; map bodies are private (no ordinary edge may
        // enter them); bounded edges name existing continuations.
        for n in &self.nodes {
            for e in n.edges() {
                if !ids.contains(e.target()) {
                    return Err(self.err(format!(
                        "node {} points to unknown node {}",
                        n.id(),
                        e.target()
                    )));
                }
                if map_bodies.contains(e.target()) {
                    return Err(self.err(format!(
                        "node {} enters map body {} directly; bodies run only through their map",
                        n.id(),
                        e.target()
                    )));
                }
                if let Edge::Bounded { exhausted, .. } = e {
                    if !ids.contains(exhausted) {
                        return Err(self.err(format!(
                            "node {} exhausted continuation {} does not exist",
                            n.id(),
                            exhausted
                        )));
                    }
                }
            }
        }
        // Acyclic over forward edges (bounded edges excluded); a forward
        // edge that closes a cycle is the authoring error the bound fixes.
        self.check_acyclic()?;
        // Every bounded edge must actually be a back-edge: its target must
        // reach the edge's source through forward edges, otherwise the
        // bound is meaningless (and the author probably meant a plain edge).
        let fwd = self.forward_adjacency();
        for n in &self.nodes {
            for e in n.edges() {
                if let Edge::Bounded { to, .. } = e {
                    if !reaches(&fwd, to, n.id()) {
                        return Err(self.err(format!(
                            "bounded edge {} -> {} is not a back-edge (target does not reach source)",
                            n.id(),
                            to
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_node(&self, n: &Node, ids: &BTreeSet<String>) -> Result<(), GraphError> {
        let check_out = |p: &str| -> Result<(), GraphError> {
            let segs =
                pointer::segments(p).map_err(|e| self.err(format!("node {}: {e}", n.id())))?;
            if segs.is_empty() {
                return Err(self.err(format!("node {}: out pointer cannot be the root", n.id())));
            }
            if pointer::READ_ONLY_ROOTS.contains(&segs[0].as_str()) {
                return Err(self.err(format!(
                    "node {}: out pointer {p} targets a read-only root",
                    n.id()
                )));
            }
            if segs.iter().any(|s| s == "*") {
                return Err(self.err(format!(
                    "node {}: out pointer {p} may not contain '*'",
                    n.id()
                )));
            }
            Ok(())
        };
        let check_read = |p: &str| -> Result<(), GraphError> {
            pointer::segments(p)
                .map(|_| ())
                .map_err(|e| self.err(format!("node {}: {e}", n.id())))
        };
        let check_context = |ctx: &[ContextItem]| -> Result<(), GraphError> {
            for c in ctx {
                if c.label.trim().is_empty() {
                    return Err(self.err(format!("node {}: context label empty", n.id())));
                }
                check_read(&c.from)?;
            }
            Ok(())
        };
        match n {
            Node::ToolCall {
                tool, args, out, ..
            } => {
                if !ident_ok(tool) {
                    return Err(self.err(format!("node {}: bad tool id {tool:?}", n.id())));
                }
                if !args.is_object() {
                    return Err(self.err(format!("node {}: args must be an object", n.id())));
                }
                check_arg_refs(args).map_err(|m| self.err(format!("node {}: {m}", n.id())))?;
                check_out(out)
            }
            Node::ModelStructured {
                instructions,
                context,
                output_schema,
                out,
                ..
            } => {
                if instructions.trim().is_empty() {
                    return Err(self.err(format!("node {}: instructions empty", n.id())));
                }
                if !output_schema.is_object() {
                    return Err(self.err(format!(
                        "node {}: output_schema must be a JSON Schema object",
                        n.id()
                    )));
                }
                check_grammar_safe(output_schema, "")
                    .map_err(|m| self.err(format!("node {}: output_schema {m}", n.id())))?;
                check_context(context)?;
                check_out(out)
            }
            Node::ModelText {
                instructions,
                context,
                out,
                ..
            } => {
                if instructions.trim().is_empty() {
                    return Err(self.err(format!("node {}: instructions empty", n.id())));
                }
                check_context(context)?;
                check_out(out)
            }
            Node::Branch { cases, .. } => {
                if cases.is_empty() {
                    return Err(
                        self.err(format!("node {}: branch needs at least one case", n.id()))
                    );
                }
                for c in cases {
                    check_read(&c.when.from)?;
                    let needs_value = matches!(
                        c.when.op,
                        PredicateOp::Eq
                            | PredicateOp::Neq
                            | PredicateOp::Gt
                            | PredicateOp::Lt
                            | PredicateOp::Gte
                            | PredicateOp::Lte
                    );
                    if needs_value && c.when.value.is_none() {
                        return Err(self.err(format!(
                            "node {}: predicate {:?} needs a value",
                            n.id(),
                            c.when.op
                        )));
                    }
                }
                Ok(())
            }
            Node::Map {
                over,
                item,
                body,
                collect,
                max_items,
                ..
            } => {
                check_read(over)?;
                check_out(item)?;
                check_out(collect)?;
                if *max_items == 0 {
                    return Err(self.err(format!("node {}: max_items must be >= 1", n.id())));
                }
                if !ids.contains(body) {
                    return Err(
                        self.err(format!("node {}: map body {body} does not exist", n.id()))
                    );
                }
                match self.node(body) {
                    Some(Node::ToolCall { next, .. })
                    | Some(Node::ModelText { next, .. })
                    | Some(Node::ModelStructured { next, .. }) => {
                        if next.is_some() {
                            return Err(self.err(format!(
                                "node {}: map body {body} may not declare next",
                                n.id()
                            )));
                        }
                    }
                    _ => {
                        return Err(self.err(format!(
                            "node {}: map body {body} must be a tool.call or model.* node",
                            n.id()
                        )))
                    }
                }
                Ok(())
            }
            Node::Const { out, .. } => check_out(out),
            Node::Approval { batch, .. } => check_read(batch),
            Node::End { outputs, .. } => {
                for o in outputs {
                    check_read(o)?;
                }
                Ok(())
            }
        }
    }

    fn forward_adjacency(&self) -> BTreeMap<&str, Vec<&str>> {
        let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for n in &self.nodes {
            let entry = adj.entry(n.id()).or_default();
            for e in n.edges() {
                if let Edge::To(t) = e {
                    entry.push(t);
                }
            }
            if let Node::Map { body, .. } = n {
                entry.push(body);
            }
        }
        adj
    }

    fn check_acyclic(&self) -> Result<(), GraphError> {
        let adj = self.forward_adjacency();
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            New,
            Active,
            Done,
        }
        let mut marks: BTreeMap<&str, Mark> = adj.keys().map(|k| (*k, Mark::New)).collect();
        fn visit<'a>(
            n: &'a str,
            adj: &BTreeMap<&'a str, Vec<&'a str>>,
            marks: &mut BTreeMap<&'a str, Mark>,
            stack: &mut Vec<&'a str>,
        ) -> Option<Vec<String>> {
            match marks.get(n).copied().unwrap_or(Mark::New) {
                Mark::Done => return None,
                Mark::Active => {
                    let mut cycle: Vec<String> = stack.iter().map(|s| s.to_string()).collect();
                    cycle.push(n.to_string());
                    return Some(cycle);
                }
                Mark::New => {}
            }
            marks.insert(n, Mark::Active);
            stack.push(n);
            if let Some(next) = adj.get(n) {
                for m in next {
                    if let Some(c) = visit(m, adj, marks, stack) {
                        return Some(c);
                    }
                }
            }
            stack.pop();
            marks.insert(n, Mark::Done);
            None
        }
        let ids: Vec<&str> = adj.keys().copied().collect();
        for id in ids {
            let mut stack = Vec::new();
            if let Some(cycle) = visit(id, &adj, &mut marks, &mut stack) {
                return Err(self.err(format!(
                    "unbounded cycle {}; declare max_iterations on the back-edge",
                    cycle.join(" -> ")
                )));
            }
        }
        Ok(())
    }
}

fn reaches(adj: &BTreeMap<&str, Vec<&str>>, from: &str, to: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        if let Some(next) = adj.get(n) {
            stack.extend(next.iter().copied());
        }
    }
    false
}

/// The pinned llama.cpp schema→grammar converter accepts a subset of JSON
/// Schema: string `pattern`s must be fully anchored (`^…$`) and may not be
/// combined with `minLength`/`maxLength`. Rejecting these at validation
/// time keeps structured nodes from failing at run time on the GGUF path.
pub fn check_grammar_safe(schema: &Value, path: &str) -> Result<(), String> {
    match schema {
        Value::Object(m) => {
            if let Some(Value::String(p)) = m.get("pattern") {
                if !(p.starts_with('^') && p.ends_with('$')) {
                    return Err(format!(
                        "at {path}: pattern {p:?} must be anchored with ^ and $"
                    ));
                }
                if m.contains_key("minLength") || m.contains_key("maxLength") {
                    return Err(format!(
                        "at {path}: pattern may not be combined with minLength/maxLength (use a {{n,m}} range in the pattern)"
                    ));
                }
            }
            for (k, v) in m {
                check_grammar_safe(v, &format!("{path}/{k}"))?;
            }
            Ok(())
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                check_grammar_safe(v, &format!("{path}/{i}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// `{"$state": pointer}` references must be well-formed wherever they
/// appear in a tool's args (any depth).
fn check_arg_refs(v: &Value) -> Result<(), String> {
    match v {
        Value::Object(m) => {
            if let Some(p) = m.get("$state") {
                if m.len() != 1 {
                    return Err("$state reference object may not carry other keys".into());
                }
                let p = p.as_str().ok_or("$state must be a string pointer")?;
                pointer::segments(p).map_err(|e| e.to_string())?;
                return Ok(());
            }
            for x in m.values() {
                check_arg_refs(x)?;
            }
            Ok(())
        }
        Value::Array(a) => {
            for x in a {
                check_arg_refs(x)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Resolve `{"$state": pointer}` references in `template` against `state`.
/// Missing references resolve to `null` (tools then reject them through
/// their schemas with a precise message).
pub fn resolve_refs(template: &Value, state: &Value) -> Value {
    match template {
        Value::Object(m) => {
            if let Some(Value::String(p)) = m.get("$state") {
                if m.len() == 1 {
                    return pointer::get(state, p).cloned().unwrap_or(Value::Null);
                }
            }
            Value::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), resolve_refs(v, state)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| resolve_refs(x, state)).collect()),
        other => other.clone(),
    }
}

/// Evaluate a branch predicate against the blackboard.
pub fn eval_predicate(p: &Predicate, state: &Value) -> bool {
    let v = pointer::get(state, &p.from);
    let cmp_num = |f: fn(f64, f64) -> bool| -> bool {
        match (
            v.and_then(Value::as_f64),
            p.value.as_ref().and_then(Value::as_f64),
        ) {
            (Some(a), Some(b)) => f(a, b),
            _ => false,
        }
    };
    match p.op {
        PredicateOp::Exists => v.is_some() && !v.unwrap().is_null(),
        PredicateOp::Missing => v.is_none() || v.unwrap().is_null(),
        PredicateOp::Nonempty => is_nonempty(v),
        PredicateOp::Empty => !is_nonempty(v),
        PredicateOp::Truthy => is_truthy(v),
        PredicateOp::Falsy => !is_truthy(v),
        PredicateOp::Eq => v == p.value.as_ref(),
        PredicateOp::Neq => v != p.value.as_ref(),
        PredicateOp::Gt => cmp_num(|a, b| a > b),
        PredicateOp::Lt => cmp_num(|a, b| a < b),
        PredicateOp::Gte => cmp_num(|a, b| a >= b),
        PredicateOp::Lte => cmp_num(|a, b| a <= b),
    }
}

fn is_nonempty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(m)) => !m.is_empty(),
        Some(_) => true,
    }
}

fn is_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        other => is_nonempty(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal(nodes: Value, entry: &str) -> Value {
        json!({
            "schema": "harbor.graph/v1",
            "id": "t",
            "version": 1,
            "inputs": {"type": "object"},
            "entry": entry,
            "budgets": {"max_steps": 10, "max_tool_calls": 5},
            "nodes": nodes
        })
    }

    #[test]
    fn parses_and_derives_allowlist() {
        let g = Graph::from_value(&minimal(
            json!([
                {"id": "read", "kind": "tool.call", "tool": "artifact.read", "args": {"artifact_id": {"$state": "/input/artifact_id"}}, "out": "/doc", "next": "sum"},
                {"id": "sum", "kind": "model.text", "instructions": "Summarize.", "context": [{"label": "Doc", "from": "/doc"}], "out": "/summary", "next": "done"},
                {"id": "done", "kind": "end", "outcome": "completed", "outputs": ["/summary"]}
            ]),
            "read",
        ))
        .unwrap();
        assert_eq!(
            g.tools().into_iter().collect::<Vec<_>>(),
            vec!["artifact.read".to_string()]
        );
        assert_eq!(g.model_nodes(), vec!["sum"]);
    }

    #[test]
    fn unbounded_cycle_rejected_bounded_cycle_accepted() {
        let cyc = |edge: Value| {
            minimal(
                json!([
                    {"id": "a", "kind": "model.text", "instructions": "x", "out": "/a", "next": "b"},
                    {"id": "b", "kind": "model.text", "instructions": "y", "out": "/b", "next": edge},
                    {"id": "z", "kind": "end", "outcome": "completed"}
                ]),
                "a",
            )
        };
        let err = Graph::from_value(&cyc(json!("a"))).unwrap_err();
        assert!(err.to_string().contains("unbounded cycle"), "{err}");
        Graph::from_value(&cyc(
            json!({"to": "a", "max_iterations": 3, "exhausted": "z"}),
        ))
        .unwrap();
        // A "bounded" edge that is not a back-edge is an authoring error.
        let err = Graph::from_value(&minimal(
            json!([
                {"id": "a", "kind": "model.text", "instructions": "x", "out": "/a", "next": {"to": "z", "max_iterations": 2, "exhausted": "z"}},
                {"id": "z", "kind": "end", "outcome": "completed"}
            ]),
            "a",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("not a back-edge"), "{err}");
    }

    #[test]
    fn read_only_roots_map_privacy_and_targets() {
        let err = Graph::from_value(&minimal(
            json!([{"id": "a", "kind": "model.text", "instructions": "x", "out": "/input/x"}]),
            "a",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("read-only"), "{err}");
        let err = Graph::from_value(&minimal(
            json!([{"id": "a", "kind": "model.text", "instructions": "x", "out": "/x", "next": "nope"}]),
            "a",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("unknown node"), "{err}");
        let err = Graph::from_value(&minimal(
            json!([
                {"id": "m", "kind": "map", "over": "/input/items", "item": "/cur", "body": "body", "collect": "/res", "max_items": 5, "next": "body"},
                {"id": "body", "kind": "model.text", "instructions": "x", "out": "/one"}
            ]),
            "m",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("enters map body"), "{err}");
    }

    #[test]
    fn structured_schemas_must_be_grammar_safe_and_const_nodes_validate() {
        let bad = Graph::from_value(&minimal(
            json!([{"id": "m", "kind": "model.structured", "instructions": "x", "output_schema": {"type": "object", "properties": {"a": {"type": "string", "pattern": "^=.+"}}}, "out": "/m"}]),
            "m",
        ))
        .unwrap_err();
        assert!(bad.to_string().contains("anchored"), "{bad}");
        let bad = Graph::from_value(&minimal(
            json!([{"id": "m", "kind": "model.structured", "instructions": "x", "output_schema": {"type": "object", "properties": {"a": {"type": "string", "pattern": "^x$", "maxLength": 3}}}, "out": "/m"}]),
            "m",
        ))
        .unwrap_err();
        assert!(bad.to_string().contains("minLength/maxLength"), "{bad}");
        let g = Graph::from_value(&minimal(
            json!([
                {"id": "c", "kind": "const", "value": {"reason": "fixed"}, "out": "/why", "next": "z"},
                {"id": "z", "kind": "end", "outcome": "abstained", "outputs": ["/why"]}
            ]),
            "c",
        ))
        .unwrap();
        assert_eq!(g.node("c").unwrap().kind(), "const");
        assert!(Graph::from_value(&minimal(
            json!([{"id": "c", "kind": "const", "value": 1, "out": "/input/x"}]),
            "c"
        ))
        .is_err());
    }

    #[test]
    fn refs_and_predicates() {
        let state = json!({"input": {"id": "A1", "n": 2}, "items": [], "s": "  "});
        let args = resolve_refs(
            &json!({"artifact_id": {"$state": "/input/id"}, "k": [{"$state": "/missing"}]}),
            &state,
        );
        assert_eq!(args, json!({"artifact_id": "A1", "k": [null]}));
        let p = |from: &str, op: PredicateOp, value: Option<Value>| Predicate {
            from: from.into(),
            op,
            value,
        };
        assert!(eval_predicate(
            &p("/input/n", PredicateOp::Gt, Some(json!(1))),
            &state
        ));
        assert!(!eval_predicate(
            &p("/items", PredicateOp::Nonempty, None),
            &state
        ));
        assert!(eval_predicate(&p("/s", PredicateOp::Empty, None), &state));
        assert!(eval_predicate(
            &p("/missing", PredicateOp::Missing, None),
            &state
        ));
        assert!(eval_predicate(
            &p("/input/id", PredicateOp::Eq, Some(json!("A1"))),
            &state
        ));
    }
}
