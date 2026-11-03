//! Wire format between the supervisor and the inference worker (decision
//! 0022 phase 3).
//!
//! One frame = `u32 LE json_len | json | u32 LE blob_count | (u64 LE len |
//! bytes)*`. The JSON carries the request or reply; blobs carry bulk bytes
//! (images, audio, vectors) without base64. Every length is bounded before
//! anything is allocated, so a corrupt stream is an error, never an abort.
//!
//! Requests: `{"id": n, "op": "...", ...}`. Replies: `{"id": n, "ok": v}`,
//! `{"id": n, "err": {"kind": k, "message": m}}`, or the interim
//! `{"id": n, "progress": tokens}` while a generation runs.

use std::io::{self, Read, Write};

use crate::provider::{Capabilities, ProviderError};

pub const PROTOCOL_VERSION: u64 = 1;
const MAX_JSON: u32 = 64 << 20;
const MAX_BLOBS: u32 = 4096;
const MAX_BLOB: u64 = 1 << 30;

pub struct Frame {
    pub json: serde_json::Value,
    pub blobs: Vec<Vec<u8>>,
}

pub fn write_frame(
    w: &mut impl Write,
    json: &serde_json::Value,
    blobs: &[&[u8]],
) -> io::Result<()> {
    let body = serde_json::to_vec(json).map_err(io::Error::other)?;
    let mut buf =
        Vec::with_capacity(body.len() + 8 + blobs.iter().map(|b| b.len() + 8).sum::<usize>());
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(&body);
    buf.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
    for b in blobs {
        buf.extend_from_slice(&(b.len() as u64).to_le_bytes());
        buf.extend_from_slice(b);
    }
    w.write_all(&buf)?;
    w.flush()
}

fn read_exact_or_eof(r: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

fn bad(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_string())
}

/// Read one frame; `Ok(None)` on a clean end of stream between frames.
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut len4 = [0u8; 4];
    if !read_exact_or_eof(r, &mut len4)? {
        return Ok(None);
    }
    let len = u32::from_le_bytes(len4);
    if len > MAX_JSON {
        return Err(bad("frame json too large"));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    let json: serde_json::Value = serde_json::from_slice(&body).map_err(|_| bad("frame json"))?;
    r.read_exact(&mut len4)?;
    let count = u32::from_le_bytes(len4);
    if count > MAX_BLOBS {
        return Err(bad("too many blobs"));
    }
    let mut blobs = Vec::with_capacity(count.min(64) as usize);
    for _ in 0..count {
        let mut len8 = [0u8; 8];
        r.read_exact(&mut len8)?;
        let n = u64::from_le_bytes(len8);
        if n > MAX_BLOB {
            return Err(bad("blob too large"));
        }
        let mut b = Vec::new();
        r.by_ref().take(n).read_to_end(&mut b)?;
        if b.len() as u64 != n {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        blobs.push(b);
    }
    Ok(Some(Frame { json, blobs }))
}

/// f32 vector <-> little-endian bytes (vectors and PCM travel as blobs).
pub fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn bytes_to_f32s(b: &[u8]) -> Result<Vec<f32>, ProviderError> {
    if !b.len().is_multiple_of(4) {
        return Err(ProviderError::Backend("worker: ragged f32 blob".into()));
    }
    Ok(b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect())
}

pub fn capability_from_str(s: &str) -> Option<Capabilities> {
    Some(match s {
        "chat" => Capabilities::Chat,
        "tools" => Capabilities::Tools,
        "structured_output" => Capabilities::StructuredOutput,
        "embeddings" => Capabilities::Embeddings,
        "vision" => Capabilities::Vision,
        "audio" => Capabilities::Audio,
        _ => return None,
    })
}

/// A provider error as it crosses the wire.
pub fn error_to_json(e: &ProviderError) -> serde_json::Value {
    let (kind, message) = match e {
        ProviderError::UnsupportedCapability(c) => ("unsupported", c.to_string()),
        ProviderError::ModelNotFound(m) => ("model_not_found", m.clone()),
        ProviderError::Cancelled => ("cancelled", String::new()),
        ProviderError::Backend(m) => ("backend", m.clone()),
        ProviderError::Policy(m) => ("policy", m.clone()),
        ProviderError::RuntimeCrashed(m) => ("runtime_crashed", m.clone()),
    };
    serde_json::json!({ "kind": kind, "message": message })
}

pub fn error_from_json(v: &serde_json::Value) -> ProviderError {
    let message = v["message"].as_str().unwrap_or("").to_string();
    match v["kind"].as_str().unwrap_or("") {
        "unsupported" => ProviderError::UnsupportedCapability(
            capability_from_str(&message).map_or("unknown", |c| c.as_str()),
        ),
        "model_not_found" => ProviderError::ModelNotFound(message),
        "cancelled" => ProviderError::Cancelled,
        "policy" => ProviderError::Policy(message),
        "runtime_crashed" => ProviderError::RuntimeCrashed(message),
        _ => ProviderError::Backend(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_with_blobs() {
        let mut buf = Vec::new();
        let j = serde_json::json!({"id": 7, "op": "embed", "texts": ["a", "ü"]});
        write_frame(&mut buf, &j, &[b"abc", &[]]).unwrap();
        write_frame(&mut buf, &serde_json::json!({"id": 8}), &[]).unwrap();
        let mut r = &buf[..];
        let f = read_frame(&mut r).unwrap().unwrap();
        assert_eq!(f.json, j);
        assert_eq!(f.blobs, vec![b"abc".to_vec(), vec![]]);
        assert_eq!(read_frame(&mut r).unwrap().unwrap().json["id"], 8);
        assert!(read_frame(&mut r).unwrap().is_none());
    }

    #[test]
    fn truncated_and_oversized_frames_are_errors() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &serde_json::json!({"id": 1}), &[b"xyz"]).unwrap();
        for cut in 1..buf.len() {
            assert!(read_frame(&mut &buf[..cut]).is_err(), "cut at {cut}");
        }
        let huge = (MAX_JSON + 1).to_le_bytes();
        assert!(read_frame(&mut &huge[..]).is_err());
        let mut lying = Vec::new();
        lying.extend_from_slice(&2u32.to_le_bytes());
        lying.extend_from_slice(b"{}");
        lying.extend_from_slice(&1u32.to_le_bytes());
        lying.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(read_frame(&mut &lying[..]).is_err());
    }

    #[test]
    fn errors_and_vectors_round_trip() {
        for e in [
            ProviderError::UnsupportedCapability("vision"),
            ProviderError::ModelNotFound("m".into()),
            ProviderError::Cancelled,
            ProviderError::Backend("b".into()),
            ProviderError::Policy("p".into()),
            ProviderError::RuntimeCrashed("r".into()),
        ] {
            let back = error_from_json(&error_to_json(&e));
            assert_eq!(back.to_string(), e.to_string());
        }
        let v = vec![1.5f32, -0.0, f32::MIN_POSITIVE];
        assert_eq!(bytes_to_f32s(&f32s_to_bytes(&v)).unwrap(), v);
        assert!(bytes_to_f32s(&[1, 2, 3]).is_err());
    }
}
