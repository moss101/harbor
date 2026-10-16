//! Workflow templates: ordered skill runs whose inputs are taken from the
//! workflow's own inputs or from a PRIOR step's finished blackboard
//! (decision 0018). Pure data and mapping - no execution. Every step is an
//! ordinary skill run, so its approvals, protected effects, budgets and
//! receipts are exactly the ones a hand-started run has; the workflow only
//! decides what to start next and with which already-verified values.

use serde_json::{json, Map, Value};

pub enum Source {
    /// A workflow input; absent optional inputs are left out of the step.
    Workflow {
        key: &'static str,
        optional: bool,
    },
    /// A value in an earlier step's final state, by JSON pointer.
    Prior {
        step: usize,
        pointer: &'static str,
    },
    Const(Value),
}

pub struct Step {
    pub skill_id: &'static str,
    pub inputs: Vec<(&'static str, Source)>,
}

pub struct Template {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Required workflow inputs.
    pub required: &'static [&'static str],
    /// At least one of these workflow inputs must be given.
    pub one_of: &'static [&'static str],
    pub steps: Vec<Step>,
}

pub fn templates() -> Vec<Template> {
    vec![Template {
        id: "invoices-to-office",
        title: "Supplier invoices to Excel, Word and PowerPoint",
        description: "Reviews the invoices (overdue, comparison, totals) into an Excel report, then drafts a Word management report and a PowerPoint deck from the SAME verified summary. Each file is a separate proposal you approve; rejecting one stops the workflow.",
        required: &["as_of"],
        one_of: &["invoices", "artifact_id"],
        steps: vec![
            Step {
                skill_id: "supplier-invoice-review",
                inputs: vec![
                    ("invoices", Source::Workflow { key: "invoices", optional: true }),
                    ("artifact_id", Source::Workflow { key: "artifact_id", optional: true }),
                    ("as_of", Source::Workflow { key: "as_of", optional: false }),
                    ("previous", Source::Workflow { key: "previous", optional: true }),
                ],
            },
            Step {
                skill_id: "document-drafter",
                inputs: vec![
                    ("kind", Source::Const(json!("report"))),
                    ("title", Source::Const(json!("Supplier payments: management report"))),
                    ("brief", Source::Prior { step: 0, pointer: "/review/summary_text" }),
                    ("use_sources", Source::Workflow { key: "use_sources", optional: true }),
                ],
            },
            Step {
                skill_id: "presentation-builder",
                inputs: vec![
                    ("notes", Source::Prior { step: 0, pointer: "/review/summary_text" }),
                    ("audience", Source::Const(json!("Management"))),
                ],
            },
        ],
    }]
}

pub fn find(id: &str) -> Option<Template> {
    templates().into_iter().find(|t| t.id == id)
}

impl Template {
    pub fn validate_inputs(&self, inputs: &Value) -> Result<(), String> {
        let obj = inputs
            .as_object()
            .ok_or_else(|| "workflow inputs must be an object".to_string())?;
        for k in self.required {
            match obj.get(*k) {
                Some(Value::String(s)) if !s.trim().is_empty() => {}
                _ => return Err(format!("workflow input {k:?} is required")),
            }
        }
        if !self.one_of.is_empty()
            && !self
                .one_of
                .iter()
                .any(|k| matches!(obj.get(*k), Some(Value::String(s)) if !s.trim().is_empty()))
        {
            return Err(format!("give one of {:?}", self.one_of));
        }
        Ok(())
    }

    /// The inputs for `step`, given the workflow inputs and the final
    /// states of the steps before it. Missing prior values are an error,
    /// never a silent empty string.
    pub fn step_inputs(
        &self,
        step: usize,
        workflow_inputs: &Value,
        prior_states: &[Value],
    ) -> Result<Value, String> {
        let s = self
            .steps
            .get(step)
            .ok_or_else(|| format!("{} has no step {step}", self.id))?;
        let mut out = Map::new();
        for (name, src) in &s.inputs {
            match src {
                Source::Const(v) => {
                    out.insert((*name).into(), v.clone());
                }
                Source::Workflow { key, optional } => match workflow_inputs.get(*key) {
                    Some(v) if !v.is_null() => {
                        out.insert((*name).into(), v.clone());
                    }
                    _ if *optional => {}
                    _ => return Err(format!("workflow input {key:?} is missing")),
                },
                Source::Prior {
                    step: from,
                    pointer,
                } => {
                    let v = prior_states
                        .get(*from)
                        .and_then(|st| st.pointer(pointer))
                        .filter(|v| !v.is_null() && v.as_str().is_none_or(|t| !t.trim().is_empty()))
                        .ok_or_else(|| {
                            format!(
                                "step {from} produced no {pointer}; cannot start {}",
                                s.skill_id
                            )
                        })?;
                    out.insert((*name).into(), v.clone());
                }
            }
        }
        Ok(Value::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::builtin_skills;

    #[test]
    fn every_step_names_a_runnable_skill_and_only_its_declared_inputs() {
        let skills = builtin_skills().unwrap();
        for t in templates() {
            for step in &t.steps {
                let skill = skills
                    .iter()
                    .find(|s| s.id == step.skill_id)
                    .unwrap_or_else(|| panic!("{} names unknown skill {}", t.id, step.skill_id));
                let graph = skill
                    .graph
                    .as_ref()
                    .expect("workflow steps must be runnable");
                let props = graph.inputs["properties"].as_object().unwrap();
                for (name, _) in &step.inputs {
                    assert!(
                        props.contains_key(*name),
                        "{}: {} has no input {name}",
                        t.id,
                        step.skill_id
                    );
                }
                let required = graph.inputs["required"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for req in required.iter().filter_map(|r| r.as_str()) {
                    assert!(
                        step.inputs.iter().any(|(n, _)| *n == req),
                        "{}: required input {req} of {} is not mapped",
                        t.id,
                        step.skill_id
                    );
                }
            }
        }
    }

    #[test]
    fn later_steps_take_the_prior_summary_and_missing_values_are_errors() {
        let t = find("invoices-to-office").unwrap();
        let wf = json!({"invoices": "x", "as_of": "2026-10-09"});
        t.validate_inputs(&wf).unwrap();
        assert!(t.validate_inputs(&json!({"invoices": "x"})).is_err());
        assert!(t.validate_inputs(&json!({"as_of": "2026-10-09"})).is_err());
        t.validate_inputs(&json!({"artifact_id": "scan", "as_of": "d"}))
            .unwrap();
        assert!(t
            .validate_inputs(&json!({"invoices": " ", "as_of": "d"}))
            .is_err());
        let first = t.step_inputs(0, &wf, &[]).unwrap();
        assert_eq!(first, json!({"invoices": "x", "as_of": "2026-10-09"}));
        let done = json!({"review": {"summary_text": "3 overdue totalling 9,150.50"}});
        let second = t.step_inputs(1, &wf, std::slice::from_ref(&done)).unwrap();
        assert_eq!(second["brief"], "3 overdue totalling 9,150.50");
        assert_eq!(second["kind"], "report");
        assert!(second.get("use_sources").is_none());
        let third = t.step_inputs(2, &wf, &[done]).unwrap();
        assert_eq!(third["notes"], "3 overdue totalling 9,150.50");
        // A step whose predecessor produced nothing cannot start.
        assert!(t.step_inputs(1, &wf, &[json!({})]).is_err());
        assert!(t.step_inputs(1, &wf, &[]).is_err());
        assert!(t.step_inputs(9, &wf, &[]).is_err());
    }
}
