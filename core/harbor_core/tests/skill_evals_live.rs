//! Live tier: the built-in skill eval suites against a real GGUF model on
//! the pinned llama.cpp runtime. Ignored in CI (needs weights); run on the
//! qualification machine:
//!
//! ```text
//! HARBOR_LIVE_MODEL_GGUF=/path/to/model.gguf \
//!   cargo test -p harbor_core --test skill_evals_live -- --ignored --nocapture
//! ```
//!
//! `HARBOR_LIVE_MODEL_GGUF` defaults to the Qwen2.5-1.5B fixture under
//! `fixtures/models/` when present. With `HARBOR_RECORD_CASSETTES=1` every
//! model exchange is written to `evals/skills/<skill>/cassettes/live/<case>.json`
//! (never over the hand-authored cassettes) so live outputs are inspectable
//! and can be promoted to the replay tier after review.
//!
//! The report prints per case; a live failure is information about the
//! model, not a build break, so this test asserts only that the harness
//! ran and bound the model identity, and prints the pass/fail table.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use harbor_core::builtin_skills;
use harbor_core::harness::{builtin_suite_path, run_suite, EvalSuite, HarnessOptions, Tier};
use harbor_inference::provider::ModelRef;
use harbor_inference::{GgufLlamaCppProvider, ModelProvider};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn live_model_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("HARBOR_LIVE_MODEL_GGUF") {
        return Some(PathBuf::from(p));
    }
    let default = repo_root().join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    default.exists().then_some(default)
}

/// Install a GGUF through the real staged installer into a temp root.
fn install(models_root: &Path, gguf: &Path) -> (String, String) {
    let bytes = std::fs::read(gguf).expect("live model readable");
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("live-{}", &sha[..12]);
    let file_name = gguf
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model.gguf".into());
    let installer = PackageInstaller::new(models_root);
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: file_name,
            sha256: sha.clone(),
            size_bytes: bytes.len() as u64,
        }],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin(&id).unwrap();
    installer
        .ingest_file(&mut staged, &manifest.files[0], &bytes)
        .unwrap();
    let report = installer.validate(&staged, &manifest).unwrap();
    assert!(report.ok, "{:?}", report.problems);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    (id, sha)
}

#[test]
#[ignore = "needs a GGUF model (HARBOR_LIVE_MODEL_GGUF); qualification-machine tier"]
fn live_tier_runs_every_suite_and_reports_per_case() {
    let Some(gguf) = live_model_path() else {
        panic!("set HARBOR_LIVE_MODEL_GGUF or place the Qwen fixture under fixtures/models/");
    };
    let dir = tempfile::tempdir().unwrap();
    let (package_id, sha) = install(dir.path(), &gguf);
    let provider: Arc<dyn ModelProvider> = Arc::new(
        GgufLlamaCppProvider::new(dir.path())
            .unwrap()
            .with_context_tokens(8192),
    );
    let model = ModelRef::InstalledPackage {
        package_id: package_id.clone(),
    };
    provider.load(&model).unwrap();
    let record = std::env::var("HARBOR_RECORD_CASSETTES")
        .map(|v| v == "1")
        .unwrap_or(false);
    println!(
        "live model: {} sha256={} runtime={} record={record}",
        gguf.display(),
        sha,
        harbor_inference::runtime_identity()
    );

    let mut table = Vec::new();
    let mut details = Vec::new();
    for skill in builtin_skills()
        .unwrap()
        .into_iter()
        .filter(|s| s.has_graph())
    {
        let path = builtin_suite_path(&repo_root(), &skill.id);
        let suite = EvalSuite::load(&path).unwrap();
        let case_dir = path.parent().unwrap().to_path_buf();
        for case in &suite.cases {
            let one = EvalSuite {
                schema: suite.schema.clone(),
                skill: suite.skill.clone(),
                cases: vec![case.clone()],
            };
            let tier = if record {
                Tier::Record {
                    provider: provider.clone(),
                    model: model.clone(),
                    out: PathBuf::from("cassettes/live").join(format!("{}.json", case.id)),
                }
            } else {
                Tier::Live {
                    provider: provider.as_ref(),
                    model: model.clone(),
                }
            };
            let opts = HarnessOptions {
                fixture_root: repo_root(),
                case_dir: case_dir.clone(),
                tier,
                include_blackboard: true,
            };
            let started = std::time::Instant::now();
            let report = run_suite(&skill, &one, &opts).unwrap();
            let c = &report.cases[0];
            let ms = started.elapsed().as_millis();
            println!(
                "{}/{}: {} in {ms} ms ({} steps, {} tool calls, {} ctx tokens, executed_on={:?})",
                skill.id,
                c.case,
                if c.passed { "PASS" } else { "FAIL" },
                c.steps,
                c.tool_calls,
                c.context_tokens,
                c.model.executed_on
            );
            for a in &c.assertions {
                println!(
                    "    {} {}: {}",
                    if a.passed { "✓" } else { "✗" },
                    a.kind,
                    a.detail
                );
            }
            if !c.passed {
                if let harbor_core::executor::RunStatus::Failed { error } = &c.status {
                    println!("    run error: {error}");
                }
                if let Some(b) = &c.blackboard {
                    let keys: Vec<&String> = b
                        .as_object()
                        .map(|m| m.keys().collect())
                        .unwrap_or_default();
                    println!("    blackboard keys: {keys:?}");
                    for k in ["minutes", "look", "triage"] {
                        if let Some(v) = b.get(k) {
                            println!("    {k}: {}", serde_json::to_string(v).unwrap_or_default());
                        }
                    }
                }
            }
            // Live runs bind the real model identity, never the cassette.
            assert_eq!(
                c.model.executed_on.as_deref().unwrap_or(&package_id),
                package_id
            );
            details.push(serde_json::json!({
                "skill": skill.id,
                "case": c.case,
                "passed": c.passed,
                "run_state": c.run_state,
                "steps": c.steps,
                "tool_calls": c.tool_calls,
                "context_tokens": c.context_tokens,
                "elapsed_ms": ms,
                "assertions": c.assertions,
            }));
            table.push((skill.id.clone(), c.case.clone(), c.passed));
        }
    }
    let passed = table.iter().filter(|t| t.2).count();
    println!(
        "\nlive tier: {passed}/{} cases passed on {}",
        table.len(),
        gguf.display()
    );
    assert!(!table.is_empty());
    // Commit-bound evidence (evidence/ convention): model identity, runtime
    // revision and the per-case table. Reports for other models land next
    // to it, keyed by the weights hash.
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let out_dir = repo_root().join("evidence").join("skill_evals");
    std::fs::create_dir_all(&out_dir).unwrap();
    let report = serde_json::json!({
        "schema": "harbor.skill_evals_live/v1",
        "commit": commit,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "model": {"path": gguf.display().to_string(), "sha256": sha, "package_id": package_id},
        "runtime_revision": harbor_inference::runtime_identity(),
        "tier": if record { "record" } else { "live" },
        "passed": passed,
        "total": table.len(),
        "cases": table.iter().map(|(s, c, p)| serde_json::json!({"skill": s, "case": c, "passed": p})).collect::<Vec<_>>(),
        "details": details,
    });
    let path = out_dir.join(format!("live-{}.json", &sha[..12]));
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
    println!("evidence written: {}", path.display());
}
