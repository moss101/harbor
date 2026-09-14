//! Knowledge chunk encryption: chunk text and embedding vectors are
//! private workspace content (policy 13), so the knowledge database
//! stores only AEAD-sealed payloads — the same guarantee the run event
//! log already provides for step descriptions.
//!
//! Sealed value format: `nonce(12) || ChaCha20-Poly1305 ciphertext`,
//! with AAD binding the chunk identity (`source_id`, `chunk_id`, field)
//! so rows cannot be moved between sources or fields undetected.

use crate::error::{Result, StoreError};
use crate::keys::{aead_open, aead_seal, KeyMaterial};

/// Domain separator for the knowledge chunk key derived from the
/// workspace key (never stored; re-derived on every open).
pub const KNOWLEDGE_CHUNK_KEY_DOMAIN: &str = "harbor.knowledge.chunk/v1";

/// Derive the knowledge chunk key from workspace key material.
pub fn knowledge_chunk_key(workspace_kek: &KeyMaterial) -> KeyMaterial {
    KeyMaterial::derive_subkey(workspace_kek, KNOWLEDGE_CHUNK_KEY_DOMAIN)
}

/// Seal chunk text under [key], binding it to its chunk identity.
pub fn seal_text(
    key: &KeyMaterial,
    source_id: &str,
    chunk_id: &str,
    text: &str,
) -> Result<Vec<u8>> {
    let aad = chunk_aad(source_id, chunk_id, "text");
    seal_value(key, &aad, text.as_bytes())
}

/// Open sealed chunk text; fails on any tampering or key mismatch.
pub fn open_text(
    key: &KeyMaterial,
    source_id: &str,
    chunk_id: &str,
    sealed: &[u8],
) -> Result<String> {
    let aad = chunk_aad(source_id, chunk_id, "text");
    let pt = open_value(key, &aad, sealed)?;
    String::from_utf8(pt).map_err(|_| StoreError::Crypto)
}

/// Seal an embedding vector (little-endian f32) under [key].
pub fn seal_vector(
    key: &KeyMaterial,
    source_id: &str,
    chunk_id: &str,
    vector: &[f32],
) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for v in vector {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let aad = chunk_aad(source_id, chunk_id, "vector");
    seal_value(key, &aad, &bytes)
}

/// Open a sealed embedding vector.
pub fn open_vector(
    key: &KeyMaterial,
    source_id: &str,
    chunk_id: &str,
    sealed: &[u8],
) -> Result<Vec<f32>> {
    let aad = chunk_aad(source_id, chunk_id, "vector");
    let pt = open_value(key, &aad, sealed)?;
    let (chunks, remainder) = pt.as_chunks::<4>();
    if !remainder.is_empty() {
        return Err(StoreError::Crypto);
    }
    Ok(chunks.iter().map(|b| f32::from_le_bytes(*b)).collect())
}

fn chunk_aad(source_id: &str, chunk_id: &str, field: &str) -> Vec<u8> {
    format!("harbor.knowledge/1\x1f{source_id}\x1f{chunk_id}\x1f{field}").into_bytes()
}

fn seal_value(key: &KeyMaterial, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut nonce = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
    let ct = aead_seal(key, &nonce, plaintext, aad)?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn open_value(key: &KeyMaterial, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < 12 + 16 {
        return Err(StoreError::Crypto);
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&sealed[..12]);
    aead_open(key, &nonce, &sealed[12..], aad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roundtrip_and_tamper_rejection() {
        let key = KeyMaterial::random();
        let sealed = seal_text(&key, "src-1", "src-1-0", "the contract value is 5000").unwrap();
        assert_eq!(
            open_text(&key, "src-1", "src-1-0", &sealed).unwrap(),
            "the contract value is 5000"
        );
        // AAD binding: reading under a different chunk identity fails.
        assert!(open_text(&key, "src-2", "src-1-0", &sealed).is_err());
        // Bit-flip in the ciphertext fails authentication.
        let mut tampered = sealed.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(open_text(&key, "src-1", "src-1-0", &tampered).is_err());
        // Truncated input fails structurally.
        assert!(open_text(&key, "src-1", "src-1-0", &sealed[..16]).is_err());
    }

    #[test]
    fn vector_roundtrip_preserves_precision() {
        let key = KeyMaterial::random();
        let v: Vec<f32> = vec![0.25, -1.5, 3.125, 0.0, 1e-7];
        let sealed = seal_vector(&key, "s", "s-0", &v).unwrap();
        assert_eq!(open_vector(&key, "s", "s-0", &sealed).unwrap(), v);
        // Distinct nonces: sealing twice yields different bytes.
        let again = seal_vector(&key, "s", "s-0", &v).unwrap();
        assert_ne!(sealed, again);
    }

    #[test]
    fn derived_key_is_deterministic_and_field_separated() {
        let wk = KeyMaterial::random();
        let k1 = knowledge_chunk_key(&wk);
        let k2 = knowledge_chunk_key(&wk);
        assert_eq!(k1, k2);
        let sealed = seal_text(&k1, "s", "s-0", "x").unwrap();
        // A different workspace key cannot open the row.
        let other = knowledge_chunk_key(&KeyMaterial::random());
        assert!(open_text(&other, "s", "s-0", &sealed).is_err());
    }
}
