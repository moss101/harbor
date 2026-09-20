//! Real-model GGUF inference test against the pinned llama.cpp runtime.
//!
//! Uses the tiny stories260K model (a real, trained LLM distributed by the
//! ggml-org project for engine tests). Its SHA-256 is pinned below; the
//! file lives in fixtures/models and is installed through the real
//! harbor_modelhub staged-install path before inference.

#![cfg(feature = "gguf-backend")]

use std::sync::atomic::AtomicBool;

use harbor_canonical::JsonValue;
use harbor_inference::gguf::{runtime_revision, GgufLlamaCppProvider};
use harbor_inference::provider::{Capabilities, ChatRequest, ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

const MODEL_SHA256: &str = "270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d";

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn install_test_model(dir: &std::path::Path) -> String {
    let bytes = std::fs::read(repo_root().join("fixtures/models/stories260K.gguf"))
        .expect("test model fixture present");
    assert_eq!(
        harbor_canonical::sha256_hex(&bytes),
        MODEL_SHA256,
        "model fixture integrity"
    );
    let installer = PackageInstaller::new(dir);
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: "stories260k".into(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: "stories260K.gguf".into(),
            sha256: MODEL_SHA256.into(),
            size_bytes: bytes.len() as u64,
        }],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin("stories260k").unwrap();
    installer
        .ingest_file(&mut staged, &manifest.files[0], &bytes)
        .unwrap();
    let report = installer.validate(&staged, &manifest).unwrap();
    assert!(report.ok, "validation problems: {:?}", report.problems);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    "stories260k".to_string()
}

#[test]
fn real_model_loads_and_generates_offline() {
    let dir = tempfile::tempdir().unwrap();
    let package = install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path())
        .unwrap()
        .with_context_tokens(512);

    // Capability + support checks are honest: the weights exist, chat works,
    // vision does not.
    let m = ModelRef::InstalledPackage {
        package_id: package.clone(),
    };
    assert!(provider.supports(&m, &Capabilities::Chat));
    assert!(!provider.supports(&m, &Capabilities::Vision));

    provider.load(&m).unwrap();
    let req = ChatRequest {
        model: m,
        messages: vec![
            JsonValue::object([
                ("role", JsonValue::str("system")),
                ("content", JsonValue::str("You are a story writer.")),
            ]),
            JsonValue::object([
                ("role", JsonValue::str("user")),
                (
                    "content",
                    JsonValue::str("Write one short sentence about a dog."),
                ),
            ]),
        ],
        max_tokens: 24,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
        response_schema: None,
        trace_key: None,
    };
    let resp = provider.generate(req).unwrap();
    println!("runtime: {}", runtime_revision());
    println!("output: {:?}", resp.content);
    println!(
        "usage: {} prompt / {} completion",
        resp.usage.prompt_tokens, resp.usage.completion_tokens
    );
    // The model ran on-device under its own package identity.
    assert_eq!(resp.executed_on, "stories260k");
    assert_eq!(
        resp.execution_location,
        harbor_security::policy::ExecutionLocation::OnDevice
    );
    assert!(resp.usage.prompt_tokens > 0);
    assert!(resp.usage.completion_tokens > 0);
    assert!(resp.usage.completion_tokens <= 24);
    assert!(
        !resp.content.trim().is_empty(),
        "greedy decode must produce text"
    );
}

#[test]
fn missing_model_is_a_typed_error_not_a_substitute() {
    let dir = tempfile::tempdir().unwrap();
    install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path()).unwrap();
    let ghost = ModelRef::InstalledPackage {
        package_id: "not-installed".into(),
    };
    assert!(matches!(
        provider.load(&ghost),
        Err(harbor_inference::provider::ProviderError::ModelNotFound(_))
    ));
}

#[test]
fn generation_is_cancellable() {
    let dir = tempfile::tempdir().unwrap();
    let package = install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path()).unwrap();
    let m = ModelRef::InstalledPackage {
        package_id: package,
    };
    provider.load(&m).unwrap();
    let cancel = AtomicBool::new(true);
    let req = ChatRequest {
        model: m,
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str("Once upon a time")),
        ])],
        max_tokens: 64,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
        response_schema: None,
        trace_key: None,
    };
    let r = provider.generate_cancellable(req, &cancel, None);
    assert!(
        matches!(r, Err(harbor_inference::provider::ProviderError::Cancelled)),
        "pre-cancelled request must return Cancelled"
    );
}

#[test]
fn real_model_generates_via_native_template_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let package = install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path())
        .unwrap()
        .with_context_tokens(512);
    let m = ModelRef::InstalledPackage {
        package_id: package,
    };
    provider.load(&m).unwrap();
    let req = ChatRequest {
        model: m,
        messages: vec![
            JsonValue::object([
                ("role", JsonValue::str("system")),
                ("content", JsonValue::str("You write stories.")),
            ]),
            JsonValue::object([
                ("role", JsonValue::str("user")),
                ("content", JsonValue::str("One sentence about a cat.")),
            ]),
        ],
        max_tokens: 16,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
        response_schema: None,
        trace_key: None,
    };
    let r1 = provider.generate(req.clone()).unwrap();
    let r2 = provider.generate(req).unwrap();
    // Greedy decoding is deterministic: identical prompts -> identical output.
    assert_eq!(r1.content, r2.content);
    assert!(!r1.content.is_empty());
}

#[test]
fn embeddings_via_mean_pooling_are_deterministic_and_typed() {
    let dir = tempfile::tempdir().unwrap();
    let package = install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path()).unwrap();
    let m = ModelRef::InstalledPackage {
        package_id: package,
    };
    provider.load(&m).unwrap();
    let a = provider
        .embed(&m, &["The board approved the budget.".to_string()])
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        !a.is_empty(),
        "mean-pooled embedding must have model dimension"
    );
    let b = provider
        .embed(&m, &["The board approved the budget.".to_string()])
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(a, b, "same text must embed identically");
    // Different text embeds to a different vector.
    let c = provider
        .embed(&m, &["Something completely unrelated.".to_string()])
        .unwrap()
        .pop()
        .unwrap();
    assert_ne!(a, c);
}

#[test]
fn structured_output_is_grammar_constrained_on_the_real_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let package = install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path())
        .unwrap()
        .with_context_tokens(512);
    let m = ModelRef::InstalledPackage {
        package_id: package.clone(),
    };
    assert!(provider.supports(&m, &Capabilities::StructuredOutput));
    provider.load(&m).unwrap();
    // stories260K cannot follow instructions; the grammar alone must make
    // the output a JSON object with exactly these keys and types.
    let schema = harbor_canonical::parse(
        r#"{"type":"object","properties":{"word":{"type":"string","maxLength":12},"count":{"type":"integer"}},"required":["word","count"],"additionalProperties":false}"#,
    )
    .unwrap();
    let req = ChatRequest {
        model: m,
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str("Describe a dog as JSON.")),
        ])],
        max_tokens: 40,
        temperature: 0.0,
        requires: vec![Capabilities::Chat, Capabilities::StructuredOutput],
        response_schema: Some(schema),
        trace_key: None,
    };
    let resp = provider.generate(req).unwrap();
    println!("structured output: {:?}", resp.content);
    let v: serde_json::Value = serde_json::from_str(resp.content.trim()).unwrap_or_else(|e| {
        panic!(
            "grammar-constrained output must parse as JSON: {e}: {:?}",
            resp.content
        )
    });
    let obj = v.as_object().expect("object");
    assert!(
        obj.get("word").map(|w| w.is_string()).unwrap_or(false),
        "{v}"
    );
    assert!(
        obj.get("count")
            .map(|c| c.is_i64() || c.is_u64())
            .unwrap_or(false),
        "{v}"
    );
    assert_eq!(obj.len(), 2, "additionalProperties=false must hold: {v}");
}

/// A prompt longer than llama.cpp's default batch (512 tokens) used to trip
/// `GGML_ASSERT(n_tokens_all <= cparams.n_batch)` and abort the whole
/// process; prefill is now chunked. A prompt that cannot fit the model's
/// context at all is refused with a typed error instead.
#[test]
fn long_prompts_are_prefilled_in_chunks_and_oversized_prompts_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let package = install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path())
        .unwrap()
        .with_context_tokens(1024);
    let m = ModelRef::InstalledPackage {
        package_id: package.clone(),
    };
    provider.load(&m).unwrap();
    let filler = "The dog ran to the park and played with the ball. ".repeat(80);
    let req = |content: String, max_tokens: u32| ChatRequest {
        model: m.clone(),
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str(content)),
        ])],
        max_tokens,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
        response_schema: None,
        trace_key: None,
    };
    // ~800+ tokens: more than one default batch, still inside the
    // stories260K context (2048).
    let resp = provider.generate(req(filler.clone(), 8)).unwrap();
    assert!(
        resp.usage.prompt_tokens > 512,
        "{}",
        resp.usage.prompt_tokens
    );
    assert!(resp.usage.completion_tokens >= 1);
    // Far beyond the trained context: a typed error, not an abort.
    let err = provider.generate(req(filler.repeat(6), 8)).unwrap_err();
    assert!(
        err.to_string().contains("exceeds the model context"),
        "{err}"
    );
}
