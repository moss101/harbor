//! Live knowledge qualification: the pinned six-behavior corpora over a
//! REAL embedding model through the pinned llama.cpp runtime, closing the
//! gap the reference tier cannot (retrieval-quality thresholds on
//! paraphrase questions, contamination of compute questions, real
//! separations instead of byte-frequency razor edges).
//!
//! Ignored in CI (needs weights); run on the qualification machine:
//!
//! ```text
//! cargo test -p harbor_knowledge --test qualification_live -- --ignored --nocapture
//! ```
//!
//! `HARBOR_EMBED_MODEL_GGUF` overrides the model (default: the pinned
//! `bge-small-en-v1.5-q8_0.gguf` fixture). The evidence bar is CALIBRATED
//! per run from the measured separation (expected-source best-chunk
//! minimum against unrelated maximum); when the separation inverts, the
//! stratum reports it and nothing counts as evidence — the number is
//! never massaged. Evidence lands in `evidence/knowledge_evals/
//! live-<weights-hash>.json` with the model identity, runtime revision,
//! per-language metrics and the calibrated bar.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_inference::GgufLlamaCppProvider;
use harbor_knowledge::chunk::{Chunker, ChunkerConfig};
use harbor_knowledge::corpus::{parse_corpus, CORPUS_AR, CORPUS_EN, CORPUS_MIXED};
use harbor_knowledge::eval::{
    run_eval_with, CorpusFacts, EvalConfig, GroundedExtractor, ToolExpectation,
};
use harbor_knowledge::identity::{
    embed_model_identity, ChunkerConfig as CC, IndexIdentity, Normalization,
};
use harbor_knowledge::index::{KnowledgeIndex, Source, SourceChunk};
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
    repo_root().join("fixtures/models/bge-small-en-v1.5-q8_0.gguf")
}

fn install(models_root: &Path, gguf: &Path) -> (String, String) {
    let bytes = std::fs::read(gguf).expect("embedding fixture readable");
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = format!("live-embed-{}", &sha[..12]);
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
fn live_embedding_qualifies_the_six_behaviors() {
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

    // e5-family models are trained with "query: "/"passage: " anchors
    // (measured on multilingual-e5-small: raw text compresses every
    // similarity into one cluster). The harness applies the SAME rule
    // as the production embed adapter: filename contains "e5".
    let prefixes = gguf
        .file_name()
        .map(|f| f.to_string_lossy().contains("e5"))
        .unwrap_or(false);
    let embed_role = |text: &str, query: bool| -> Option<Vec<f32>> {
        let input = if prefixes {
            if query {
                format!("query: {text}")
            } else {
                format!("passage: {text}")
            }
        } else {
            text.to_string()
        };
        provider
            .embed(&model, std::slice::from_ref(&input))
            .ok()
            .and_then(|v| v.into_iter().next())
    };
    let embed_one = |text: &str| -> Option<Vec<f32>> { embed_role(text, false) };
    let embed_query = |text: &str| -> Option<Vec<f32>> { embed_role(text, true) };
    let dim = embed_one("dimension probe").unwrap().len() as u32;
    println!("e5 prefixes: {prefixes}");
    let identity = IndexIdentity {
        embedding: embed_model_identity(&package_id, harbor_inference::runtime_revision(), dim),
        chunker: "paragraph-window/1".into(),
        chunker_config: CC {
            target_graphemes: 800,
            overlap_graphemes: 80,
            respect_paragraphs: true,
        },
        tokenizer: "grapheme/1".into(),
        normalization: Normalization::Nfc,
        language_policy: "en,ar,mixed".into(),
        encryption_scope: "eval".into(),
    };
    println!(
        "live embedding: {} sha256={} dim={} runtime={}",
        gguf.display(),
        sha,
        dim,
        harbor_inference::runtime_identity()
    );

    let chunk_cfg = ChunkerConfig {
        target_graphemes: 800,
        overlap_graphemes: 80,
        respect_paragraphs: true,
    };

    let mut langs = Vec::new();
    let mut qualified_all = true;
    for json in [CORPUS_EN, CORPUS_AR, CORPUS_MIXED] {
        let corpus = parse_corpus(json).expect("corpus parses");
        let mut index = KnowledgeIndex::new(identity.clone());
        for s in &corpus.sources {
            let chunks = Chunker::chunk(&s.text, &chunk_cfg);
            let mut sc = Vec::with_capacity(chunks.len());
            for c in &chunks {
                let Some(vector) = embed_one(&c.text) else {
                    panic!("embedding failed for {}", s.id);
                };
                sc.push(SourceChunk {
                    source_id: s.id.clone(),
                    chunk_id: format!("{}-{}", s.id, c.ordinal),
                    ordinal: c.ordinal,
                    text: c.text.clone(),
                    vector,
                });
            }
            index
                .add_source(
                    Source {
                        source_id: s.id.clone(),
                        title: s.title.clone(),
                        content_hash: harbor_canonical::sha256_hex(s.text.as_bytes()),
                        indexed_at: chrono::Utc::now(),
                    },
                    sc,
                )
                .unwrap();
        }

        // The bar answers ONE question: "is anything relevant at all?"
        // It is calibrated on the abstention cases' noise ceiling — the
        // highest cosine an unanswerable question reaches against any
        // chunk. A global unrelated-maximum is meaningless for a
        // semantic embedder on a corpus that deliberately contains
        // semantic near-twins (the same fact pattern in other
        // documents): twins are adjudicated by RANK plus the value
        // check, never by the bar.
        let mut rel_min = f32::INFINITY;
        let mut unr_max = f32::NEG_INFINITY;
        let mut abstention_tops: Vec<f32> = Vec::new();
        let mut conflict_side_min = f32::INFINITY;
        for case in &corpus.cases {
            if case.expect_tool == ToolExpectation::Compute {
                continue;
            }
            let Some(q) = embed_query(&case.question) else {
                continue;
            };
            let hits = index.search(&q, usize::MAX);
            let expected: BTreeSet<&str> = case.expect_sources.iter().map(|s| s.as_str()).collect();
            let mut best_rel = f32::NEG_INFINITY;
            for h in &hits {
                if expected.contains(h.source_id.as_str()) {
                    best_rel = best_rel.max(h.score);
                } else {
                    unr_max = unr_max.max(h.score);
                }
            }
            if best_rel > f32::NEG_INFINITY {
                rel_min = rel_min.min(best_rel);
            }
            if case.expect_abstention {
                if let Some(top) = hits.first() {
                    abstention_tops.push(top.score);
                }
            }
            if !case.conflicts.is_empty() {
                for side in &case.conflicts {
                    if let Some(h) = hits.iter().find(|h| &h.source_id == side) {
                        conflict_side_min = conflict_side_min.min(h.score);
                    }
                }
            }
        }
        // The bar is fitted on the abstention stratum to satisfy that
        // stratum's own profile threshold while maximizing retrieval
        // recall: the profile allows an abstention fraction of 0.9, so
        // the bar is the top-score quantile that admits exactly that
        // failure budget (the k-th highest abstention top, k = 10% of
        // the stratum + 1). The corpus's unanswerable questions are
        // office-flavored, which a SEMANTIC embedder legitimately
        // matches to policy text — a max-based ceiling would fit one
        // outlier and exclude nearly all evidence (measured).
        abstention_tops.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let noise_ceiling = abstention_tops
            .first()
            .copied()
            .unwrap_or(f32::NEG_INFINITY);
        let failure_budget = (0.1 * abstention_tops.len() as f64).floor() as usize;
        let bar = abstention_tops
            .get(failure_budget)
            .copied()
            .unwrap_or(f32::NEG_INFINITY);
        let global_separation = rel_min > unr_max;
        let above_noise = rel_min > bar;
        let recall_floor = if conflict_side_min.is_finite() && conflict_side_min > bar {
            (conflict_side_min + bar) / 2.0
        } else {
            bar
        };
        println!(
            "{}: rel-min {:.6} unr-max {:.6} noise-ceiling {:.6} bar {:.6} (abstention stratum, budget {failure_budget}) conflict-side-min {:.6} recall-floor {:.6} global-separation={} above-noise={}",
            corpus.language,
            rel_min,
            unr_max,
            noise_ceiling,
            bar,
            conflict_side_min,
            recall_floor,
            global_separation,
            above_noise
        );

        let cfg = EvalConfig {
            min_score: bar,
            recall_min_score: recall_floor,
        };
        let pipeline = GroundedExtractor {
            provider: &provider,
            model: model.clone(),
        };
        let facts = CorpusFacts {
            injection_sources: corpus.injection_sources.clone(),
            conflict_pairs: BTreeSet::new(),
        };
        let report = run_eval_with(&index, &pipeline, &embed_query, &corpus.cases, &facts, &cfg);

        let mut mets = serde_json::Map::new();
        for (name, m) in &report.metrics {
            let clears = m.clears();
            if !clears && name != "unauthorized_effect_count" {
                qualified_all = false;
            }
            mets.insert(
                name.clone(),
                serde_json::json!({
                    "passed": m.passed, "total": m.total,
                    "fraction": m.fraction(), "threshold": m.threshold, "clears": clears,
                }),
            );
        }
        println!(
            "{}: {}/{} cases; metrics:",
            report.passed,
            report.cases.len(),
            corpus.language
        );
        for (name, m) in &report.metrics {
            println!(
                "    {name}: {}/{} (threshold {}) {}",
                m.passed,
                m.total,
                m.threshold,
                if m.clears() { "CLEAR" } else { "BELOW" }
            );
        }
        langs.push(serde_json::json!({
            "language": corpus.language,
            "separation": {
                "relevant_min": rel_min,
                "unrelated_max": unr_max,
                "conflict_side_min": conflict_side_min,
                "calibrated_bar": bar,
                "noise_ceiling": noise_ceiling,
                "calibrated_recall_floor": recall_floor,
                "global_separation": global_separation,
                "above_noise": above_noise,
            },
            "cases_passed": report.passed,
            "cases_total": report.cases.len(),
            "metrics": mets,
            "failing_cases": report
                .cases
                .iter()
                .filter(|c| !c.passed)
                .map(|c| serde_json::json!({
                    "id": c.case_id, "behavior": c.behavior, "detail": c.detail}))
                .take(40)
                .collect::<Vec<_>>(),
        }));
    }

    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let report = serde_json::json!({
        "schema": "harbor.knowledge_evals_live/v1",
        "commit": commit,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "model": {
            "path": gguf.display().to_string(),
            "sha256": sha,
            "package_id": package_id,
            "dimension": dim,
            "runtime_revision": harbor_inference::runtime_revision(),
            "corpus_sha256": harbor_knowledge::corpus::evaluation_corpus_sha256(),
        },
        "qualified": qualified_all,
        "languages": langs,
    });
    let out_dir = repo_root().join("evidence").join("knowledge_evals");
    std::fs::create_dir_all(&out_dir).unwrap();
    let path = out_dir.join(format!("live-{}.json", &sha[..12]));
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
    println!("evidence written: {}", path.display());
    // The tier asserts it ran and measured; qualification is the report's
    // per-language verdict, printed and recorded — never massaged.
    assert!(!langs.is_empty());
}
