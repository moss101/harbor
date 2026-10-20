//! A multi-byte character split across tokens must survive decoding.
//!
//! Byte-level BPE vocabularies (Qwen, Llama) emit some characters as
//! several tokens that each hold part of the UTF-8 bytes (Arabic-Indic
//! digits such as ٤٥ are a common case). Decoding every token with a
//! FRESH decoder turned each fragment into U+FFFD, so an Arabic answer
//! quoting "٤٥" came back as "��" (found by the Arabic-over-Arabic-file
//! probe in apps/harbor_app/test/work_live_test.dart). The decoder is
//! stateful on purpose; it must live for the whole generation.
//!
//! Skips (passes) when the Qwen fixture is absent, like the other live
//! tests.

use harbor_canonical::JsonValue;
use harbor_inference::gguf::GgufLlamaCppProvider;
use harbor_inference::provider::{Capabilities, ChatRequest, ModelProvider, ModelRef};
use harbor_modelhub::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

fn install(dir: &std::path::Path, gguf: &std::path::Path) -> String {
    let bytes = std::fs::read(gguf).unwrap();
    let sha = harbor_canonical::sha256_hex(&bytes);
    let id = "qwen-utf8".to_string();
    let installer = PackageInstaller::new(dir);
    let manifest = PackageManifest {
        schema: "harbor.model/v3".into(),
        id: id.clone(),
        reference_type: "installed_package".into(),
        files: vec![PackageFile {
            role: "weights".into(),
            path: gguf.file_name().unwrap().to_string_lossy().to_string(),
            sha256: sha,
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
    assert!(installer.validate(&staged, &manifest).unwrap().ok);
    installer
        .commit(&mut staged, &manifest, chrono::Utc::now())
        .unwrap();
    id
}

#[test]
fn arabic_indic_digits_are_not_replaced_by_u_fffd() {
    let gguf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !gguf.exists() {
        eprintln!("SKIP: qwen2.5-1.5b fixture absent");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let id = install(dir.path(), &gguf);
    let provider = GgufLlamaCppProvider::new(dir.path()).unwrap();
    let model = ModelRef::InstalledPackage { package_id: id };
    provider.load(&model).unwrap();
    let sentence = "شروط الدفع: يتم سداد كل فاتورة خلال ٤٥ يوماً من تاريخ استلامها.";
    let resp = provider
        .generate_cancellable(
            ChatRequest {
                model: model.clone(),
                messages: vec![JsonValue::object([
                    ("role", JsonValue::str("user")),
                    (
                        "content",
                        JsonValue::str(format!(
                            "Copy this sentence exactly, character for character, \
                             and write nothing else:\n{sentence}"
                        )),
                    ),
                ])],
                max_tokens: 64,
                temperature: 0.0,
                requires: vec![Capabilities::Chat],
                response_schema: None,
                trace_key: None,
            },
            &std::sync::atomic::AtomicBool::new(false),
            None,
        )
        .unwrap();
    println!("UTF8_PROBE {}", resp.content);
    assert!(
        !resp.content.contains('\u{FFFD}'),
        "a split multi-byte character was replaced: {:?}",
        resp.content
    );
    assert!(
        resp.content.contains('٤'),
        "the copy should keep the Arabic-Indic digit: {:?}",
        resp.content
    );
}
