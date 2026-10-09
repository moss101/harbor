//! Catalog key ceremony (dev bootstrap): generate the release root key,
//! sign the production catalog containing the qualified model packages,
//! and write the public artifacts.
//!
//! The PRIVATE key is written to ~/.harbor-keys/ (0600, gitignored) and
//! must be moved to secure storage / an offline signing machine before any
//! public release. Only public artifacts are committed.

use harbor_canonical::JsonValue;
use harbor_modelhub::catalog_signing::{sign_catalog, CatalogSigningKey};

fn main() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

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
            let _ = std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600));
        }
        key
    };

    let public_hex = hex_encode(&key.public_bytes());

    // 2. The production catalog: the qualified model packages.
    let entries: JsonValue = harbor_canonical::parse(r#"{
        "packages": [
            {"context_tokens": 4096, "files": [{"path": "qwen2.5-1.5b-instruct-q4_k_m.gguf", "role": "weights", "sha256": "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e"}], "id": "qwen2.5-1.5b-instruct", "license": "Apache-2.0", "publisher": "harbor", "quantization": "Q4_K_M", "repo_id": "Qwen/Qwen2.5-1.5B-Instruct-GGUF", "revision": "main", "tiers": ["Balanced"]},
            {"context_tokens": 512, "files": [{"path": "bge-small-en-v1.5-q8_0.gguf", "role": "weights", "sha256": "f046db1dc724cf4f6f0a0c5917e922823b73eb1d27b8f9a9c2797f7866974804"}], "id": "bge-small-en-v1.5", "license": "MIT", "publisher": "harbor", "quantization": "Q8_0", "repo_id": "ggml-org/bge-small-en-v1.5-Q8_0-GGUF", "revision": "main", "tiers": ["Embeddings"]},
            {"context_tokens": 512, "files": [{"path": "tinyllamas/stories260K.gguf", "role": "weights", "sha256": "270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d"}], "id": "stories260k", "license": "Apache-2.0", "publisher": "harbor", "quantization": "Q8_0", "repo_id": "ggml-org/models", "revision": "main", "tiers": ["Test"]},
            {"context_tokens": 512, "files": [{"path": "multilingual-e5-small-q8_0.gguf", "role": "weights", "sha256": "e011debc1208e31bf7b6aebee2d9fc8bd2ca11694a77ed66ac9d0c9d0a877c93"}], "id": "multilingual-e5-small", "license": "MIT", "publisher": "harbor", "quantization": "Q8_0", "repo_id": "TwinSunsLLC/multilingual-e5-small-gguf", "revision": "main", "tiers": ["Embeddings"]},
            {"context_tokens": 8192, "files": [{"path": "bge-m3-q8_0.gguf", "role": "weights", "sha256": "950f4a8e5e19477a6d3c26d2f162233c20002c601f75e4b002e3239997821167"}], "id": "bge-m3", "license": "MIT", "publisher": "harbor", "quantization": "Q8_0", "repo_id": "gpustack/bge-m3-GGUF", "revision": "main", "tiers": ["Embeddings"]},
            {"context_tokens": 8192, "files": [{"path": "embeddinggemma-2-Q8_0.gguf", "role": "weights", "sha256": "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135"}], "id": "embeddinggemma-2", "license": "Apache-2.0", "publisher": "harbor", "quantization": "Q8_0", "repo_id": "ggml-org/embeddinggemma-2-GGUF", "revision": "bfcd298762cc34d0357ece5ebdd31791a3a374d8", "tiers": ["Embeddings"]},
            {"context_tokens": 8192, "files": [{"path": "embeddinggemma-2-Q8_0.gguf", "role": "weights", "sha256": "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135"}, {"path": "mmproj-embeddinggemma-2-Q8_0.gguf", "role": "mmproj", "sha256": "c4a8a52691ecef40618438928bdf9e68379b854e24166f292592353db0aab64f"}], "id": "embeddinggemma-2-multimodal", "license": "Apache-2.0", "publisher": "harbor", "quantization": "Q8_0", "repo_id": "ggml-org/embeddinggemma-2-GGUF", "revision": "bfcd298762cc34d0357ece5ebdd31791a3a374d8", "tiers": ["Embeddings"]}
        ]
    }"#).unwrap();

    // 3. Sign at epoch 5. History: epoch 4 added the publisher field (every
    // entry in the signed catalog is Harbor-curated — the Store's
    // first-party section is exactly the signed catalog); epoch 3 added
    // bge-m3; epoch 5 adds embeddinggemma-2 (text) and
    // embeddinggemma-2-multimodal (the same weights plus the image/audio
    // projector), decisions 0013 and 0015. Both run only on builds that
    // carry the vendored gemma-embedding2 runtime patch, and are pinned to
    // the exact upstream model revision. STAGED, NOT YET SIGNED: epochs are
    // monotonic and the signing key is the operator's.
    let signed = sign_catalog(&key, 5, "2026-10-09T00:00:00Z", entries).unwrap();

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
    std::fs::write(
        cat_dir.join("signed_catalog.json"),
        serde_json::to_string_pretty(&doc).unwrap(),
    )
    .unwrap();

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
