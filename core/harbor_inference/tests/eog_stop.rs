//! Upgrade plan C2: generation must stop on ANY end-of-generation token,
//! not only the vocabulary's EOS. Chat models end a turn with a token that
//! is EOG but not EOS (Llama 3 `<|eot_id|>`, ChatML `<|im_end|>`, Gemma
//! `<end_of_turn>`); the old `tok == token_eos()` check ran them to
//! `max_tokens`.
//!
//! Needs the instruct GGUFs in fixtures/models (not committed; large). A
//! missing fixture is reported and skipped, never faked. The package is
//! linked, not copied, into a temp install root (no extra disk use).

#![cfg(feature = "gguf-backend")]

use harbor_canonical::JsonValue;
use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::provider::{Capabilities, ChatRequest, ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageManifest, RuntimeBinding};
use llama_cpp_2::model::AddBos;

fn fixture(name: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models")
        .join(name);
    if p.exists() {
        Some(p)
    } else {
        eprintln!("SKIP: fixture {name} not present");
        None
    }
}

/// Link `weights` into `root/<id>/` with a manifest, as an installed
/// package the provider can load.
fn link_package(root: &std::path::Path, id: &str, weights: &std::path::Path) {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let name = weights.file_name().unwrap().to_str().unwrap().to_string();
    std::os::unix::fs::symlink(weights.canonicalize().unwrap(), dir.join(&name)).unwrap();
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.into(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: name,
            sha256: "0".repeat(64),
            size_bytes: std::fs::metadata(weights).unwrap().len(),
        }],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    std::fs::write(
        dir.join("harbor_manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

/// (fixture file, end-of-turn markers that must all be EOG)
const CASES: &[(&str, &[&str])] = &[
    (
        "Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        &["<|eot_id|>", "<|end_of_text|>"],
    ),
    (
        "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        &["<|im_end|>", "<|endoftext|>"],
    ),
];

#[test]
fn end_of_turn_tokens_are_eog_and_generation_stops_on_them() {
    let root = tempfile::tempdir().unwrap();
    let provider = GgufLlamaCppProvider::new(root.path())
        .unwrap()
        .with_context_tokens(1024);
    let mut ran = 0;
    for (file, markers) in CASES {
        let Some(weights) = fixture(file) else {
            continue;
        };
        ran += 1;
        let id = file.trim_end_matches(".gguf").to_lowercase();
        link_package(root.path(), &id, &weights);
        let m = ModelRef::InstalledPackage {
            package_id: id.clone(),
        };
        provider.load(&m).unwrap();
        let model = provider.loaded_model(&id).unwrap();

        // Every end-of-turn marker is a single special token that the
        // binding reports as EOG; at least one of them is NOT the EOS
        // token, i.e. the old EOS-only check would have run past it.
        let eos = model.token_eos();
        let mut non_eos_eog = Vec::new();
        for marker in *markers {
            let toks = model.str_to_token(marker, AddBos::Never).unwrap();
            assert_eq!(toks.len(), 1, "{file}: {marker} is one special token");
            assert!(model.is_eog_token(toks[0]), "{file}: {marker} must be EOG");
            if toks[0] != eos {
                non_eos_eog.push(*marker);
            }
        }
        assert!(
            !non_eos_eog.is_empty(),
            "{file}: expected an EOG marker distinct from EOS ({eos:?})"
        );
        eprintln!("{file}: EOS={eos:?}; EOG-but-not-EOS markers: {non_eos_eog:?}");

        // A short answer ends well before max_tokens, with no marker text
        // leaking into the content.
        let max_tokens = 200;
        let r = provider
            .generate(ChatRequest {
                model: m,
                messages: vec![JsonValue::object([
                    ("role", JsonValue::str("user")),
                    (
                        "content",
                        JsonValue::str("Reply with exactly one word: hello"),
                    ),
                ])],
                max_tokens,
                temperature: 0.0,
                requires: vec![Capabilities::Chat],
                response_schema: None,
                trace_key: None,
            })
            .unwrap();
        eprintln!(
            "{file}: {} completion tokens: {:?}",
            r.usage.completion_tokens, r.content
        );
        assert!(
            r.usage.completion_tokens < 20,
            "{file}: generation must stop at end of turn, ran {} tokens: {:?}",
            r.usage.completion_tokens,
            r.content
        );
        for marker in *markers {
            assert!(!r.content.contains(marker), "{file}: {:?}", r.content);
        }
    }
    if ran == 0 {
        eprintln!("SKIP: no instruct fixture present; C2 not exercised live");
    }
}
