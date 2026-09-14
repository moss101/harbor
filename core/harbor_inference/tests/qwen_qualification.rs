//! Production-model qualification: Qwen2.5-1.5B-Instruct (Apache-2.0,
//! official Qwen repository) acquired through the brokered in-app path and
//! qualified for chat on this device. This is the model package whose hash
//! is bound as `model_package_sha256` in 26_Qualification_Profiles.json.

#![cfg(feature = "gguf-backend")]

use std::collections::BTreeMap;

use harbor_canonical::JsonValue;
use harbor_inference::provider::{Capabilities, ChatRequest, ModelProvider, ModelRef};
use harbor_inference::GgufLlamaCppProvider;
use harbor_modelhub::acquire::{HfAcquirer, HF_CDN_ORIGINS};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};
use harbor_modelhub::catalog_signing::{sign_catalog, CatalogSigningKey, CatalogVerifier};
use harbor_net::AuditSink;
use harbor_net::audit::SqliteAuditSink;
use harbor_net::broker::{EgressBroker, EgressClass};
use harbor_net::transport::UreqTransport;
use harbor_security::policy::PrivacyMode;

/// The package hash recorded from the executed acquisition run (see
/// docs/decisions/0004-production-model-selection.md).
pub const QWEN_SHA256: &str = "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e";
pub const QWEN_SIZE: u64 = 1_117_320_736;

fn sessions_for(broker: &EgressBroker) -> BTreeMap<String, harbor_net::broker::EgressSession> {
    let mut sessions = BTreeMap::new();
    for origin in ["https://huggingface.co", HF_CDN_ORIGINS[0], HF_CDN_ORIGINS[1], HF_CDN_ORIGINS[2], HF_CDN_ORIGINS[3]] {
        let s = broker
            .open_session(EgressClass::WeightTransfer, origin, chrono::Duration::minutes(30), PrivacyMode::LocalOnly)
            .unwrap();
        sessions.insert(origin.to_string(), s);
    }
    sessions
}

#[test]
#[ignore = "requires network and ~1.1GB download (run: cargo test -- --ignored)"]
fn acquire_streaming_and_install_production_model() {
    let dir = tempfile::tempdir().unwrap();
    let sink = std::sync::Arc::new(SqliteAuditSink::open_in_memory().unwrap());
    let broker = EgressBroker::new(Box::new(sink.clone()));
    let transport = UreqTransport::new();
    let installer = PackageInstaller::new(dir.path().join("models"));
    let sessions = sessions_for(&broker);
    let acquirer = HfAcquirer {
        broker: &broker,
        transport: &transport,
        installer: &installer,
        sessions,
        auth_token: None,
        progress: None,
    };
    // Streaming path: 1.1GB acquired in 64KiB chunks with incremental
    // SHA-256; the pinned hash enforces package identity.
    let result = acquirer
        .acquire(
            "qwen2.5-1.5b-instruct",
            "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
            "main",
            &[(
                "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
                "weights".to_string(),
                QWEN_SHA256.to_string(),
            )],
            chrono::Utc::now(),
        )
        .unwrap();
    assert_eq!(result["installed"], "qwen2.5-1.5b-instruct");
    assert_eq!(installer.installed_packages().unwrap(), vec!["qwen2.5-1.5b-instruct".to_string()]);
    // Brokered evidence.
    assert!(sink.entries().iter().any(|e| e.kind == harbor_net::NetworkEventKind::Completed));
}

#[test]
fn qualify_installed_production_model_chat() {
    // Requires the model to have been acquired into a store; the test
    // re-acquires via the streaming path when the env var points at a
    // prepared store, otherwise installs into its own temp store.
    let dir = tempfile::tempdir().unwrap();
    let installer = PackageInstaller::new(dir.path().join("models"));
    // Install from the already-downloaded fixture if present (offline
    // re-qualification); otherwise acquire through the broker.
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap()
            .join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if fixture.exists() {
        let bytes = std::fs::read(&fixture).unwrap();
        let manifest = PackageManifest {
            schema: "harbor.model/v3".into(),
            id: "qwen2.5-1.5b-instruct".into(),
            reference_type: "installed_package".into(),
            files: vec![PackageFile {
                role: "weights".into(),
                path: "qwen2.5-1.5b-instruct-q4_k_m.gguf".into(),
                sha256: harbor_canonical::sha256_hex(&bytes),
                size_bytes: bytes.len() as u64,
            }],
            runtime: RuntimeBinding {
                kind: "gguf/llama.cpp".into(),
                min_revision: "0.1.156".into(),
                targets: vec![std::env::consts::ARCH.to_string()],
            },
        };
        let mut staged = installer.begin("qwen2.5-1.5b-instruct").unwrap();
        installer.ingest_file(&mut staged, &manifest.files[0], &bytes).unwrap();
        installer.commit(&mut staged, &manifest, chrono::Utc::now()).unwrap();
    } else {
        let sink = std::sync::Arc::new(SqliteAuditSink::open_in_memory().unwrap());
        let broker = EgressBroker::new(Box::new(sink));
        let transport = UreqTransport::new();
        let sessions = sessions_for(&broker);
        let acquirer = HfAcquirer {
            broker: &broker,
            transport: &transport,
            installer: &installer,
            sessions,
            auth_token: None,
        progress: None,
        };
        // First acquisition: identity recorded from the downloaded bytes.
        acquirer
            .acquire(
                "qwen2.5-1.5b-instruct",
                "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
                "main",
                &[(
                    "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
                    "weights".to_string(),
                    String::new(),
                )],
                chrono::Utc::now(),
            )
            .unwrap();
    }

    let provider = GgufLlamaCppProvider::new(dir.path().join("models"))
        .unwrap()
        .with_context_tokens(2048);
    let m = ModelRef::InstalledPackage { package_id: "qwen2.5-1.5b-instruct".into() };
    provider.load(&m).unwrap();
    assert!(provider.supports(&m, &Capabilities::Chat));

    let req = ChatRequest {
        model: m,
        messages: vec![
            JsonValue::object([
                ("role", JsonValue::str("system")),
                ("content", JsonValue::str("You are a helpful assistant.")),
            ]),
            JsonValue::object([
                ("role", JsonValue::str("user")),
                ("content", JsonValue::str("What is the capital of France?")),
            ]),
        ],
        max_tokens: 32,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
    };
    let r1 = provider.generate(req.clone()).unwrap();
    let r2 = provider.generate(req).unwrap();
    // Greedy decoding is deterministic across runs.
    assert_eq!(r1.content, r2.content);
    assert!(!r1.content.trim().is_empty());
    println!("qwen answer: {:?}", r1.content);
    println!("usage: {} prompt / {} completion", r1.usage.prompt_tokens, r1.usage.completion_tokens);
    assert!(r1.usage.prompt_tokens > 0);
    assert_eq!(r1.executed_on, "qwen2.5-1.5b-instruct");
    assert_eq!(
        r1.execution_location,
        harbor_security::policy::ExecutionLocation::OnDevice
    );
}

#[test]
fn signed_catalog_entry_carries_the_pinned_model_hash() {
    // The production model enters the catalog through the SAME signed
    // path as everything else; signature binds the pinned hash.
    let key = CatalogSigningKey::from_secret_bytes(&[21u8; 32]);
    let entries = harbor_canonical::parse(&format!(
        r#"{{"packages":[{{"context_tokens":4096,"files":[{{"path":"qwen2.5-1.5b-instruct-q4_k_m.gguf","role":"weights","sha256":"{QWEN_SHA256}"}}],"id":"qwen2.5-1.5b-instruct","quantization":"Q4_K_M","repo_id":"Qwen/Qwen2.5-1.5B-Instruct-GGUF","revision":"main"}}]}}"#
    ))
    .unwrap();
    let signed = sign_catalog(&key, 1, "2026-09-12T00:00:00Z", entries).unwrap();
    let mut verifier = CatalogVerifier::new(&hex_encode(&key.public_bytes())).unwrap();
    verifier.verify(&signed).unwrap();
    let packages =
        harbor_modelhub::acquire::parse_catalog_document(&signed.entries).unwrap();
    assert_eq!(packages[0].id, "qwen2.5-1.5b-instruct");
    assert_eq!(packages[0].files[0].2, QWEN_SHA256);
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
