//! Live tier for the Apple system model: the built-in skill eval
//! suites against FoundationModels through the system-host bridge
//! (decision 0009). Ignored in CI; run on a qualification machine with
//! the adapter built:
//!
//! ```text
//! tools/build_apple_system_host.sh
//! cargo test -p harbor_core --test skill_evals_system -- --ignored --nocapture
//! ```
//!
//! `HARBOR_SYSTEM_HOST_DYLIB` overrides the adapter path. The dylib is
//! dlopened (never linked) so absence degrades to a typed error, the
//! same "optional provider" contract the runtime enforces. With
//! `HARBOR_RECORD_CASSETTES=1` every exchange is written to
//! `evals/skills/<skill>/cassettes/system/<case>.json` for promotion to
//! the replay tier after review.
//!
//! A live failure is information about the model or the bridge, not a
//! build break: like skill_evals_live, this asserts only that the tier
//! ran and bound the system-model identity, and prints the table. The
//! honest number lands in evidence/.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use harbor_core::builtin_skills;
use harbor_core::harness::{builtin_suite_path, run_suite, EvalSuite, HarnessOptions, Tier};
use harbor_inference::provider::ModelRef;
use harbor_inference::{ModelProvider, SystemHostBridge, SystemHostVtable};
use libloading::{Library, Symbol};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn host_dylib_path() -> PathBuf {
    if let Ok(p) = std::env::var("HARBOR_SYSTEM_HOST_DYLIB") {
        return PathBuf::from(p);
    }
    repo_root().join("native/apple/system_host/build/libharbor_system_host.dylib")
}

/// dlopen the adapter and expose it as a vtable. The Library must
/// outlive every call made through the vtable.
unsafe fn load_bridge(dylib: &Path) -> (Library, SystemHostBridge) {
    let lib = Library::new(dylib).unwrap_or_else(|e| {
        panic!(
            "cannot load system host adapter {}: {e}; run tools/build_apple_system_host.sh",
            dylib.display()
        )
    });
    let descriptor: Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_char> =
        lib.get(b"harbor_system_host_descriptor").unwrap();
    let generate: Symbol<
        unsafe extern "C" fn(
            *const std::ffi::c_char,
            *const AtomicBool,
            *mut *mut std::ffi::c_char,
        ) -> i32,
    > = lib.get(b"harbor_system_host_generate").unwrap();
    let free_string: Symbol<unsafe extern "C" fn(*mut std::ffi::c_char)> =
        lib.get(b"harbor_system_host_free_string").unwrap();
    let vtable = SystemHostVtable {
        descriptor: *descriptor,
        generate: *generate,
        free_string: *free_string,
    };
    let bridge = SystemHostBridge::from_vtable(vtable)
        .unwrap_or_else(|e| panic!("system host adapter rejected: {e}"));
    (lib, bridge)
}

#[test]
#[ignore = "needs the Apple system model adapter and an Apple-Intelligence device; qualification-machine tier"]
fn system_tier_runs_every_suite_and_reports_per_case() {
    let dylib = host_dylib_path();
    assert!(dylib.exists(), "adapter missing: {}", dylib.display());
    let (_lib, bridge) = unsafe { load_bridge(&dylib) };
    if let Some(reason) = bridge.unavailable_reason() {
        panic!(
            "Apple system model unavailable on this device: {reason}; \
             the bridge contract refuses to run against a missing model"
        );
    }
    // Capture identity before the Arc erases the concrete type.
    let identity = bridge.provider_model();
    let descriptor_identity: serde_json::Value =
        serde_json::to_value(bridge.identity()).unwrap_or_default();
    let provider: Arc<dyn ModelProvider> = Arc::new(bridge);
    let model = ModelRef::SystemManaged {
        provider_id: "apple-system".into(),
        model_id: "foundation-model/default".into(),
    };
    provider.load(&model).unwrap();
    let record = std::env::var("HARBOR_RECORD_CASSETTES")
        .map(|v| v == "1")
        .unwrap_or(false);
    println!(
        "system host: {} sha256={} record={record}",
        dylib.display(),
        adapter_sha(&dylib),
    );
    println!("identity: {identity}");

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
            if !case.tiers.iter().any(|t| t == "live") {
                println!(
                    "{}/{}: SKIP (replay-only guard case: {})",
                    skill.id,
                    case.id,
                    case.title.clone().unwrap_or_default()
                );
                continue;
            }
            let one = EvalSuite {
                schema: suite.schema.clone(),
                skill: suite.skill.clone(),
                cases: vec![case.clone()],
            };
            let tier = if record {
                Tier::Record {
                    provider: provider.clone(),
                    model: model.clone(),
                    out: PathBuf::from("cassettes/system").join(format!("{}.json", case.id)),
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
                }
            }
            // System runs bind the system-model identity, never the
            // cassette or a substituted package.
            assert_eq!(
                c.model.executed_on.as_deref().unwrap_or(&identity),
                identity,
                "a system-model run executed somewhere else"
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
        "\nsystem tier (apple): {passed}/{} cases passed on FoundationModels",
        table.len()
    );
    assert!(!table.is_empty());
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
        "schema": "harbor.skill_evals_system/v1",
        "commit": commit,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "model": {
            "provider_id": "apple-system",
            "model_id": "foundation-model/default",
            "executed_on": identity,
            "descriptor_identity": descriptor_identity,
            "adapter": {
                "path": dylib.display().to_string(),
                "sha256": adapter_sha(&dylib),
            },
        },
        "tier": if record { "record" } else { "live" },
        "passed": passed,
        "total": table.len(),
        "cases": table.iter().map(|(s, c, p)| serde_json::json!({"skill": s, "case": c, "passed": p})).collect::<Vec<_>>(),
        "details": details,
    });
    let path = out_dir.join(format!(
        "system-apple-{}.json",
        &harbor_canonical::sha256_hex(identity.as_bytes())[..12]
    ));
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
    println!("evidence written: {}", path.display());
}

fn adapter_sha(path: &Path) -> String {
    std::fs::read(path)
        .map(|b| harbor_canonical::sha256_hex(&b))
        .unwrap_or_default()
}
