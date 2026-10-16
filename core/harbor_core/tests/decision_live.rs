//! Live measurement of the action-kind decision (decision 0019): which kind
//! of action does a request need? Labeled corpus `evals/decision/intents.json`
//! (EN/AR/FR). Prints the per-language table and the threshold sweep, writes
//! evidence, and states the bars the selection rule picks (highest coverage
//! with precision-when-confident >= 0.75) for `decision_calibration.json`.
//!
//! ```text
//! HARBOR_EMBED_MODEL_GGUF=fixtures/models/embeddinggemma-2-Q8_0.gguf \
//!   cargo test -p harbor_core --test decision_live -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use harbor_core::decision::intent_catalog;
use harbor_core::router::{DecisionTask, RouteThresholds, SkillRouter};
use harbor_inference::provider::ModelRef;
use harbor_inference::{GgufLlamaCppProvider, ModelProvider};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
#[ignore = "needs an embedding GGUF (HARBOR_EMBED_MODEL_GGUF); qualification-machine tier"]
fn live_decision_picks_the_kind_of_action() {
    let gguf = std::env::var("HARBOR_EMBED_MODEL_GGUF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("fixtures/models/bge-m3-q8_0.gguf"));
    let bytes = std::fs::read(&gguf).expect("embedding model readable");
    let sha = harbor_canonical::sha256_hex(&bytes);
    // The id carries the model family so the instruction policy and the
    // calibration lookup resolve exactly as in production.
    let family = gguf.file_name().unwrap().to_string_lossy().to_lowercase();
    let family = if family.contains("embeddinggemma") {
        "embeddinggemma2"
    } else if family.contains("bge-m3") {
        "bge-m3"
    } else if family.contains("e5") {
        "multilingual-e5-small"
    } else {
        "embed"
    };
    let id = format!("{family}-{}", &sha[..12]);
    let dir = tempfile::tempdir().unwrap();
    let models_root = dir.path().join("models");
    std::fs::create_dir_all(&models_root).unwrap();
    let installer = PackageInstaller::new(&models_root);
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: gguf.file_name().unwrap().to_string_lossy().to_string(),
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
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    let provider = GgufLlamaCppProvider::new(&models_root).unwrap();
    let model = ModelRef::InstalledPackage {
        package_id: id.clone(),
    };
    provider.load(&model).unwrap();

    let router = SkillRouter::prewarm_items(
        &intent_catalog(),
        DecisionTask::Intent,
        &provider,
        &model,
        None,
    )
    .unwrap();
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("evals/decision/intents.json")).unwrap(),
    )
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();

    // Score every case once at "no bars", then sweep.
    let open = RouteThresholds {
        min_score: -1.0,
        min_margin: 0.0,
        top_k: 3,
    };
    struct Row {
        lang: String,
        intent: String,
        top: String,
        s1: f32,
        s2: f32,
        hit3: bool,
        input: String,
    }
    let mut rows = Vec::new();
    for c in cases {
        let d = router
            .evaluate(&provider, &model, c["input"].as_str().unwrap(), open)
            .unwrap();
        let want = c["intent"].as_str().unwrap();
        rows.push(Row {
            lang: c["lang"].as_str().unwrap().into(),
            intent: want.into(),
            top: d.ranked[0].skill_id.clone(),
            s1: d.ranked[0].score,
            s2: d
                .ranked
                .get(1)
                .map(|h| h.score)
                .unwrap_or(f32::NEG_INFINITY),
            hit3: d.ranked.iter().any(|h| h.skill_id == want),
            input: c["input"].as_str().unwrap().into(),
        });
    }
    for r in rows.iter().filter(|r| r.top != r.intent && r.lang == "en") {
        println!(
            "WRONG want={} got={} ({:.2}): {}",
            r.intent, r.top, r.s1, r.input
        );
    }
    let n = rows.len() as f64;
    let top1 = rows.iter().filter(|r| r.top == r.intent).count() as f64 / n;
    let top3 = rows.iter().filter(|r| r.hit3).count() as f64 / n;
    println!(
        "MODEL {id}: cases={} top1={:.3} top3={:.3}",
        rows.len(),
        top1,
        top3
    );
    let mut by_lang: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for r in &rows {
        let e = by_lang.entry(r.lang.clone()).or_default();
        e.0 += 1;
        e.1 += usize::from(r.top == r.intent);
    }
    for (l, (t, h)) in &by_lang {
        println!("LANG {l}: top1 {h}/{t}");
    }
    let mut sweep = Vec::new();
    let mut best: Option<(f64, f32, f32, f64)> = None;
    for ms in [0.3f32, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9] {
        for mm in [0.0f32, 0.05, 0.1, 0.2, 0.3] {
            let conf: Vec<&Row> = rows
                .iter()
                .filter(|r| r.s1 >= ms && r.s1 - r.s2 >= mm)
                .collect();
            let cov = conf.len() as f64 / n;
            let prec = if conf.is_empty() {
                f64::NAN
            } else {
                conf.iter().filter(|r| r.top == r.intent).count() as f64 / conf.len() as f64
            };
            println!("SWEEP min_score={ms} min_margin={mm}: coverage={cov:.3} precision={prec:.3}");
            sweep.push(serde_json::json!({"min_score": ms, "min_margin": mm, "coverage": cov, "precision": prec}));
            if prec >= 0.75 && best.is_none_or(|b| cov > b.0) {
                best = Some((cov, ms, mm, prec));
            }
        }
    }
    println!("CHOSEN {family}: {best:?}");
    // The confusion pairs worth knowing about.
    let mut conf: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.top != r.intent) {
        *conf.entry((r.intent.clone(), r.top.clone())).or_default() += 1;
    }
    println!("CONFUSIONS {conf:?}");
    let out = repo_root().join("evidence/decision");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join(format!("intent-{}.json", &sha[..12])),
        serde_json::to_string_pretty(&serde_json::json!({
            "model": id, "sha256": sha, "cases": rows.len(), "top1": top1, "top3": top3,
            "sweep": sweep, "chosen": best,
        }))
        .unwrap(),
    )
    .unwrap();
}
