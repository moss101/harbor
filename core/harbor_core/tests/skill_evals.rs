//! Built-in skill eval suites on the replay tier: no model weights, every
//! model node answered by its cassette, tools/approvals/budgets/replay
//! exercised for real. This is the CI gate for graph skills.

use harbor_core::harness::{run_builtin_suite, Tier};
use harbor_core::{builtin_skills, SkillManifest};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn graph_skills() -> Vec<SkillManifest> {
    builtin_skills()
        .unwrap()
        .into_iter()
        .filter(|s| s.has_graph())
        .collect()
}

#[test]
fn every_graph_skill_has_an_eval_suite() {
    let skills = graph_skills();
    assert!(
        skills.len() >= 4,
        "expected the four decomposed skills, got {}",
        skills.len()
    );
    for s in &skills {
        let path = harbor_core::harness::builtin_suite_path(&repo_root(), &s.id);
        assert!(
            path.exists(),
            "{}: missing eval suite at {}",
            s.id,
            path.display()
        );
    }
}

#[test]
fn replay_tier_passes_every_case_for_every_graph_skill() {
    let mut failures = Vec::new();
    let mut total = 0usize;
    for skill in graph_skills() {
        let report = run_builtin_suite(&repo_root(), &skill, Tier::Replay)
            .unwrap_or_else(|e| panic!("{}: {e}", skill.id));
        assert!(!report.runtime_revision.is_empty());
        for case in &report.cases {
            total += 1;
            println!(
                "{}/{}: {} ({} steps, {} tool calls, {} ms, provider {})",
                skill.id,
                case.case,
                if case.passed { "PASS" } else { "FAIL" },
                case.steps,
                case.tool_calls,
                case.elapsed_ms,
                case.model.provider_id
            );
            for a in case.assertions.iter().filter(|a| !a.passed) {
                println!("    ✗ {}: {}", a.kind, a.detail);
            }
            if !case.passed {
                failures.push(format!(
                    "{}/{}: {:?}\n  blackboard: {}",
                    skill.id,
                    case.case,
                    case.assertions
                        .iter()
                        .filter(|a| !a.passed)
                        .map(|a| format!("{}: {}", a.kind, a.detail))
                        .collect::<Vec<_>>(),
                    case.blackboard
                        .as_ref()
                        .map(|b| serde_json::to_string(b).unwrap_or_default())
                        .unwrap_or_default()
                ));
            }
            assert!(
                case.cassette_misses.is_empty(),
                "{}/{}: cassette misses {:?}",
                skill.id,
                case.case,
                case.cassette_misses
            );
            assert_eq!(case.model.tier, "replay");
        }
        assert_eq!(report.passed + report.failed, report.cases.len());
    }
    assert!(
        total >= 9,
        "expected at least nine cases across the four skills, got {total}"
    );
    assert!(
        failures.is_empty(),
        "failing cases:\n{}",
        failures.join("\n")
    );
}

#[test]
fn prose_skills_are_not_evaluable_and_say_so() {
    let prose = builtin_skills()
        .unwrap()
        .into_iter()
        .find(|s| !s.has_graph())
        .unwrap();
    let suite = harbor_core::harness::EvalSuite::parse(&format!(
        r#"{{"schema":"harbor.skill_eval/v1","skill":"{}","cases":[{{"id":"x","assertions":[{{"kind":"replay_verified"}}]}}]}}"#,
        prose.id
    ))
    .unwrap();
    let opts = harbor_core::harness::HarnessOptions {
        fixture_root: repo_root(),
        case_dir: repo_root(),
        tier: Tier::Replay,
        include_blackboard: false,
    };
    let err = harbor_core::harness::run_suite(&prose, &suite, &opts).unwrap_err();
    assert!(err.contains("no graph"), "{err}");
}
