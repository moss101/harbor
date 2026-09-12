//! One-shot production-model acquisition through the brokered streaming
//! path. Writes the model into fixtures/models and prints its recorded
//! package identity (sha256 + size) for the qualification binding.

use harbor_modelhub::acquire::{HfAcquirer, HF_CDN_ORIGINS};
use harbor_modelhub::install::PackageInstaller;
use harbor_net::audit::SqliteAuditSink;
use harbor_net::broker::{EgressBroker, EgressClass};
use harbor_net::transport::UreqTransport;
use harbor_security::policy::PrivacyMode;
use std::collections::BTreeMap;

fn main() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let out_fixture = repo_root.join("fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");

    let dir = tempfile::tempdir().unwrap();
    let sink = SqliteAuditSink::open_in_memory().unwrap();
    let broker = EgressBroker::new(Box::new(sink));
    let transport = UreqTransport::new();
    let installer = PackageInstaller::new(dir.path().join("models"));
    let mut sessions = BTreeMap::new();
    for origin in ["https://huggingface.co", HF_CDN_ORIGINS[0], HF_CDN_ORIGINS[1], HF_CDN_ORIGINS[2], HF_CDN_ORIGINS[3]] {
        let s = broker
            .open_session(EgressClass::WeightTransfer, origin, chrono::Duration::minutes(60), PrivacyMode::LocalOnly)
            .unwrap();
        sessions.insert(origin.to_string(), s);
    }
    let acquirer = HfAcquirer {
        broker: &broker,
        transport: &transport,
        installer: &installer,
        sessions,
        auth_token: None,
    };
    let result = acquirer
        .acquire(
            "qwen2.5-1.5b-instruct",
            "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
            "main",
            &[(
                "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
                "weights".to_string(),
                String::new(), // first acquisition: identity recorded from bytes
            )],
            chrono::Utc::now(),
        )
        .unwrap();
    println!("installed: {}", result["installed"].as_str().unwrap());
    let installed_path = dir.path().join("models/qwen2.5-1.5b-instruct/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    let bytes_len = std::fs::metadata(&installed_path).unwrap().len();
    println!("size: {bytes_len}");
    std::fs::copy(&installed_path, &out_fixture).unwrap();
    let sha = harbor_canonical::sha256_hex(&std::fs::read(&out_fixture).unwrap());
    println!("sha256: {sha}");
    println!("fixture: {}", out_fixture.display());
}
