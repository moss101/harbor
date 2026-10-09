//! Live skill-routing qualification: the built-in skills' own eval-case
//! inputs, routed by the prewarmed router over a REAL embedding model.
//! Measures decision-routing quality the way the knowledge tier measures
//! retrieval: top-1 accuracy over labeled inputs, the score separation,
//! abstention behavior at the shipped thresholds, and latency.
//!
//! Ignored in CI (needs weights); qualification-machine tier:
//!
//! ```text
//! cargo test -p harbor_core --test router_live -- --ignored --nocapture
//! ```
//!
//! `HARBOR_EMBED_MODEL_GGUF` overrides the model (default: the pinned
//! bge-m3 fixture — the package that qualifies every knowledge stratum).
//! Evidence lands in `evidence/skill_routing/live-<weights-hash>.json`
//! with the model identity, per-candidate distributions and the exact
//! command. The router can only ever RECOMMEND; this file measures how
//! often the recommendation would be right.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use harbor_core::router::{AbstainReason, SkillRouter};
use harbor_core::skills::builtin_skills;
use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn model_path() -> PathBuf {
    if let Ok(p) = std::env::var("HARBOR_EMBED_MODEL_GGUF") {
        return PathBuf::from(p);
    }
    repo_root().join("fixtures/models/bge-m3-q8_0.gguf")
}

fn install(models_root: &Path, gguf: &Path) -> (String, String) {
    let bytes = std::fs::read(gguf).expect("embedding fixture readable");
    let sha = harbor_canonical::sha256_hex(&bytes);
    let stem = gguf
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model".into());
    let id = format!("live-router-{stem}-{}", &sha[..12]);
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
#[ignore = "needs an embedding GGUF (HARBOR_EMBED_MODEL_GGUF); qualification-machine tier"]
fn live_router_routes_the_builtin_eval_inputs() {
    let gguf = model_path();
    assert!(gguf.exists(), "embedding model missing: {}", gguf.display());
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let (package_id, sha) = install(&models_root, &gguf);
    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let model = ModelRef::InstalledPackage {
        package_id: package_id.clone(),
    };
    provider.load(&model).unwrap();

    // HARBOR_EMBED_DIM measures a Matryoshka-truncated embedder (its own
    // calibration entry; EmbeddingGemma 2 only).
    let dim: Option<u32> = std::env::var("HARBOR_EMBED_DIM")
        .ok()
        .and_then(|d| d.parse().ok());
    let embedder =
        harbor_inference::mrl::TruncatedEmbedder::with_dim(&provider, dim.map(|d| d as usize));

    let skills = builtin_skills().expect("builtin skills parse+validate");
    let prewarm_start = std::time::Instant::now();
    let router = SkillRouter::prewarm_truncated(&skills, &embedder, &model, dim).expect("prewarm");
    let prewarm_ms = prewarm_start.elapsed().as_millis() as u64;

    // Labeled routing corpus: every built-in skill's eval inputs route
    // to the skill that carries them.
    let mut labels: Vec<(String, String, String, String)> = Vec::new(); // (skill_id, input, expect, lang)
    for s in &skills {
        for c in &s.eval_cases {
            labels.push((s.id.clone(), c.input.clone(), c.expect.clone(), "en".into()));
        }
    }
    // Non-English routing inputs (evals/skill_routing/multilingual.json):
    // the built-in eval cases are English-only, so without these the
    // router's behavior for Arabic/French users would be unmeasured.
    let multilingual: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("evals/skill_routing/multilingual.json"))
            .expect("multilingual routing fixture readable"),
    )
    .expect("multilingual routing fixture parses");
    for c in multilingual["cases"].as_array().expect("cases array") {
        labels.push((
            c["skill"].as_str().expect("skill").to_string(),
            c["input"].as_str().expect("input").to_string(),
            String::new(),
            c["lang"].as_str().expect("lang").to_string(),
        ));
    }

    // Measure at the embedder's shipped calibration; an embedder with
    // none is measured at the bge-m3 default so the sweep stays comparable
    // (its router would abstain as Uncalibrated in production).
    let thresholds = router
        .calibration()
        .map(|c| c.thresholds())
        .unwrap_or_default();
    let mut top1 = 0u64;
    let mut top3 = 0u64;
    let mut abstained = 0u64;
    let mut correct_when_confident = 0u64;
    let mut confident = 0u64;
    let mut eval_ms_total = 0u64;
    let mut per_case: Vec<serde_json::Value> = Vec::new();
    let mut score_hist: BTreeMap<String, u64> = BTreeMap::new();

    for (skill_id, input, expect, lang) in &labels {
        let t0 = std::time::Instant::now();
        let d = router
            .evaluate(&embedder, &model, input, thresholds)
            .expect("evaluate");
        eval_ms_total += t0.elapsed().as_millis() as u64;
        let hit1 = d.ranked.first().map(|h| h.skill_id.as_str()) == Some(skill_id.as_str());
        let hit3 = d
            .ranked
            .iter()
            .any(|h| h.skill_id.as_str() == skill_id.as_str());
        if hit1 {
            top1 += 1;
        }
        if hit3 {
            top3 += 1;
        }
        if d.abstained {
            abstained += 1;
        } else {
            confident += 1;
            if hit1 {
                correct_when_confident += 1;
            }
        }
        if let Some(top) = d.ranked.first() {
            let bucket = format!("{:.1}", top.score);
            *score_hist.entry(bucket).or_default() += 1;
        }
        per_case.push(serde_json::json!({
            "skill": skill_id,
            "lang": lang,
            "input": input,
            "expect": expect,
            "top1": d.ranked.first().map(|h| h.skill_id.clone()),
            "top1_score": d.ranked.first().map(|h| h.score),
            "top2": d.ranked.get(1).map(|h| h.skill_id.clone()),
            "top2_score": d.ranked.get(1).map(|h| h.score),
            "abstained": d.abstained,
            "reason": d.reason.map(|r| match r {
                AbstainReason::NoConfidentMatch => "no_confident_match",
                AbstainReason::Ambiguous => "ambiguous",
                AbstainReason::Uncalibrated => "uncalibrated",
            }),
            "hit1": hit1,
            "hit3": hit3,
        }));
    }

    // Threshold calibration sweep (decision 0011: bars are measured,
    // not asserted). The SHIPPED default lives in
    // harbor_core::router::RouteThresholds; this table is how it is
    // chosen and when to move it.
    let mut sweep: Vec<serde_json::Value> = Vec::new();
    for min_score in [0.70f32, 0.75, 0.80, 0.85] {
        for min_margin in [0.0f32, 0.02, 0.05] {
            let mut conf = 0u64;
            let mut right = 0u64;
            for case in &per_case {
                let top_score = case["top1_score"].as_f64().unwrap_or(0.0) as f32;
                let second = case["top2_score"]
                    .as_f64()
                    .unwrap_or(f32::NEG_INFINITY as f64) as f32;
                if top_score < min_score || top_score - second < min_margin {
                    continue;
                }
                conf += 1;
                if case["hit1"].as_bool().unwrap_or(false) {
                    right += 1;
                }
            }
            sweep.push(serde_json::json!({
                "min_score": min_score,
                "min_margin": min_margin,
                "confident": conf,
                "coverage": conf as f64 / labels.len() as f64,
                "precision_when_confident": if conf > 0 { Some(right as f64 / conf as f64) } else { None },
            }));
        }
    }

    let total = labels.len() as f64;
    // Per-language breakdown: a single blended rate would hide an
    // Arabic/French collapse behind the English majority.
    let mut by_lang: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for lang in ["en", "ar", "fr"] {
        let rows: Vec<&serde_json::Value> = per_case
            .iter()
            .filter(|c| c["lang"].as_str() == Some(lang))
            .collect();
        let n = rows.len().max(1) as f64;
        let conf: Vec<&&serde_json::Value> = rows
            .iter()
            .filter(|c| !c["abstained"].as_bool().unwrap_or(true))
            .collect();
        let right = conf
            .iter()
            .filter(|c| c["hit1"].as_bool().unwrap_or(false))
            .count();
        by_lang.insert(
            lang.to_string(),
            serde_json::json!({
                "cases": rows.len(),
                "top1_rate": rows.iter().filter(|c| c["hit1"].as_bool().unwrap_or(false)).count() as f64 / n,
                "top3_rate": rows.iter().filter(|c| c["hit3"].as_bool().unwrap_or(false)).count() as f64 / n,
                "coverage": conf.len() as f64 / n,
                "precision_when_confident": if conf.is_empty() { None } else { Some(right as f64 / conf.len() as f64) },
            }),
        );
    }
    let report = serde_json::json!({
        "model": gguf.display().to_string(),
        "weights_sha256": sha,
        "package_id": package_id,
        "runtime": harbor_inference::runtime_identity(),
        "policy": router.policy().identity(),
        "embedding_dimension": dim,
        "candidates": router.candidate_count(),
        "thresholds": {
            "min_score": thresholds.min_score,
            "min_margin": thresholds.min_margin,
            "top_k": thresholds.top_k,
        },
        "cases": total as u64,
        "top1": top1,
        "top1_rate": top1 as f64 / total,
        "top3": top3,
        "top3_rate": top3 as f64 / total,
        "abstained": abstained,
        "abstain_rate": abstained as f64 / total,
        "confident_cases": confident,
        "precision_when_confident": if confident > 0 {
            Some(correct_when_confident as f64 / confident as f64)
        } else {
            None
        },
        "prewarm_ms": prewarm_ms,
        "evaluate_ms_p50_note": "mean; per-case timings not recorded individually",
        "evaluate_ms_mean": eval_ms_total as f64 / total,
        "by_language": by_lang,
        "top_score_histogram": score_hist,
        "threshold_sweep": sweep,
        "per_case": per_case,
        "command": "cargo test -p harbor_core --test router_live -- --ignored --nocapture",
    });
    let evidence_dir = repo_root().join("evidence/skill_routing");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    let suffix = dim.map(|d| format!("-mrl{d}")).unwrap_or_default();
    let path = evidence_dir.join(format!("live-{}{suffix}.json", &sha[..12]));
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    println!("evidence: {}", path.display());

    // The shipped threshold must keep gross misrouting rare even where
    // it is not perfectly calibrated: an uncalibrated router that always
    // answers is worse than one that abstains. These asserts are the
    // FLOOR (they fail on a broken embedder or a mis-keyed corpus), not
    // a quality claim — the quality numbers are the recorded evidence.
    assert!(
        top1 as f64 / total >= 0.5,
        "top-1 routing {top1}/{total} below the broken-embedder floor"
    );
    assert!(
        correct_when_confident as f64 / confident.max(1) as f64 >= 0.6,
        "confident recommendations are wrong too often — recalibrate thresholds"
    );
}
