//! Harbor durable storage: SQLite database with versioned migrations,
//! encrypted per-workspace blob store, OS-secure key-source abstraction and
//! the decrypted working window registry.
//!
//! Authority: `03_Architecture_Contracts.md` §10, `13_Network_and_Storage_Policy.md`.
//!
//! - Private workspace data (artifacts, extracts, embeddings, previews,
//!   run evidence, caches) is encrypted per workspace with ChaCha20-Poly1305.
//! - A device root key in the OS secure store wraps per-workspace keys.
//! - Per-blob data keys support rotation by rewrapping.
//! - Cross-workspace private-blob deduplication is disabled by default.
//! - Crypto-erasure of a workspace deletes its keys; unreachable encrypted
//!   blobs are garbage-collected by reference counting.

pub mod blob;
pub mod db;
pub mod error;
pub mod kcipher;
pub mod keys;
pub mod native_keystore;
pub mod settings;
pub mod temp;

pub use blob::{BlobRef, BlobStore, PutOptions};
pub use db::{Database, Migration};
pub use error::StoreError;
pub use kcipher::{knowledge_chunk_key, open_text, open_vector, seal_text, seal_vector};
pub use keys::{FileKeyStore, KeyMaterial, KeyStore, WrappedKey};
#[cfg(windows)]
pub use native_keystore::DpapiKeyStore;
pub use native_keystore::{InjectedKeyStore, KeychainKeyStore};
pub use settings::SettingsStore;
pub use temp::{TempHandle, TempRegistry};
