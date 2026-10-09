//! Multimodal embeddings for EmbeddingGemma 2 (decision 0015).
//!
//! The model embeds text, images and audio into ONE vector space by
//! running them through the same bidirectional transformer in a single
//! pass, mean-pooled. llama.cpp's stock multimodal helper decodes text and
//! media in separate batches, which for a non-causal embedding model means
//! the pieces never attend to each other — so this module builds the
//! joint input itself:
//!
//! - text tokens become their (sqrt(n_embd)-scaled) input embeddings,
//!   read straight from the model's `token_embd.weight`;
//! - image / audio chunks are encoded by the mmproj projector (`mtmd`) and
//!   used as-is (raw embeddings are not rescaled);
//! - all rows go into one embedding batch, one forward pass.
//!
//! Everything is reached through the same GGUF provider and installed
//! package as text embedding; the optional `mmproj` file only adds the
//! media towers. Text-only input through here must equal the stock text
//! path (asserted by the live test), which is what validates the row
//! construction independent of any image.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::mtmd::{
    mtmd_default_marker, MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputChunkType,
    MtmdInputText,
};

use crate::gguf::GgufLlamaCppProvider;
use crate::provider::ProviderError;

/// Upper bound on frames per video item (24 x ~280 tokens = ~6.7K of the
/// shared 8,192-token context, leaving room for any text).
pub const MAX_VIDEO_FRAMES: usize = 24;

/// One piece of an interleaved input, in order.
#[derive(Debug, Clone)]
pub enum MediaPart {
    Text(String),
    /// An encoded image (PNG / JPEG / …).
    Image(Vec<u8>),
    /// A video as its sampled frames, in order (each an encoded image).
    /// The model treats video as sampled frames through the vision
    /// encoder; the caller samples (about 1 frame per second, evenly
    /// spaced) because the core carries no video decoder. At most
    /// [`MAX_VIDEO_FRAMES`]: frames share the 8,192-token context at ~280
    /// tokens each.
    Video(Vec<Vec<u8>>),
    /// An encoded audio file (WAV / MP3 / FLAC); the runtime decodes and
    /// resamples it.
    AudioFile(Vec<u8>),
    /// Mono PCM f32 at the projector's sample rate (16 kHz for
    /// EmbeddingGemma 2).
    Audio(Vec<f32>),
}

fn backend_err(what: &str, e: impl std::fmt::Display) -> ProviderError {
    ProviderError::Backend(format!("{what}: {e}"))
}

// ---------------------------------------------------------------------
// token_embd.weight reader (F32 / F16 / BF16 / Q8_0 rows)
// ---------------------------------------------------------------------

struct TokenEmbeddings {
    path: std::path::PathBuf,
    data_offset: u64,
    n_embd: usize,
    n_vocab: usize,
    ggml_type: u32,
    cache: Mutex<std::collections::HashMap<i32, Vec<f32>>>,
}

fn rd<const N: usize>(f: &mut std::fs::File) -> std::io::Result<[u8; N]> {
    let mut b = [0u8; N];
    f.read_exact(&mut b)?;
    Ok(b)
}
fn rd_u32(f: &mut std::fs::File) -> std::io::Result<u32> {
    Ok(u32::from_le_bytes(rd::<4>(f)?))
}
fn rd_u64(f: &mut std::fs::File) -> std::io::Result<u64> {
    Ok(u64::from_le_bytes(rd::<8>(f)?))
}
fn rd_str(f: &mut std::fs::File) -> std::io::Result<String> {
    let n = rd_u64(f)? as usize;
    if n > 1 << 20 {
        return Err(std::io::Error::other("gguf string too long"));
    }
    let mut b = vec![0u8; n];
    f.read_exact(&mut b)?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}

/// Skip one GGUF metadata value of type `t`; returns the u32 value when
/// it is an unsigned scalar (for `general.alignment`).
fn skip_value(f: &mut std::fs::File, t: u32) -> std::io::Result<Option<u64>> {
    let fixed = |t: u32| match t {
        0 | 1 | 7 => Some(1u64),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        10..=12 => Some(8),
        _ => None,
    };
    match t {
        8 => {
            rd_str(f)?;
            Ok(None)
        }
        9 => {
            let et = rd_u32(f)?;
            let n = rd_u64(f)?;
            if et == 8 {
                for _ in 0..n {
                    rd_str(f)?;
                }
            } else if let Some(sz) = fixed(et) {
                f.seek(SeekFrom::Current((n * sz) as i64))?;
            } else {
                return Err(std::io::Error::other("unsupported gguf array element"));
            }
            Ok(None)
        }
        _ => {
            let sz = fixed(t).ok_or_else(|| std::io::Error::other("unsupported gguf value"))?;
            let mut b = [0u8; 8];
            f.read_exact(&mut b[..sz as usize])?;
            Ok(Some(u64::from_le_bytes(b)))
        }
    }
}

impl TokenEmbeddings {
    fn open(path: &Path) -> Result<Self, ProviderError> {
        let io = |e: std::io::Error| backend_err("read token embeddings", e);
        let mut f = std::fs::File::open(path).map_err(io)?;
        if &rd::<4>(&mut f).map_err(io)? != b"GGUF" {
            return Err(backend_err("read token embeddings", "not a GGUF file"));
        }
        let _version = rd_u32(&mut f).map_err(io)?;
        let n_tensors = rd_u64(&mut f).map_err(io)?;
        let n_kv = rd_u64(&mut f).map_err(io)?;
        let mut alignment = 32u64;
        for _ in 0..n_kv {
            let key = rd_str(&mut f).map_err(io)?;
            let t = rd_u32(&mut f).map_err(io)?;
            let v = skip_value(&mut f, t).map_err(io)?;
            if key == "general.alignment" {
                if let Some(a) = v {
                    alignment = a.max(1);
                }
            }
        }
        let mut found: Option<(usize, usize, u32, u64)> = None;
        for _ in 0..n_tensors {
            let name = rd_str(&mut f).map_err(io)?;
            let n_dims = rd_u32(&mut f).map_err(io)? as usize;
            let mut dims = Vec::with_capacity(n_dims);
            for _ in 0..n_dims {
                dims.push(rd_u64(&mut f).map_err(io)? as usize);
            }
            let ggml_type = rd_u32(&mut f).map_err(io)?;
            let offset = rd_u64(&mut f).map_err(io)?;
            if name == "token_embd.weight" && dims.len() == 2 {
                found = Some((dims[0], dims[1], ggml_type, offset));
            }
        }
        let header_end = f.stream_position().map_err(io)?;
        let data_start = header_end.div_ceil(alignment) * alignment;
        let (n_embd, n_vocab, ggml_type, offset) =
            found.ok_or_else(|| backend_err("read token embeddings", "no token_embd.weight"))?;
        if !matches!(ggml_type, 0 | 1 | 30 | 8) {
            return Err(backend_err(
                "read token embeddings",
                format!("unsupported tensor type {ggml_type} (need F32/F16/BF16/Q8_0)"),
            ));
        }
        Ok(TokenEmbeddings {
            path: path.to_path_buf(),
            data_offset: data_start + offset,
            n_embd,
            n_vocab,
            ggml_type,
            cache: Mutex::new(Default::default()),
        })
    }

    fn row_bytes(&self) -> usize {
        match self.ggml_type {
            0 => self.n_embd * 4,
            1 | 30 => self.n_embd * 2,
            _ => self.n_embd / 32 * 34, // Q8_0: f16 scale + 32 x i8 per block
        }
    }

    /// The embedding row for `token`, dequantized to f32 (unscaled).
    fn row(&self, token: i32) -> Result<Vec<f32>, ProviderError> {
        if let Some(r) = self.cache.lock().unwrap().get(&token) {
            return Ok(r.clone());
        }
        if token < 0 || token as usize >= self.n_vocab {
            return Err(backend_err(
                "token row",
                format!("token {token} out of vocab"),
            ));
        }
        let io = |e: std::io::Error| backend_err("read token row", e);
        let mut f = std::fs::File::open(&self.path).map_err(io)?;
        let rb = self.row_bytes();
        f.seek(SeekFrom::Start(self.data_offset + token as u64 * rb as u64))
            .map_err(io)?;
        let mut raw = vec![0u8; rb];
        f.read_exact(&mut raw).map_err(io)?;
        let row: Vec<f32> = match self.ggml_type {
            0 => raw
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect(),
            1 => raw
                .chunks_exact(2)
                .map(|b| f16_to_f32(u16::from_le_bytes([b[0], b[1]])))
                .collect(),
            30 => raw
                .chunks_exact(2)
                .map(|b| f32::from_bits((u16::from_le_bytes([b[0], b[1]]) as u32) << 16))
                .collect(),
            _ => raw
                .chunks_exact(34)
                .flat_map(|blk| {
                    let d = f16_to_f32(u16::from_le_bytes([blk[0], blk[1]]));
                    blk[2..].iter().map(move |q| d * (*q as i8) as f32)
                })
                .collect(),
        };
        self.cache.lock().unwrap().insert(token, row.clone());
        Ok(row)
    }
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = match (exp, frac) {
        (0, 0) => sign << 31,
        (0, f) => {
            // subnormal: normalize
            let mut e = 127 - 15 + 1;
            let mut m = f;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
        (0x1f, 0) => (sign << 31) | 0x7f80_0000,
        (0x1f, f) => (sign << 31) | 0x7f80_0000 | (f << 13),
        (e, f) => (sign << 31) | ((e + 127 - 15) << 23) | (f << 13),
    };
    f32::from_bits(bits)
}

// ---------------------------------------------------------------------
// The embedder
// ---------------------------------------------------------------------

/// Joint text / image / audio embedder over an installed package that has
/// an `mmproj` file. It owns its model handle and projector: DROP IT to
/// free the media towers (the owning service does so on memory release).
/// One embed at a time.
pub struct MediaEmbedder {
    model: std::sync::Arc<LlamaModel>,
    mtmd: MtmdContext,
    tokens: TokenEmbeddings,
    n_embd: usize,
    n_ctx: u32,
    backend: &'static llama_cpp_2::llama_backend::LlamaBackend,
    guard: Mutex<()>,
}

impl MediaEmbedder {
    /// Open the media towers for an installed, LOADED package. Errors
    /// (typed) when the package ships no `mmproj` file.
    pub fn open(provider: &GgufLlamaCppProvider, package_id: &str) -> Result<Self, ProviderError> {
        let model = provider
            .loaded_model(package_id)
            .ok_or_else(|| ProviderError::ModelNotFound(format!("{package_id} not loaded")))?;
        let mmproj = provider.mmproj_file(package_id)?;
        let weights = provider.weights_file(package_id)?;
        let tokens = TokenEmbeddings::open(&weights)?;
        let n_embd = model.n_embd() as usize;
        if tokens.n_embd != n_embd {
            return Err(backend_err(
                "multimodal",
                format!(
                    "token_embd width {} != model n_embd {n_embd}",
                    tokens.n_embd
                ),
            ));
        }
        // Honor the CPU pin: a package whose GPU path failed the numerics
        // canary must not run its projector on the GPU either.
        let params = MtmdContextParams {
            use_gpu: !provider.is_cpu_pinned(package_id)
                && std::env::var("HARBOR_GGUF_CPU_ONLY").as_deref() != Ok("1"),
            n_threads: crate::gguf::compute_threads(),
            ..MtmdContextParams::default()
        };
        let mtmd = MtmdContext::init_from_file(
            mmproj
                .to_str()
                .ok_or_else(|| backend_err("multimodal", "non-utf8 mmproj path"))?,
            &model,
            &params,
        )
        .map_err(|e| backend_err("mmproj init", e))?;
        let n_ctx = model.n_ctx_train().clamp(512, 8192);
        Ok(MediaEmbedder {
            model,
            mtmd,
            tokens,
            n_embd,
            n_ctx,
            backend: provider.backend(),
            guard: Mutex::new(()),
        })
    }

    pub fn supports_vision(&self) -> bool {
        self.mtmd.support_vision()
    }

    pub fn supports_audio(&self) -> bool {
        self.mtmd.support_audio()
    }

    /// Sample rate audio must be supplied at, if the projector has an
    /// audio tower.
    pub fn audio_sample_rate(&self) -> Option<u32> {
        self.mtmd.get_audio_sample_rate()
    }

    /// Embed an interleaved input as ONE vector (mean pooled, not
    /// normalized — callers normalize / truncate like text vectors).
    pub fn embed(&self, parts: &[MediaPart]) -> Result<Vec<f32>, ProviderError> {
        let _g = self.guard.lock().unwrap();
        if parts.is_empty() {
            return Err(backend_err("multimodal", "empty input"));
        }
        let marker = mtmd_default_marker();
        let mut prompt = String::new();
        let mut bitmaps: Vec<MtmdBitmap> = Vec::new();
        for part in parts {
            match part {
                MediaPart::Text(t) => prompt.push_str(t),
                MediaPart::Image(bytes) => {
                    if !self.supports_vision() {
                        return Err(backend_err("multimodal", "package has no vision tower"));
                    }
                    let bm = MtmdBitmap::from_buffer(&self.mtmd, bytes, false)
                        .map_err(|e| backend_err("image decode", e))?;
                    bitmaps.push(bm);
                    prompt.push_str(marker);
                }
                MediaPart::Video(frames) => {
                    if !self.supports_vision() {
                        return Err(backend_err("multimodal", "package has no vision tower"));
                    }
                    if frames.is_empty() || frames.len() > MAX_VIDEO_FRAMES {
                        return Err(backend_err(
                            "multimodal",
                            format!(
                                "a video needs 1..={MAX_VIDEO_FRAMES} frames, got {}",
                                frames.len()
                            ),
                        ));
                    }
                    for frame in frames {
                        let bm = MtmdBitmap::from_buffer(&self.mtmd, frame, false)
                            .map_err(|e| backend_err("video frame decode", e))?;
                        bitmaps.push(bm);
                        prompt.push_str(marker);
                    }
                }
                MediaPart::AudioFile(bytes) => {
                    if !self.supports_audio() {
                        return Err(backend_err("multimodal", "package has no audio tower"));
                    }
                    let bm = MtmdBitmap::from_buffer(&self.mtmd, bytes, false)
                        .map_err(|e| backend_err("audio decode", e))?;
                    bitmaps.push(bm);
                    prompt.push_str(marker);
                }
                MediaPart::Audio(pcm) => {
                    if !self.supports_audio() {
                        return Err(backend_err("multimodal", "package has no audio tower"));
                    }
                    let bm = MtmdBitmap::from_audio_data(pcm)
                        .map_err(|e| backend_err("audio decode", e))?;
                    bitmaps.push(bm);
                    prompt.push_str(marker);
                }
            }
        }
        let refs: Vec<&MtmdBitmap> = bitmaps.iter().collect();
        let chunks = self
            .mtmd
            .tokenize(
                MtmdInputText {
                    text: prompt,
                    add_special: true,
                    parse_special: true,
                },
                &refs,
            )
            .map_err(|e| backend_err("mtmd tokenize", e))?;

        let scale = (self.n_embd as f32).sqrt();
        let mut rows: Vec<f32> = Vec::new();
        for i in 0..chunks.len() {
            let chunk = chunks
                .get(i)
                .ok_or_else(|| backend_err("multimodal", "chunk index"))?;
            match chunk.chunk_type() {
                MtmdInputChunkType::Text => {
                    for t in chunk.text_tokens().unwrap_or(&[]) {
                        // Token path in the graph: tok_embd[id] * sqrt(n_embd).
                        rows.extend(self.tokens.row(t.0)?.iter().map(|x| x * scale));
                    }
                }
                MtmdInputChunkType::Image | MtmdInputChunkType::Audio => {
                    self.mtmd
                        .encode_chunk(&chunk)
                        .map_err(|e| backend_err("media encode", e))?;
                    let n = chunk.n_tokens() * self.n_embd;
                    // SAFETY: n floats were just produced by encode_chunk;
                    // copied out before the next encode.
                    rows.extend_from_slice(unsafe { self.mtmd.output_embeddings(n) });
                }
            }
        }
        let n_tokens = rows.len() / self.n_embd;
        if n_tokens == 0 || !rows.len().is_multiple_of(self.n_embd) {
            return Err(backend_err("multimodal", "malformed embedding rows"));
        }
        if n_tokens > self.n_ctx as usize {
            return Err(backend_err(
                "multimodal",
                format!(
                    "input of {n_tokens} positions exceeds the shared {}-token context",
                    self.n_ctx
                ),
            ));
        }
        if rows.iter().any(|x| !x.is_finite()) {
            return Err(backend_err("multimodal", "non-finite input embeddings"));
        }

        // Size the context to the input, not to the model's 8,192 ceiling:
        // the compute buffer scales with the batch, and an 8K context is
        // ~1.2 GB — fine on a desktop, fatal on a phone. Rounded to 256 so
        // contexts are reusable-shaped, never above the shared limit.
        let need = (n_tokens as u32)
            .next_multiple_of(256)
            .max(512)
            .min(self.n_ctx);
        let n_ctx = std::num::NonZeroU32::new(need).unwrap();
        let params = LlamaContextParams::default()
            .with_n_ctx(Some(n_ctx))
            .with_n_batch(n_ctx.get())
            .with_n_ubatch(n_ctx.get())
            .with_embeddings(true)
            .with_pooling_type(llama_cpp_2::context::params::LlamaPoolingType::Mean)
            .with_n_threads(crate::gguf::compute_threads())
            .with_n_threads_batch(crate::gguf::compute_threads());
        let mut ctx = self
            .model
            .new_context(self.backend, params)
            .map_err(|e| backend_err("context", e))?;
        let mut batch = LlamaBatch::new_embd(n_tokens, self.n_embd, 1);
        for (pos, row) in rows.chunks_exact(self.n_embd).enumerate() {
            batch
                .add_embd(row, self.n_embd, pos as i32, 0, pos == n_tokens - 1)
                .map_err(|e| backend_err("batch", e))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| backend_err("decode", e))?;
        let emb = ctx
            .embeddings_seq_ith(0)
            .map_err(|e| backend_err("embeddings", e))?;
        if emb.iter().any(|x| !x.is_finite()) {
            return Err(backend_err("multimodal", "non-finite output embedding"));
        }
        Ok(emb.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_conversion_matches_known_values() {
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xc000), -2.0);
        assert_eq!(f16_to_f32(0x0000), 0.0);
        assert!((f16_to_f32(0x3555) - 0.333_251_95).abs() < 1e-6);
        assert!(f16_to_f32(0x7c00).is_infinite());
        // smallest subnormal
        assert!((f16_to_f32(0x0001) - 5.960_464_5e-8).abs() < 1e-12);
    }
}
