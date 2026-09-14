//! Harbor Skill v1: declarative capability packages.
//!
//! Authority: `03_Architecture_Contracts.md` §6, goal §8.
//! A skill is instructions + tool permissions + capability requirements +
//! model policy + budgets + expected outputs + evaluation cases. Skills
//! are DATA: no arbitrary executable code is bundled or executed, and a
//! skill can never widen workspace privacy or OS capabilities — validation
//! enforces both properties at load time.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillManifest {
    pub schema: String,
    pub id: String,
    pub title: String,
    pub family: String,
    /// User-language description of the work this skill performs.
    pub description: String,
    /// Natural-language instructions consumed by the model at run time.
    pub instructions: String,
    /// Tool ids this skill may use (deny-by-default allowlist).
    #[serde(default)]
    pub tools: Vec<String>,
    /// Capability requirements (e.g. "knowledge.search", "artifact.write").
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub model_policy: ModelPolicy,
    #[serde(default)]
    pub budgets: SkillBudgets,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
    #[serde(default)]
    pub eval_cases: Vec<SkillEvalCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelPolicy {
    /// "local_first" (default) | "local_only".
    #[serde(default)]
    pub execution: String,
    /// Recommended model tier from the catalog (Fast/Balanced/...).
    #[serde(default)]
    pub tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SkillBudgets {
    #[serde(default)]
    pub max_steps: Option<u32>,
    #[serde(default)]
    pub max_tool_calls: Option<u32>,
    #[serde(default)]
    pub max_context_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillEvalCase {
    pub id: String,
    pub input: String,
    /// Behavior the run must exhibit (checked by the eval harness).
    pub expect: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("invalid skill manifest: {0}")]
    Invalid(String),
    #[error("skill {0}: tools {1:?} are not registered; skills cannot mint tools")]
    UnregisteredTool(String, Vec<String>),
    #[error("skill {0}: requirement {1} is not a registered capability")]
    UnregisteredRequirement(String, String),
    #[error("skill {0}: execution policy {1} would widen workspace privacy")]
    PrivacyWidening(String, String),
    #[error("code payload detected in skill {0}")]
    ExecutableCode(String),
}

pub const SCHEMA: &str = "harbor.skill/v1";

/// Registry of tools/capabilities a deployment offers. Skills reference
/// these by id; the registry is closed (host-controlled).
pub struct CapabilityCatalog {
    pub tools: BTreeMap<String, ()>,
    pub requirements: BTreeMap<String, ()>,
}

impl CapabilityCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tools(mut self, tools: &[&str]) -> Self {
        for t in tools {
            self.tools.insert(t.to_string(), ());
        }
        self
    }

    pub fn with_requirements(mut self, reqs: &[&str]) -> Self {
        for r in reqs {
            self.requirements.insert(r.to_string(), ());
        }
        self
    }
}

impl Default for CapabilityCatalog {
    fn default() -> Self {
        // The Harbor core tool/capability set (closed; extended by host).
        Self {
            tools: [
                "fs.read_workspace_file",
                "knowledge.search",
                "artifact.read",
                "artifact.propose_batch",
                "model.ask",
                "model.embed",
                "clipboard.read",
            ]
            .into_iter()
            .map(|t| (t.to_string(), ()))
            .collect(),
            requirements: [
                "knowledge.index",
                "artifact.engine",
                "formula.qualified",
                "ocr.qualified",
            ]
            .into_iter()
            .map(|r| (r.to_string(), ()))
            .collect(),
        }
    }
}

impl SkillManifest {
    pub fn parse(json: &str) -> Result<Self, SkillError> {
        let m: SkillManifest =
            serde_json::from_str(json).map_err(|e| SkillError::Invalid(e.to_string()))?;
        if m.schema != SCHEMA {
            return Err(SkillError::Invalid(format!(
                "schema must be {SCHEMA}, got {}",
                m.schema
            )));
        }
        if m.id.trim().is_empty() || m.instructions.trim().is_empty() {
            return Err(SkillError::Invalid(
                "id and instructions are required".into(),
            ));
        }
        Ok(m)
    }

    /// Validate against a host capability catalog and the privacy rules.
    pub fn validate(&self, catalog: &CapabilityCatalog) -> Result<(), SkillError> {
        let unknown_tools: Vec<String> = self
            .tools
            .iter()
            .filter(|t| !catalog.tools.contains_key(*t))
            .cloned()
            .collect();
        if !unknown_tools.is_empty() {
            return Err(SkillError::UnregisteredTool(self.id.clone(), unknown_tools));
        }
        for r in &self.requires {
            if !catalog.requirements.contains_key(r) {
                return Err(SkillError::UnregisteredRequirement(
                    self.id.clone(),
                    r.clone(),
                ));
            }
        }
        match self.model_policy.execution.as_str() {
            "" | "local_first" | "local_only" => {}
            // A skill may not authorize cloud execution; Remote Allowed is
            // a WORKSPACE-level decision, never a skill-level one.
            other => return Err(SkillError::PrivacyWidening(self.id.clone(), other.into())),
        }
        // No code payloads: instructions are prose; reject obvious script
        // markers that would indicate an attempt to smuggle execution.
        const MARKERS: [&str; 6] = ["#!/", "<script", "function(", "eval(", "os.system", "exec("];
        let haystack = format!("{} {}", self.instructions, self.description).to_lowercase();
        for m in MARKERS {
            if haystack.contains(m) {
                return Err(SkillError::ExecutableCode(self.id.clone()));
            }
        }
        Ok(())
    }
}

/// The built-in skill set shipped with Harbor (goal §8 families). Each is a
/// declarative package; adding one is data + eval cases, not code.
pub const BUILTIN_SKILLS_JSON: &str = include_str!("builtin_skills.json");

pub fn builtin_skills() -> Result<Vec<SkillManifest>, SkillError> {
    let v: Vec<SkillManifest> = serde_json::from_str(BUILTIN_SKILLS_JSON)
        .map_err(|e| SkillError::Invalid(e.to_string()))?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> CapabilityCatalog {
        CapabilityCatalog::default()
    }

    #[test]
    fn builtins_parse_validate_and_cover_authority_families() {
        let skills = builtin_skills().unwrap();
        assert!(
            skills.len() >= 21,
            "goal §8 requires >= 21 families, got {}",
            skills.len()
        );
        for s in &skills {
            s.validate(&catalog())
                .unwrap_or_else(|e| panic!("{}: {e}", s.id));
        }
    }

    #[test]
    fn unregistered_tool_rejected() {
        let json = r#"{
            "schema": "harbor.skill/v1", "id": "evil", "title": "Evil",
            "family": "test", "description": "d", "instructions": "do the work",
            "tools": ["os.shell"]
        }"#;
        let s = SkillManifest::parse(json).unwrap();
        assert!(matches!(
            s.validate(&catalog()),
            Err(SkillError::UnregisteredTool(_, _))
        ));
    }

    #[test]
    fn privacy_widening_policy_rejected() {
        let json = r#"{
            "schema": "harbor.skill/v1", "id": "cloudy", "title": "Cloudy",
            "family": "test", "description": "d", "instructions": "do the work",
            "model_policy": {"execution": "remote_allowed"}
        }"#;
        let s = SkillManifest::parse(json).unwrap();
        assert!(matches!(
            s.validate(&catalog()),
            Err(SkillError::PrivacyWidening(_, _))
        ));
    }

    #[test]
    fn code_smuggling_rejected() {
        let json = r#"{
            "schema": "harbor.skill/v1", "id": "scripty", "title": "S",
            "family": "test", "description": "d", "instructions": "run eval(payload) then summarize"
        }"#;
        let s = SkillManifest::parse(json).unwrap();
        assert!(matches!(
            s.validate(&catalog()),
            Err(SkillError::ExecutableCode(_))
        ));
    }

    #[test]
    fn wrong_schema_rejected() {
        assert!(SkillManifest::parse(r#"{"schema":"harbor.skill/v9"}"#).is_err());
    }
}
