//! Dump the full formula qualification report (HBR-153 / ACC-051 tier)
//! as JSON with a commit binding, for evidence/formula_evals/.
//!
//! Usage: cargo run -p harbor_formula --example qualify_dump -- [out.json]

use harbor_formula::qualify::run_qualification;

fn main() {
    let report = run_qualification().expect("qualification run");
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(true);
    let doc = serde_json::json!({
        "schema": "harbor.formula_qualification/v1",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "commit": commit,
        "tree_dirty": dirty,
        "report": report,
    });
    let out = std::env::args().nth(1);
    let text = serde_json::to_string_pretty(&doc).unwrap();
    match out {
        Some(path) => std::fs::write(path, text).expect("write evidence"),
        None => println!("{text}"),
    }
    eprintln!(
        "cases: {} pass / {} fail; targets PASS {}/{} [{}]",
        report.passed,
        report.failed,
        report.target_status.values().filter(|s| **s == "PASS").count(),
        report.target_status.len(),
        report.platform,
    );
}
