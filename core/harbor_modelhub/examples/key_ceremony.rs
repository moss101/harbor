//! Catalog key ceremony (dev bootstrap): generate the release root key,
//! sign the production catalog containing the qualified model packages,
//! and write the public artifacts.
//!
//! The PRIVATE key is written to ~/.harbor-keys/ (0600, gitignored) and
//! must be moved to secure storage / an offline signing machine before any
//! public release. Only public artifacts are committed.

use harbor_modelhub::catalog_signing::{sign_catalog, CatalogSigningKey};
use harbor_canonical::JsonValue;

fn main() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap().to_path_buf();

    // 1. Load or create the root key outside the repository.
    let key_dir = std::env::home_dir()
        .or_else(|| std::env::var("HOME").map(std::path::PathBuf::from).ok())
        .unwrap()
        .join(".harbor-keys");
    std::fs::create_dir_all(&key_dir).unwrap();
    let secret_path = key_dir.join("catalog-root.key");
    let key = if secret_path.exists() {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&std::fs::read(&secret_path).unwrap());
        CatalogSigningKey::from_secret_bytes(&bytes)
    } else {
        let key = CatalogSigningKey::generate();
        // Persist the SECRET seed (not the public key) with 0600 perms.
        std::fs::write(&secret_path, key.secret_bytes()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &secret_path,
                std::fs::Permissions::from_mode(0o600),
            );
        }
        key
    };

    let public_hex = hex_encode(&key.public_bytes());

    // 2. The production catalog: the qualified model packages.
    let entries: JsonValue = harbor_canonical::parse(&format!(r#"{{
        "packages": [
            {{"context_tokens": 4096, "files": [{{"path": "qwen2.5-1.5b-instruct-q4_k_m.gguf", "role": "weights", "sha256": "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e"}}], "id": "qwen2.5-1.5b-instruct", "license": "Apache-2.0", "quantization": "Q4_K_M", "repo_id": "Qwen/Qwen2.5-1.5B-Instruct-GGUF", "revision": "main", "tiers": ["Balanced"]}},
            {{"context_tokens": 512, "files": [{{"path": "bge-small-en-v1.5-q8_0.gguf", "role": "weights", "sha256": "f046db1dc724cf4f6f0a0c5917e922823b73eb1d27b8f9a9c2797f7866974804"}}], "id": "bge-small-en-v1.5", "license": "MIT", "quantization": "Q8_0", "repo_id": "ggml-org/bge-small-en-v1.5-Q8_0-GGUF", "revision": "main", "tiers": ["Embeddings"]}},
            {{"context_tokens": 512, "files": [{{"path": "tinyllamas/stories260K.gguf", "role": "weights", "sha256": "270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d"}}], "id": "stories260k", "license": "Apache-2.0", "quantization": "Q8_0", "repo_id": "ggml-org/models", "revision": "main", "tiers": ["Test"]}}
        ]
    }}"#)).unwrap();

    // 3. Sign at epoch 1.
    let signed = sign_catalog(&key, 1, "2026-09-12T00:00:00Z", entries).unwrap();

    // 4. Public artifacts.
    let cat_dir = repo_root.join("fixtures/catalog");
    std::fs::create_dir_all(&cat_dir).unwrap();
    std::fs::write(cat_dir.join("root_public.hex"), format!("{public_hex}\n")).unwrap();
    let doc = serde_json::json!({
        "schema": "harbor.catalog/v1",
        "epoch": signed.epoch,
        "published_at": signed.published_at,
        "key_id": signed.key_id,
        "signature": signed.signature,
        "entries": signed.entries,
    });
    std::fs::write(cat_dir.join("signed_catalog.json"),
        serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    // 5. Verify immediately: the committed artifacts must validate.
    let mut verifier = harbor_modelhub::catalog_signing::CatalogVerifier::new(&public_hex).unwrap();
    verifier.verify(&signed).unwrap();
    println!("key_id: {}", key.key_id);
    println!("epoch: {}", signed.epoch);
    println!("verified: ok");
    println!("artifacts: fixtures/catalog/{{root_public.hex,signed_catalog.json}}");
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
