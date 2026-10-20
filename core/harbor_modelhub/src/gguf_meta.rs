//! Bounded, panic-free GGUF header reader used to classify an installed
//! package as an embedding model or a chat/generation model WITHOUT loading
//! it. Reads only the metadata key/value section (GGUF v2/v3), skipping
//! arrays and unwanted values by seeking, and never allocates from an
//! untrusted size without checking it against hard caps and the file length.
//!
//! Classification rule (derived from the real fixtures, see the test
//! `real_fixtures_classify`):
//! * `<arch>.pooling_type` present and != 0 (llama.cpp `NONE`) => embedder.
//!   Every embedding fixture (bge, e5, bge-m3, embeddinggemma) carries one;
//!   no generative fixture does.
//! * otherwise, a known encoder/embedding architecture => embedder.
//! * otherwise, a header that parsed and names an architecture => chat.
//! * header unreadable / no architecture => unknown (never guess chat).

use std::io::{Read, Seek};
use std::path::Path;

/// What a package's weights are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Embedder,
    Chat,
    Unknown,
}

impl ModelRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelRole::Embedder => "embedder",
            ModelRole::Chat => "chat",
            ModelRole::Unknown => "unknown",
        }
    }
}

/// The metadata the classifier needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GgufFacts {
    pub architecture: Option<String>,
    /// Value of `<arch>.pooling_type` when present and an integer.
    pub pooling_type: Option<u64>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GgufMetaError {
    #[error("not a GGUF file")]
    BadMagic,
    #[error("unsupported GGUF version {0}")]
    Version(u32),
    #[error("truncated or malformed GGUF header")]
    Malformed,
    #[error("GGUF header exceeds the safety limits")]
    TooLarge,
    #[error("io: {0}")]
    Io(String),
}

const MAX_KV: u64 = 65_536;
const MAX_KEY_LEN: u64 = 1024;
/// Longest string VALUE we will actually read (only `general.architecture`
/// is read; longer strings are skipped).
const MAX_READ_STRING: u64 = 256;
/// Arrays nest at most this deep (GGUF arrays of arrays are not used).
const MAX_ARRAY_NESTING: u32 = 1;

/// Architectures that are embedding/encoder models even when the file
/// carries no pooling key.
const EMBEDDER_ARCHS: &[&str] = &[
    "bert",
    "nomic-bert",
    "nomic-bert-moe",
    "jina-bert-v2",
    "jina-bert-v3",
    "modern-bert",
    "neo-bert",
    "eurobert",
    "gemma-embedding",
    "gemma-embedding2",
    "gemma3-embedding",
    "embeddinggemma",
    "t5encoder",
    "roberta",
    "xlm-roberta",
    "distilbert",
];

fn io_err(e: std::io::Error) -> GgufMetaError {
    if e.kind() == std::io::ErrorKind::UnexpectedEof {
        GgufMetaError::Malformed
    } else {
        GgufMetaError::Io(e.to_string())
    }
}

struct Rd<R: Read + Seek> {
    r: std::io::BufReader<R>,
    len: u64,
    pos: u64,
}

impl<R: Read + Seek> Rd<R> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], GgufMetaError> {
        if self.pos + N as u64 > self.len {
            return Err(GgufMetaError::Malformed);
        }
        let mut b = [0u8; N];
        self.r.read_exact(&mut b).map_err(io_err)?;
        self.pos += N as u64;
        Ok(b)
    }
    fn u32(&mut self) -> Result<u32, GgufMetaError> {
        Ok(u32::from_le_bytes(self.bytes::<4>()?))
    }
    fn u64(&mut self) -> Result<u64, GgufMetaError> {
        Ok(u64::from_le_bytes(self.bytes::<8>()?))
    }
    fn skip(&mut self, n: u64) -> Result<(), GgufMetaError> {
        let end = self.pos.checked_add(n).ok_or(GgufMetaError::Malformed)?;
        if end > self.len {
            return Err(GgufMetaError::Malformed);
        }
        // `seek_relative` keeps the read buffer when the target is inside it
        // (skipping a 100k-entry tokenizer array stays cheap).
        let delta = i64::try_from(n).map_err(|_| GgufMetaError::Malformed)?;
        self.r
            .seek_relative(delta)
            .map_err(|e| GgufMetaError::Io(e.to_string()))?;
        self.pos = end;
        Ok(())
    }
    fn vec(&mut self, n: u64) -> Result<Vec<u8>, GgufMetaError> {
        // Callers cap `n` first; re-check against the file for safety.
        if n > self.len - self.pos.min(self.len) {
            return Err(GgufMetaError::Malformed);
        }
        let mut v = vec![0u8; n as usize];
        self.r.read_exact(&mut v).map_err(io_err)?;
        self.pos += n;
        Ok(v)
    }
}

fn scalar_size(t: u32) -> Option<u64> {
    match t {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        10..=12 => Some(8),
        _ => None,
    }
}

/// Skip one value of type `t` (strings and arrays included).
fn skip_value<R: Read + Seek>(rd: &mut Rd<R>, t: u32, nesting: u32) -> Result<(), GgufMetaError> {
    if let Some(sz) = scalar_size(t) {
        return rd.skip(sz);
    }
    match t {
        8 => {
            let n = rd.u64()?;
            rd.skip(n)
        }
        9 => {
            if nesting >= MAX_ARRAY_NESTING {
                return Err(GgufMetaError::TooLarge);
            }
            let et = rd.u32()?;
            let n = rd.u64()?;
            if let Some(sz) = scalar_size(et) {
                rd.skip(n.checked_mul(sz).ok_or(GgufMetaError::Malformed)?)
            } else if et == 8 {
                // Each string needs >= 8 bytes of length prefix, so a count
                // larger than the remaining file is malformed up front.
                if n > (rd.len - rd.pos) / 8 {
                    return Err(GgufMetaError::Malformed);
                }
                for _ in 0..n {
                    let l = rd.u64()?;
                    rd.skip(l)?;
                }
                Ok(())
            } else {
                Err(GgufMetaError::Malformed)
            }
        }
        _ => Err(GgufMetaError::Malformed),
    }
}

fn read_uint<R: Read + Seek>(rd: &mut Rd<R>, t: u32) -> Result<Option<u64>, GgufMetaError> {
    Ok(match t {
        0 => Some(u64::from(rd.bytes::<1>()?[0])),
        2 => Some(u64::from(u16::from_le_bytes(rd.bytes::<2>()?))),
        4 => Some(u64::from(rd.u32()?)),
        10 => Some(rd.u64()?),
        1 | 3 | 5 | 11 => {
            // Signed: negative values are not valid pooling types.
            let v: i64 = match t {
                1 => i64::from(rd.bytes::<1>()?[0] as i8),
                3 => i64::from(i16::from_le_bytes(rd.bytes::<2>()?)),
                5 => i64::from(i32::from_le_bytes(rd.bytes::<4>()?)),
                _ => i64::from_le_bytes(rd.bytes::<8>()?),
            };
            u64::try_from(v).ok()
        }
        _ => {
            skip_value(rd, t, 0)?;
            None
        }
    })
}

/// Walk the KV section from an already-positioned-at-0 reader of `len` bytes.
pub fn read_facts<R: Read + Seek>(r: R, len: u64) -> Result<GgufFacts, GgufMetaError> {
    let mut rd = Rd {
        r: std::io::BufReader::with_capacity(64 * 1024, r),
        len,
        pos: 0,
    };
    if &rd.bytes::<4>()? != b"GGUF" {
        return Err(GgufMetaError::BadMagic);
    }
    let version = rd.u32()?;
    if version != 2 && version != 3 {
        return Err(GgufMetaError::Version(version));
    }
    let _tensor_count = rd.u64()?;
    let kv_count = rd.u64()?;
    if kv_count > MAX_KV {
        return Err(GgufMetaError::TooLarge);
    }
    let mut facts = GgufFacts::default();
    let mut raw_pooling: Option<(String, Option<u64>)> = None;
    for _ in 0..kv_count {
        let klen = rd.u64()?;
        if klen > MAX_KEY_LEN {
            return Err(GgufMetaError::TooLarge);
        }
        let key = String::from_utf8_lossy(&rd.vec(klen)?).into_owned();
        let t = rd.u32()?;
        if key == "general.architecture" && t == 8 {
            let n = rd.u64()?;
            if n <= MAX_READ_STRING {
                facts.architecture = Some(String::from_utf8_lossy(&rd.vec(n)?).into_owned());
            } else {
                rd.skip(n)?;
            }
        } else if key.ends_with(".pooling_type") && key.matches('.').count() == 1 {
            raw_pooling = Some((key, read_uint(&mut rd, t)?));
        } else {
            skip_value(&mut rd, t, 0)?;
        }
        // Everything we need is in hand: stop without walking the (huge)
        // tokenizer arrays.
        if let (Some(a), Some((k, v))) = (&facts.architecture, &raw_pooling) {
            if k == &format!("{a}.pooling_type") {
                facts.pooling_type = *v;
                return Ok(facts);
            }
        }
    }
    if let (Some(a), Some((k, v))) = (&facts.architecture, raw_pooling) {
        if k == format!("{a}.pooling_type") {
            facts.pooling_type = v;
        }
    }
    Ok(facts)
}

/// Pure classification of parsed facts.
pub fn classify(f: &GgufFacts) -> ModelRole {
    if let Some(p) = f.pooling_type {
        if p != 0 {
            return ModelRole::Embedder;
        }
    }
    match f.architecture.as_deref() {
        None | Some("") => ModelRole::Unknown,
        Some(a) if EMBEDDER_ARCHS.contains(&a) => ModelRole::Embedder,
        Some(_) => ModelRole::Chat,
    }
}

/// Classify a GGUF file on disk; any failure is `Unknown`.
pub fn classify_file(path: &Path) -> ModelRole {
    let go = || -> Result<GgufFacts, GgufMetaError> {
        let f = std::fs::File::open(path).map_err(|e| GgufMetaError::Io(e.to_string()))?;
        let len = f
            .metadata()
            .map_err(|e| GgufMetaError::Io(e.to_string()))?
            .len();
        read_facts(f, len)
    };
    match go() {
        Ok(f) => classify(&f),
        Err(_) => ModelRole::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn s(out: &mut Vec<u8>, v: &str) {
        out.extend((v.len() as u64).to_le_bytes());
        out.extend(v.as_bytes());
    }

    /// Synthetic GGUF v3: arch string, optional pooling u32, plus a string
    /// array and a u32 array in between.
    fn synth(arch: &str, pooling: Option<u32>) -> Vec<u8> {
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        let n = 3 + u64::from(pooling.is_some());
        b.extend(n.to_le_bytes());
        s(&mut b, "general.architecture");
        b.extend(8u32.to_le_bytes());
        s(&mut b, arch);
        s(&mut b, "tokenizer.ggml.tokens");
        b.extend(9u32.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend(2u64.to_le_bytes());
        s(&mut b, "a");
        s(&mut b, "bc");
        s(&mut b, "x.scores");
        b.extend(9u32.to_le_bytes());
        b.extend(6u32.to_le_bytes());
        b.extend(2u64.to_le_bytes());
        b.extend([0u8; 8]);
        if let Some(p) = pooling {
            s(&mut b, &format!("{arch}.pooling_type"));
            b.extend(4u32.to_le_bytes());
            b.extend(p.to_le_bytes());
        }
        b
    }

    fn role(b: &[u8]) -> ModelRole {
        match read_facts(Cursor::new(b.to_vec()), b.len() as u64) {
            Ok(f) => classify(&f),
            Err(_) => ModelRole::Unknown,
        }
    }

    #[test]
    fn synthetic_classes() {
        assert_eq!(role(&synth("llama", None)), ModelRole::Chat);
        assert_eq!(role(&synth("llama", Some(0))), ModelRole::Chat);
        assert_eq!(role(&synth("qwen2", Some(1))), ModelRole::Embedder);
        assert_eq!(role(&synth("bert", Some(2))), ModelRole::Embedder);
        // Known encoder arch with no pooling key.
        assert_eq!(role(&synth("nomic-bert", None)), ModelRole::Embedder);
    }

    #[test]
    fn hostile_inputs_are_errors_never_panics() {
        let good = synth("llama", Some(1));
        // Every truncation is Err or a (shorter) valid parse, never a panic.
        for cut in 0..good.len() {
            let _ = read_facts(Cursor::new(good[..cut].to_vec()), cut as u64);
        }
        assert_eq!(role(b""), ModelRole::Unknown);
        assert_eq!(role(b"GGU"), ModelRole::Unknown);
        assert_eq!(role(b"NOTGGUFATALL........"), ModelRole::Unknown);
        // Wrong version.
        let mut v = good.clone();
        v[4..8].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            read_facts(Cursor::new(v.clone()), v.len() as u64),
            Err(GgufMetaError::Version(1))
        );
        // Oversized kv_count.
        let mut v = good.clone();
        v[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(
            read_facts(Cursor::new(v.clone()), v.len() as u64),
            Err(GgufMetaError::TooLarge)
        );
        // kv_count larger than the data: runs out, Malformed.
        let mut v = good.clone();
        v[16..24].copy_from_slice(&1000u64.to_le_bytes());
        v.truncate(v.len() - 12);
        assert!(read_facts(Cursor::new(v.clone()), v.len() as u64).is_err());
        // Huge key length / string length / array count.
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend(1u64.to_le_bytes());
        b.extend(u64::MAX.to_le_bytes());
        assert_eq!(role(&b), ModelRole::Unknown);
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend(1u64.to_le_bytes());
        s(&mut b, "k");
        b.extend(9u32.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend(u64::MAX.to_le_bytes());
        assert_eq!(role(&b), ModelRole::Unknown);
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend(1u64.to_le_bytes());
        s(&mut b, "general.architecture");
        b.extend(8u32.to_le_bytes());
        b.extend(u64::MAX.to_le_bytes());
        assert_eq!(role(&b), ModelRole::Unknown);
        // Unknown value type, nested array.
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend(1u64.to_le_bytes());
        s(&mut b, "k");
        b.extend(99u32.to_le_bytes());
        assert_eq!(role(&b), ModelRole::Unknown);
        // Header with no architecture is unknown, not chat.
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        assert_eq!(role(&b), ModelRole::Unknown);
    }

    #[test]
    fn missing_file_is_unknown() {
        assert_eq!(
            classify_file(Path::new("/nonexistent/x.gguf")),
            ModelRole::Unknown
        );
    }

    #[test]
    fn real_fixtures_classify() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/models");
        let want = [
            ("bge-small-en-v1.5-q8_0.gguf", ModelRole::Embedder),
            ("bge-m3-q8_0.gguf", ModelRole::Embedder),
            ("embeddinggemma-2-Q8_0.gguf", ModelRole::Embedder),
            ("multilingual-e5-small-q8_0.gguf", ModelRole::Embedder),
            ("Llama-3.2-1B-Instruct-Q4_K_M.gguf", ModelRole::Chat),
            ("Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf", ModelRole::Chat),
            ("SmolVLM2-500M-Video-Instruct-Q8_0.gguf", ModelRole::Chat),
            ("qwen2.5-1.5b-instruct-q4_k_m.gguf", ModelRole::Chat),
            ("stories260K.gguf", ModelRole::Chat),
        ];
        for (name, expected) in want {
            let p = dir.join(name);
            if !p.exists() {
                eprintln!("skip absent fixture {name}");
                continue;
            }
            let t = std::time::Instant::now();
            assert_eq!(classify_file(&p), expected, "{name}");
            assert!(t.elapsed().as_millis() < 500, "{name} header read slow");
        }
    }
}
