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

/// The grammar must hold for the schema shapes the product actually ships,
/// not only for a flat object.
///
/// `structured_output_is_grammar_constrained_on_the_real_runtime` above
/// proves the constraint with two scalar properties. Every real graph
/// schema is richer than that: `meeting-notes` nests an array of objects
/// and marks two fields `["string","null"]`. A run of that graph on iOS
/// returned an ARRAY where the root schema says object — which a grammar,
/// if it were doing its job, could not emit. That is the gap this covers:
/// the guarantee was verified where it was easy and assumed where it was
/// hard.
///
/// stories260K cannot follow instructions at all, so anything well-formed
/// here is the grammar's doing and nothing else.
#[test]
fn the_grammar_holds_for_a_nested_schema_with_a_nullable_union() {
    let dir = tempfile::tempdir().unwrap();
    let package = install_test_model(dir.path());
    let provider = GgufLlamaCppProvider::new(dir.path())
        .unwrap()
        .with_context_tokens(512);
    let m = ModelRef::InstalledPackage {
        package_id: package,
    };
    provider.load(&m).unwrap();
    // The shape of meeting-notes/minutes, reduced but structurally faithful:
    // object root, array of objects, and a nullable string.
    let schema = harbor_canonical::parse(
        r#"{"type":"object","properties":{
             "summary":{"type":"string","maxLength":40},
             "actions":{"type":"array","maxItems":2,"items":{
               "type":"object",
               "properties":{"text":{"type":"string","maxLength":20},
                             "owner":{"type":["string","null"],"maxLength":20}},
               "required":["text","owner"],"additionalProperties":false}}},
           "required":["summary","actions"],"additionalProperties":false}"#,
    )
    .unwrap();
    let req = ChatRequest {
        model: m,
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str("Summarise a meeting as JSON.")),
        ])],
        max_tokens: 120,
        temperature: 0.0,
        requires: vec![Capabilities::Chat, Capabilities::StructuredOutput],
        response_schema: Some(schema),
        trace_key: None,
    };
    let resp = provider.generate(req).unwrap();
    println!("nested structured output: {:?}", resp.content);
    let v: serde_json::Value = serde_json::from_str(resp.content.trim())
        .unwrap_or_else(|e| panic!("must parse as JSON: {e}: {:?}", resp.content));
    // The root is the whole point: an array here is the iOS failure.
    let obj = v
        .as_object()
        .unwrap_or_else(|| panic!("root must be an object, got {v}"));
    assert!(
        obj.get("summary").map(|s| s.is_string()).unwrap_or(false),
        "{v}"
    );
    let actions = obj
        .get("actions")
        .and_then(|a| a.as_array())
        .unwrap_or_else(|| panic!("actions must be an array: {v}"));
    for a in actions {
        let ao = a
            .as_object()
            .unwrap_or_else(|| panic!("action must be an object: {v}"));
        assert!(
            ao.get("text").map(|t| t.is_string()).unwrap_or(false),
            "{v}"
        );
        let owner = ao
            .get("owner")
            .unwrap_or_else(|| panic!("owner required: {v}"));
        assert!(
            owner.is_string() || owner.is_null(),
            "owner must be string|null: {v}"
        );
        assert_eq!(ao.len(), 2, "additionalProperties=false must hold: {v}");
    }
    assert_eq!(
        obj.len(),
        2,
        "additionalProperties=false must hold at root: {v}"
    );
}

/// The exact schema that failed on iOS, loaded from the shipped graph.
///
/// The reduced version above passes, so the grammar handles nesting and
/// nullable unions in principle. This pins the real thing — 5 properties,
/// `maxItems: 100`, an enum, and an array of 3-field objects — because a
/// run of this graph on an iPhone simulator came back with an ARRAY at a
/// root the schema declares an object, after a grammar was supposedly
/// applied. Either the grammar silently weakens on this schema, or the
/// fault is elsewhere; a reduced proxy cannot tell us which, and this is
/// the only version that ships.
#[test]
fn the_grammar_holds_for_the_shipped_meeting_notes_schema() {
    let graph: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("core/harbor_core/src/graphs/meeting-notes.json")).unwrap(),
    )
    .unwrap();
    let node_schema = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "minutes")
        .expect("minutes node")["output_schema"]
        .clone();
    assert_eq!(
        node_schema["type"], "object",
        "precondition: root is object"
    );
    let schema = harbor_canonical::convert(node_schema).expect("shipped schema canonicalises");

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
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str("Turn this into minutes.")),
        ])],
        max_tokens: 200,
        temperature: 0.0,
        requires: vec![Capabilities::Chat, Capabilities::StructuredOutput],
        response_schema: Some(schema),
        trace_key: None,
    };
    let resp = provider.generate(req).unwrap();
    println!("shipped-schema output: {:?}", resp.content);
    let v: serde_json::Value = serde_json::from_str(resp.content.trim())
        .unwrap_or_else(|e| panic!("must parse as JSON: {e}: {:?}", resp.content));
    assert!(
        v.is_object(),
        "root must be an object — this is exactly the iOS failure: {v}"
    );
}

/// Inspect the GBNF the shipped schema actually compiles to.
///
/// A grammar can be APPLIED and still be wrong. If `json_schema_to_grammar`
/// falls back to a permissive rule for anything it does not understand,
/// the root accepts an array as readily as an object — and then whether a
/// run succeeds depends on which branch greedy decoding happens to take
/// for a given model. That would explain why stories260K yields an object
/// on macOS while qwen2.5-1.5b yields an array on iOS, with no platform
/// difference at all.
///
/// This asserts the property directly, with no model in the loop: the root
/// rule must commit to an object.
#[test]
fn the_shipped_schema_compiles_to_a_grammar_whose_root_is_an_object() {
    let graph: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("core/harbor_core/src/graphs/meeting-notes.json")).unwrap(),
    )
    .unwrap();
    let node_schema = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "minutes")
        .unwrap()["output_schema"]
        .clone();
    let gbnf = llama_cpp_2::json_schema_to_grammar(&serde_json::to_string(&node_schema).unwrap())
        .expect("schema must compile to a grammar");
    println!("--- GBNF for meeting-notes/minutes ---\n{gbnf}\n--- end ---");
    let root = gbnf
        .lines()
        .find(|l| l.trim_start().starts_with("root "))
        .unwrap_or_else(|| panic!("no root rule in grammar:\n{gbnf}"));
    println!("root rule: {root}");
    assert!(
        !root.contains('['),
        "the root rule must not admit an array — this is the iOS failure: {root}"
    );
}

/// What does the recommended model ACTUALLY emit for meeting-notes?
///
/// `#[ignore]` — needs the 1.1 GB catalog model in fixtures/models and is
/// far too slow for CI. Run deliberately:
///   cargo test -p harbor_inference --features gguf-backend --test gguf_provider \
///     -- --ignored what_the_recommended_model_emits --nocapture
///
/// This exists because I claimed, from a truncation alone, that the model
/// "keeps emitting array items rather than closing". That was inference,
/// not observation — the exact move this session has been removing from
/// the code. This prints the real output so the claim can be checked.
#[test]
#[ignore]
fn what_the_recommended_model_emits_for_the_shipped_schema() {
    let weights = repo_root().join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !weights.exists() {
        eprintln!("skipped: {} not present", weights.display());
        return;
    }
    let bytes = std::fs::read(&weights).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let dir = tempfile::tempdir().unwrap();
    let installer = PackageInstaller::new(dir.path());
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: "qwen".into(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: "qwen.gguf".into(),
            sha256: sha.clone(),
            size_bytes: bytes.len() as u64,
        }],
        runtime: RuntimeBinding {
            kind: "gguf/llama.cpp".into(),
            min_revision: "0.1.156".into(),
            targets: vec![std::env::consts::ARCH.to_string()],
        },
    };
    let mut staged = installer.begin("qwen").unwrap();
    installer
        .ingest_file(&mut staged, &manifest.files[0], &bytes)
        .unwrap();
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();

    let graph: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("core/harbor_core/src/graphs/meeting-notes.json")).unwrap(),
    )
    .unwrap();
    let node = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "minutes")
        .unwrap()
        .clone();
    let schema = harbor_canonical::convert(node["output_schema"].clone()).unwrap();
    let max_tokens = node["max_tokens"].as_u64().unwrap() as u32;

    let provider = GgufLlamaCppProvider::new(dir.path())
        .unwrap()
        .with_context_tokens(4096);
    let m = ModelRef::InstalledPackage {
        package_id: "qwen".into(),
    };
    provider.load(&m).unwrap();
    let system = format!(
        "{}\n\nRespond with a single JSON value that conforms to this JSON Schema and nothing else:\n{}",
        node["instructions"].as_str().unwrap(),
        serde_json::to_string(&node["output_schema"]).unwrap()
    );
    let req = ChatRequest {
        model: m,
        messages: vec![
            JsonValue::object([
                ("role", JsonValue::str("system")),
                ("content", JsonValue::str(system)),
            ]),
            JsonValue::object([
                ("role", JsonValue::str("user")),
                (
                    "content",
                    // Exactly what render_context produces for this node:
                    // labelled sections, and "(none)" for the optional
                    // language the iOS run left empty. My first pass here
                    // hand-wrote "Transcript: ..." instead, which is NOT
                    // the prompt the executor sends — so a pass proved
                    // nothing about the run that failed.
                    JsonValue::str(
                        "## Target language (if any)\n(none)\n\n                         ## Transcript\nAna: ship the fix today.                          Ben: I run the checklist tomorrow.\n\n",
                    ),
                ),
            ]),
        ],
        max_tokens,
        temperature: 0.0,
        requires: vec![Capabilities::Chat, Capabilities::StructuredOutput],
        response_schema: Some(schema),
        trace_key: None,
    };
    let resp = provider.generate(req).unwrap();
    println!("=== completion_tokens: {}", resp.usage.completion_tokens);
    println!(
        "=== hit budget: {}",
        resp.usage.completion_tokens >= max_tokens as u64
    );
    println!("=== output ===\n{}\n=== end ===", resp.content);
    let parsed = serde_json::from_str::<serde_json::Value>(resp.content.trim());
    println!("=== parses as JSON: {}", parsed.is_ok());
}
