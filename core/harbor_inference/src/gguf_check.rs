//! Pre-load GGUF validation (decision 0022, phase 2).
//!
//! A pure-Rust parser that walks the GGUF header, the metadata key/value
//! table and the tensor-info table *before* llama.cpp ever sees the file, so
//! a malformed or hostile model yields a typed [`GgufCheckError`] instead of
//! a `GGML_ASSERT` abort inside the runtime.
//!
//! Design rules:
//!
//! * The parser streams. It never reads the tensor data section, never
//!   buffers whole metadata values it does not need, and never allocates in
//!   proportion to an attacker-controlled count: lengths are capped and
//!   checked against the bytes actually remaining before anything is read.
//! * It mirrors `gguf_init_from_reader` in the vendored
//!   `ggml/src/gguf.cpp` and the file-bounds check in
//!   `src/llama-model-loader.h`, and is stricter where that is cheap (zero
//!   dimensions, bounded counts, an architecture allow-list, sane
//!   hyper-parameters).
//! * The ggml type table and the architecture allow-list are static Rust
//!   tables. Tests re-derive both from the vendored llama.cpp sources, so a
//!   runtime bump that adds or changes a type or an architecture fails the
//!   build's tests instead of silently drifting.
//!
//! No llama.cpp dependency: this module compiles without `gguf-backend`.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

/// `GGUF_VERSION` in the vendored `ggml/include/gguf.h`. Version 1 is
/// rejected by gguf.cpp; version 2 shares version 3's little-endian layout.
pub const GGUF_MAX_VERSION: u32 = 3;
/// `GGUF_DEFAULT_ALIGNMENT`.
pub const GGUF_DEFAULT_ALIGNMENT: u64 = 32;
/// `GGML_MAX_NAME`: tensor names must be strictly shorter (NUL terminator).
pub const GGML_MAX_NAME: usize = 64;
/// `GGML_MAX_DIMS`.
pub const GGML_MAX_DIMS: u32 = 4;

/// Upper bound on the number of tensors. The largest published MoE GGUFs
/// carry a few thousand; this is generous while keeping the duplicate-name
/// set bounded.
pub const MAX_TENSOR_COUNT: u64 = 1 << 16;
/// Upper bound on the number of metadata keys (real files: tens).
pub const MAX_KV_COUNT: u64 = 1 << 16;
/// Upper bound on a key length in bytes.
pub const MAX_KEY_LEN: u64 = 1 << 12;
/// Upper bound on a string value (chat templates are the largest, ~100 KiB).
/// gguf.cpp allows 1 GiB; we never buffer these, but cap anyway.
pub const MAX_STRING_LEN: u64 = 64 << 20;
/// `GGUF_MAX_ARRAY_ELEMENTS` in gguf.cpp.
pub const MAX_ARRAY_LEN: u64 = 1 << 30;
/// Upper bound on header + metadata + tensor table, i.e. the data-section
/// offset. Real tokenizer-heavy files are ~10-20 MiB.
pub const MAX_HEADER_BYTES: u64 = 256 << 20;

/// Sanity caps on hyper-parameters (inclusive).
pub const MAX_CONTEXT_LENGTH: u64 = 1 << 24;
pub const MAX_EMBEDDING_LENGTH: u64 = 1 << 20;
/// `LLAMA_MAX_LAYERS` (src/llama-hparams.h): llama-model.cpp GGML_ASSERTs
/// `0 < n_layer <= LLAMA_MAX_LAYERS`, so exceeding it aborts the process.
pub const MAX_BLOCK_COUNT: u64 = 512;
/// `LLAMA_MAX_EXPERTS` (src/llama-hparams.h), also GGML_ASSERTed.
pub const MAX_EXPERT_COUNT: u64 = 512;
pub const MAX_HEAD_COUNT: u64 = 1 << 14;
pub const MAX_HEAD_DIM: u64 = 1 << 16;

/// GGUF metadata value type ids (`enum gguf_type`).
const GGUF_TYPE_UINT8: u32 = 0;
const GGUF_TYPE_INT8: u32 = 1;
const GGUF_TYPE_UINT16: u32 = 2;
const GGUF_TYPE_INT16: u32 = 3;
const GGUF_TYPE_UINT32: u32 = 4;
const GGUF_TYPE_INT32: u32 = 5;
const GGUF_TYPE_FLOAT32: u32 = 6;
const GGUF_TYPE_BOOL: u32 = 7;
const GGUF_TYPE_STRING: u32 = 8;
const GGUF_TYPE_ARRAY: u32 = 9;
const GGUF_TYPE_UINT64: u32 = 10;
const GGUF_TYPE_INT64: u32 = 11;
const GGUF_TYPE_FLOAT64: u32 = 12;

/// One ggml tensor type: `(name, block size in elements, block size in bytes)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GgmlType {
    pub name: &'static str,
    pub blck_size: u64,
    pub type_size: u64,
}

const fn t(name: &'static str, blck_size: u64, type_size: u64) -> Option<GgmlType> {
    Some(GgmlType {
        name,
        blck_size,
        type_size,
    })
}

/// `GGML_TYPE_COUNT` in the vendored `ggml/include/ggml.h`.
pub const GGML_TYPE_COUNT: usize = 43;

/// The vendored ggml `type_traits` table (`ggml/src/ggml.c`), indexed by
/// type id. `None` marks ids that were removed (their traits have
/// `blck_size == 0`, which gguf.cpp rejects). Block byte sizes are the
/// `static_assert(sizeof(block_*) == ...)` expressions in
/// `ggml/src/ggml-common.h`. Verified by `type_table_matches_vendored_ggml`.
pub const GGML_TYPES: [Option<GgmlType>; GGML_TYPE_COUNT] = [
    t("f32", 1, 4),        // 0
    t("f16", 1, 2),        // 1
    t("q4_0", 32, 18),     // 2
    t("q4_1", 32, 20),     // 3
    None,                  // 4  Q4_2 removed
    None,                  // 5  Q4_3 removed
    t("q5_0", 32, 22),     // 6
    t("q5_1", 32, 24),     // 7
    t("q8_0", 32, 34),     // 8
    t("q8_1", 32, 36),     // 9
    t("q2_K", 256, 84),    // 10
    t("q3_K", 256, 110),   // 11
    t("q4_K", 256, 144),   // 12
    t("q5_K", 256, 176),   // 13
    t("q6_K", 256, 210),   // 14
    t("q8_K", 256, 292),   // 15
    t("iq2_xxs", 256, 66), // 16
    t("iq2_xs", 256, 74),  // 17
    t("iq3_xxs", 256, 98), // 18
    t("iq1_s", 256, 50),   // 19
    t("iq4_nl", 32, 18),   // 20
    t("iq3_s", 256, 110),  // 21
    t("iq2_s", 256, 82),   // 22
    t("iq4_xs", 256, 136), // 23
    t("i8", 1, 1),         // 24
    t("i16", 1, 2),        // 25
    t("i32", 1, 4),        // 26
    t("i64", 1, 8),        // 27
    t("f64", 1, 8),        // 28
    t("iq1_m", 256, 56),   // 29
    t("bf16", 1, 2),       // 30
    None,                  // 31 Q4_0_4_4 removed
    None,                  // 32 Q4_0_4_8 removed
    None,                  // 33 Q4_0_8_8 removed
    t("tq1_0", 256, 54),   // 34
    t("tq2_0", 256, 66),   // 35
    None,                  // 36 IQ4_NL_4_4 removed
    None,                  // 37 IQ4_NL_4_8 removed
    None,                  // 38 IQ4_NL_8_8 removed
    t("mxfp4", 32, 17),    // 39
    t("nvfp4", 64, 36),    // 40
    t("q1_0", 128, 18),    // 41
    t("q2_0", 64, 18),     // 42
];

/// `general.architecture` values this runtime revision can load: the
/// `LLM_ARCH_NAMES` table in the vendored `src/llama-arch.cpp` (minus
/// `(unknown)`), in source order. It includes `clip`, the architecture
/// mmproj projector files carry, and Harbor's `gemma-embedding2` back-port.
/// Verified by `arch_list_matches_vendored_llama_arch`.
pub const KNOWN_ARCHITECTURES: &[&str] = &[
    "clip",
    "llama",
    "llama4",
    "deci",
    "falcon",
    "grok",
    "gpt2",
    "gptj",
    "gptneox",
    "mpt",
    "baichuan",
    "starcoder",
    "refact",
    "bert",
    "modern-bert",
    "nomic-bert",
    "nomic-bert-moe",
    "neo-bert",
    "jina-bert-v2",
    "jina-bert-v3",
    "eurobert",
    "bloom",
    "stablelm",
    "qwen",
    "qwen2",
    "qwen2moe",
    "qwen2vl",
    "qwen3",
    "qwen3moe",
    "qwen3next",
    "qwen3vl",
    "qwen3vlmoe",
    "qwen35",
    "qwen35moe",
    "phi2",
    "phi3",
    "phimoe",
    "plamo",
    "plamo2",
    "plamo3",
    "codeshell",
    "orion",
    "internlm2",
    "minicpm",
    "minicpm3",
    "gemma",
    "gemma2",
    "gemma3",
    "gemma3n",
    "gemma4",
    "gemma4-assistant",
    "gemma-embedding",
    "gemma-embedding2",
    "starcoder2",
    "mamba",
    "mamba2",
    "jamba",
    "falcon-h1",
    "xverse",
    "command-r",
    "cohere2",
    "cohere2moe",
    "dbrx",
    "olmo",
    "olmo2",
    "olmoe",
    "muse-glimmer",
    "openelm",
    "arctic",
    "deepseek",
    "deepseek2",
    "deepseek2-ocr",
    "deepseek32",
    "deepseek4",
    "chatglm",
    "glm4",
    "glm4moe",
    "glm-dsa",
    "bitnet",
    "t5",
    "t5encoder",
    "jais",
    "jais2",
    "nemotron",
    "nemotron_h",
    "nemotron_h_moe",
    "exaone",
    "exaone4",
    "exaone-moe",
    "rwkv6",
    "rwkv6qwen2",
    "rwkv7",
    "arwkv7",
    "granite",
    "granitemoe",
    "granitehybrid",
    "graniteswitch",
    "chameleon",
    "wavtokenizer-dec",
    "plm",
    "bailingmoe",
    "bailingmoe2",
    "dots1",
    "arcee",
    "afmoe",
    "laguna",
    "ernie4_5",
    "ernie4_5-moe",
    "hunyuan-moe",
    "hunyuan-dense",
    "hunyuan_vl",
    "hy_v3",
    "smollm3",
    "gpt-oss",
    "lfm2",
    "lfm2moe",
    "dream",
    "smallthinker",
    "llada",
    "llada-moe",
    "seed_oss",
    "grovemoe",
    "apertus",
    "minimax-m2",
    "minimax-m3",
    "cogvlm",
    "rnd1",
    "pangu-embedded",
    "mistral3",
    "eagle3",
    "dflash",
    "mistral4",
    "paddleocr",
    "mimo2",
    "step35",
    "llama-embed",
    "maincoder",
    "kimi-linear",
    "talkie",
    "mellum",
    "nanbeige",
    "qwen3tts",
    "pockettts",
];

/// What the check learned about a file that passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufSummary {
    pub version: u32,
    pub architecture: String,
    pub tensor_count: u64,
    pub kv_count: u64,
    pub alignment: u64,
    /// Absolute file offset of the (aligned) tensor data section.
    pub data_offset: u64,
    pub file_len: u64,
    /// Sum of `ggml_nbytes` over all tensors (unpadded).
    pub tensor_bytes: u64,
    pub context_length: Option<u64>,
    pub embedding_length: Option<u64>,
    pub block_count: Option<u64>,
    /// Max over layers when the key is a per-layer array.
    pub head_count: Option<u64>,
    /// Max over layers when the key is a per-layer array.
    pub head_count_kv: Option<u64>,
    pub key_length: Option<u64>,
    pub value_length: Option<u64>,
    /// `<arch>.expert_count` (MoE), when present.
    pub expert_count: Option<u64>,
}

/// Why a file was refused. Every variant names the failing field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GgufCheckError {
    #[error("gguf: cannot read file: {0}")]
    Io(String),
    #[error("gguf: file truncated while reading {field} at offset {offset}")]
    Truncated { field: String, offset: u64 },
    #[error("gguf: bad magic {0:02x?}, expected \"GGUF\"")]
    BadMagic([u8; 4]),
    #[error("gguf: version {0} looks byte-swapped (big-endian files are not supported)")]
    EndianMismatch(u32),
    #[error("gguf: unsupported version {0} (supported: 2..={GGUF_MAX_VERSION})")]
    UnsupportedVersion(u32),
    #[error("gguf: {field} is {count}, above the cap of {max}")]
    CountTooLarge {
        field: &'static str,
        count: u64,
        max: u64,
    },
    #[error("gguf: string {field} has length {len}, above the cap of {max}")]
    StringTooLong { field: String, len: u64, max: u64 },
    #[error("gguf: {field} is not valid UTF-8")]
    InvalidUtf8 { field: String },
    #[error("gguf: metadata key #{index} is empty")]
    EmptyKey { index: u64 },
    #[error("gguf: duplicate metadata key '{0}'")]
    DuplicateKey(String),
    #[error("gguf: key '{key}' has unknown value type {type_id}")]
    UnknownValueType { key: String, type_id: u32 },
    #[error("gguf: key '{key}' is a nested array")]
    NestedArray { key: String },
    #[error("gguf: key '{key}' array has {len} elements, above the cap of {max}")]
    ArrayTooLong { key: String, len: u64, max: u64 },
    #[error("gguf: key '{key}' must be {expected}, found value type {found}")]
    WrongValueType {
        key: String,
        expected: &'static str,
        found: u32,
    },
    #[error("gguf: general.alignment {0} is not a power of two")]
    BadAlignment(u64),
    #[error(
        "gguf: tensor #{index} name length {len} is not below GGML_MAX_NAME ({GGML_MAX_NAME})"
    )]
    TensorNameTooLong { index: u64, len: u64 },
    #[error("gguf: duplicate tensor name '{0}'")]
    DuplicateTensor(String),
    #[error("gguf: tensor '{tensor}' has {n_dims} dimensions (allowed 1..={GGML_MAX_DIMS})")]
    BadDimCount { tensor: String, n_dims: u32 },
    #[error("gguf: tensor '{tensor}' dimension {dim} is {value} (must be >= 1)")]
    BadDim {
        tensor: String,
        dim: u32,
        value: i64,
    },
    #[error("gguf: tensor '{tensor}' element count overflows")]
    ElementOverflow { tensor: String },
    #[error("gguf: tensor '{tensor}' has unknown ggml type {type_id}")]
    UnknownTensorType { tensor: String, type_id: u32 },
    #[error("gguf: tensor '{tensor}' uses removed ggml type {type_id}")]
    RemovedTensorType { tensor: String, type_id: u32 },
    #[error(
        "gguf: tensor '{tensor}' row of {ne0} elements is not a multiple of block size {block}"
    )]
    RowNotBlockMultiple {
        tensor: String,
        ne0: u64,
        block: u64,
    },
    #[error("gguf: tensor '{tensor}' byte size overflows")]
    ByteSizeOverflow { tensor: String },
    #[error("gguf: tensor '{tensor}' offset {offset} is not aligned to {alignment}")]
    MisalignedOffset {
        tensor: String,
        offset: u64,
        alignment: u64,
    },
    #[error("gguf: tensor '{tensor}' offset {found}, expected {expected} (tensors must be contiguous and non-overlapping)")]
    UnexpectedOffset {
        tensor: String,
        expected: u64,
        found: u64,
    },
    #[error("gguf: tensor '{tensor}' data ends at {end}, past the end of the file ({file_len})")]
    TensorOutOfBounds {
        tensor: String,
        end: u64,
        file_len: u64,
    },
    #[error("gguf: header is {len} bytes, above the cap of {max}")]
    HeaderTooLarge { len: u64, max: u64 },
    #[error("gguf: general.architecture is missing")]
    MissingArchitecture,
    #[error("gguf: architecture '{0}' is not supported by this runtime")]
    UnknownArchitecture(String),
    #[error("gguf: {key} = {value} is outside {min}..={max}")]
    OutOfRange {
        key: String,
        value: i128,
        min: u64,
        max: u64,
    },
    /// Hyper-parameters that are each in range but violate an invariant
    /// llama.cpp `GGML_ASSERT`s in `llama_model::load_hparams`.
    #[error("gguf: {key} is inconsistent: {detail}")]
    Inconsistent { key: String, detail: String },
}

type Result<T> = std::result::Result<T, GgufCheckError>;

/// Validate a GGUF file on disk. Reads only the header, metadata and tensor
/// table (streamed, never the tensor data).
pub fn check_file(path: &Path) -> Result<GgufSummary> {
    let file = File::open(path).map_err(|e| GgufCheckError::Io(e.to_string()))?;
    let file_len = file
        .metadata()
        .map_err(|e| GgufCheckError::Io(e.to_string()))?
        .len();
    let reader = BufReader::with_capacity(64 * 1024, file);
    parse(reader, file_len, file_len)
}

/// Validate a GGUF from its leading bytes. `header` must contain at least
/// the header, metadata and tensor table; `file_len` is the length of the
/// whole file (used for the tensor-data bounds check). This is the fuzz
/// entry point: any input returns `Ok` or `Err`, never panics.
pub fn check_bytes(header: &[u8], file_len: u64) -> Result<GgufSummary> {
    let readable = (header.len() as u64).min(file_len);
    parse(header, readable, file_len)
}

/// Look up a ggml type id.
pub fn ggml_type(type_id: u32) -> Option<GgmlType> {
    GGML_TYPES.get(type_id as usize).copied().flatten()
}

/// Whether `general.architecture` is on this runtime's allow-list.
pub fn is_known_architecture(arch: &str) -> bool {
    KNOWN_ARCHITECTURES.contains(&arch)
}

// ---------------------------------------------------------------------------

struct Reader<R: Read> {
    inner: R,
    pos: u64,
    /// Bytes readable from the start (`pos` never exceeds it).
    limit: u64,
}

impl<R: Read> Reader<R> {
    fn remaining(&self) -> u64 {
        self.limit - self.pos
    }

    fn truncated(&self, field: impl Into<String>) -> GgufCheckError {
        GgufCheckError::Truncated {
            field: field.into(),
            offset: self.pos,
        }
    }

    fn need(&self, n: u64, field: &str) -> Result<()> {
        if n > self.remaining() {
            Err(self.truncated(field))
        } else {
            Ok(())
        }
    }

    fn fill(&mut self, buf: &mut [u8], field: &str) -> Result<()> {
        self.need(buf.len() as u64, field)?;
        match self.inner.read_exact(buf) {
            Ok(()) => {
                self.pos += buf.len() as u64;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Err(self.truncated(field)),
            Err(e) => Err(GgufCheckError::Io(e.to_string())),
        }
    }

    fn array<const N: usize>(&mut self, field: &str) -> Result<[u8; N]> {
        let mut b = [0u8; N];
        self.fill(&mut b, field)?;
        Ok(b)
    }

    fn u32(&mut self, field: &str) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array(field)?))
    }

    fn u64(&mut self, field: &str) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array(field)?))
    }

    fn i64(&mut self, field: &str) -> Result<i64> {
        Ok(i64::from_le_bytes(self.array(field)?))
    }

    fn skip(&mut self, n: u64, field: &str) -> Result<()> {
        self.need(n, field)?;
        let copied = io::copy(&mut (&mut self.inner).take(n), &mut io::sink())
            .map_err(|e| GgufCheckError::Io(e.to_string()))?;
        self.pos += copied;
        if copied != n {
            return Err(self.truncated(field));
        }
        Ok(())
    }

    /// Read a length-prefixed string of at most `max` bytes into memory.
    fn string(&mut self, field: &str, max: u64) -> Result<Vec<u8>> {
        let len = self.u64(field)?;
        if len > max {
            return Err(GgufCheckError::StringTooLong {
                field: field.into(),
                len,
                max,
            });
        }
        self.need(len, field)?;
        let mut buf = vec![0u8; len as usize];
        self.fill(&mut buf, field)?;
        Ok(buf)
    }

    fn skip_string(&mut self, field: &str) -> Result<()> {
        let len = self.u64(field)?;
        if len > MAX_STRING_LEN {
            return Err(GgufCheckError::StringTooLong {
                field: field.into(),
                len,
                max: MAX_STRING_LEN,
            });
        }
        self.skip(len, field)
    }

    /// Read one integer scalar of GGUF type `ty`, or `None` for non-integers
    /// (the value is still consumed).
    fn scalar(&mut self, ty: u32, field: &str) -> Result<Option<i128>> {
        Ok(match ty {
            GGUF_TYPE_UINT8 => Some(self.array::<1>(field)?[0] as i128),
            GGUF_TYPE_INT8 => Some(self.array::<1>(field)?[0] as i8 as i128),
            GGUF_TYPE_UINT16 => Some(u16::from_le_bytes(self.array(field)?) as i128),
            GGUF_TYPE_INT16 => Some(i16::from_le_bytes(self.array(field)?) as i128),
            GGUF_TYPE_UINT32 => Some(self.u32(field)? as i128),
            GGUF_TYPE_INT32 => Some(i32::from_le_bytes(self.array(field)?) as i128),
            GGUF_TYPE_UINT64 => Some(self.u64(field)? as i128),
            GGUF_TYPE_INT64 => Some(self.i64(field)? as i128),
            GGUF_TYPE_BOOL => {
                self.skip(1, field)?;
                None
            }
            GGUF_TYPE_FLOAT32 => {
                self.skip(4, field)?;
                None
            }
            GGUF_TYPE_FLOAT64 => {
                self.skip(8, field)?;
                None
            }
            GGUF_TYPE_STRING => {
                self.skip_string(field)?;
                None
            }
            _ => unreachable!("scalar() called with validated type"),
        })
    }
}

/// Fixed element size for scalar GGUF types, `None` for strings.
fn fixed_size(ty: u32) -> Option<u64> {
    match ty {
        GGUF_TYPE_UINT8 | GGUF_TYPE_INT8 | GGUF_TYPE_BOOL => Some(1),
        GGUF_TYPE_UINT16 | GGUF_TYPE_INT16 => Some(2),
        GGUF_TYPE_UINT32 | GGUF_TYPE_INT32 | GGUF_TYPE_FLOAT32 => Some(4),
        GGUF_TYPE_UINT64 | GGUF_TYPE_INT64 | GGUF_TYPE_FLOAT64 => Some(8),
        _ => None,
    }
}

fn is_int(ty: u32) -> bool {
    matches!(
        ty,
        GGUF_TYPE_UINT8
            | GGUF_TYPE_INT8
            | GGUF_TYPE_UINT16
            | GGUF_TYPE_INT16
            | GGUF_TYPE_UINT32
            | GGUF_TYPE_INT32
            | GGUF_TYPE_UINT64
            | GGUF_TYPE_INT64
    )
}

/// Hyper-parameter suffixes captured from `<arch>.<suffix>` keys. The
/// architecture key may come after them, so capture first and resolve at
/// the end. `array_ok` keys may be per-layer arrays (max is taken).
const TRACKED: &[(&str, bool)] = &[
    ("context_length", false),
    ("embedding_length", false),
    ("block_count", false),
    ("attention.head_count", true),
    ("attention.head_count_kv", true),
    ("attention.key_length", false),
    ("attention.value_length", false),
    ("expert_count", false),
    ("expert_used_count", false),
    ("expert_group_count", false),
    ("expert_group_used_count", false),
    ("posnet.block_count", false),
    ("convnext.block_count", false),
];

fn tracked_suffix(key: &str) -> Option<(&'static str, bool)> {
    let (_, rest) = key.split_once('.')?;
    TRACKED.iter().find(|(s, _)| *s == rest).copied()
}

fn utf8(bytes: Vec<u8>, field: impl Into<String>) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| GgufCheckError::InvalidUtf8 {
        field: field.into(),
    })
}

fn pad(x: u64, align: u64) -> Option<u64> {
    // align is a validated power of two
    x.checked_add(align - 1).map(|v| v & !(align - 1))
}

fn parse<R: Read>(inner: R, readable: u64, file_len: u64) -> Result<GgufSummary> {
    let mut r = Reader {
        inner,
        pos: 0,
        limit: readable,
    };

    // --- header -----------------------------------------------------------
    let magic: [u8; 4] = r.array("magic")?;
    if &magic != b"GGUF" {
        return Err(GgufCheckError::BadMagic(magic));
    }
    let version = r.u32("version")?;
    if version != 0 && version & 0xFFFF == 0 {
        return Err(GgufCheckError::EndianMismatch(version));
    }
    if !(2..=GGUF_MAX_VERSION).contains(&version) {
        return Err(GgufCheckError::UnsupportedVersion(version));
    }
    let tensor_count = r.u64("tensor_count")?;
    if tensor_count > MAX_TENSOR_COUNT {
        return Err(GgufCheckError::CountTooLarge {
            field: "tensor_count",
            count: tensor_count,
            max: MAX_TENSOR_COUNT,
        });
    }
    let kv_count = r.u64("kv_count")?;
    if kv_count > MAX_KV_COUNT {
        return Err(GgufCheckError::CountTooLarge {
            field: "kv_count",
            count: kv_count,
            max: MAX_KV_COUNT,
        });
    }
    // Each KV is at least key-len(8) + type(4) + 1 byte; each tensor info at
    // least name-len(8) + n_dims(4) + one dim(8) + type(4) + offset(8).
    r.need(
        kv_count * 13 + tensor_count * 32,
        "metadata and tensor table",
    )?;

    // --- key/value pairs --------------------------------------------------
    let mut keys: HashSet<String> = HashSet::new();
    let mut architecture: Option<String> = None;
    let mut alignment = GGUF_DEFAULT_ALIGNMENT;
    let mut captured: HashMap<String, i128> = HashMap::new();

    for index in 0..kv_count {
        let key = r.string("metadata key", MAX_KEY_LEN)?;
        if key.is_empty() {
            return Err(GgufCheckError::EmptyKey { index });
        }
        let key = utf8(key, "metadata key")?;
        if !keys.insert(key.clone()) {
            return Err(GgufCheckError::DuplicateKey(key));
        }
        let mut ty = r.u32(&key)?;
        let is_array = ty == GGUF_TYPE_ARRAY;
        let mut n = 1u64;
        if is_array {
            ty = r.u32(&key)?;
            n = r.u64(&key)?;
            if ty == GGUF_TYPE_ARRAY {
                return Err(GgufCheckError::NestedArray { key });
            }
        }
        if ty > GGUF_TYPE_FLOAT64 || ty == GGUF_TYPE_ARRAY {
            return Err(GgufCheckError::UnknownValueType { key, type_id: ty });
        }
        if is_array {
            if n > MAX_ARRAY_LEN {
                return Err(GgufCheckError::ArrayTooLong {
                    key,
                    len: n,
                    max: MAX_ARRAY_LEN,
                });
            }
            // Strings carry at least their 8-byte length prefix.
            let min = n * fixed_size(ty).unwrap_or(8);
            r.need(min, &key)?;
        }

        let tracked = tracked_suffix(&key);
        match key.as_str() {
            "general.architecture" => {
                if is_array || ty != GGUF_TYPE_STRING {
                    return Err(GgufCheckError::WrongValueType {
                        key,
                        expected: "string",
                        found: if is_array { GGUF_TYPE_ARRAY } else { ty },
                    });
                }
                let v = r.string(&key, MAX_KEY_LEN)?;
                architecture = Some(utf8(v, key)?);
            }
            "general.alignment" => {
                if is_array || ty != GGUF_TYPE_UINT32 {
                    return Err(GgufCheckError::WrongValueType {
                        key,
                        expected: "uint32",
                        found: if is_array { GGUF_TYPE_ARRAY } else { ty },
                    });
                }
                let a = r.u32(&key)? as u64;
                if a == 0 || !a.is_power_of_two() {
                    return Err(GgufCheckError::BadAlignment(a));
                }
                alignment = a;
            }
            _ if tracked.is_some() => {
                let array_ok = tracked.is_some_and(|(_, ok)| ok);
                if !is_int(ty) || (is_array && !array_ok) {
                    return Err(GgufCheckError::WrongValueType {
                        key,
                        expected: "integer",
                        found: if is_array { GGUF_TYPE_ARRAY } else { ty },
                    });
                }
                // Consume every element; keep the first negative one (to
                // report it) or else the max over layers.
                let mut max: Option<i128> = None;
                let mut negative: Option<i128> = None;
                for _ in 0..n {
                    let v = r.scalar(ty, &key)?.unwrap_or(0);
                    if v < 0 && negative.is_none() {
                        negative = Some(v);
                    }
                    max = Some(max.map_or(v, |m| m.max(v)));
                }
                if let Some(v) = negative.or(max) {
                    captured.insert(key, v);
                }
            }
            _ => {
                if let Some(size) = fixed_size(ty) {
                    r.skip(n * size, &key)?;
                } else {
                    for _ in 0..n {
                        r.skip_string(&key)?;
                    }
                }
            }
        }
        if r.pos > MAX_HEADER_BYTES {
            return Err(GgufCheckError::HeaderTooLarge {
                len: r.pos,
                max: MAX_HEADER_BYTES,
            });
        }
    }

    // --- tensor infos -----------------------------------------------------
    let mut names: HashSet<String> = HashSet::new();
    let mut expected_offset = 0u64;
    let mut tensor_bytes = 0u64;
    // (relative end of data, tensor name) for the furthest-reaching tensor.
    let mut furthest: Option<(u64, String)> = None;

    for index in 0..tensor_count {
        let len = r.u64("tensor name")?;
        if len >= GGML_MAX_NAME as u64 {
            return Err(GgufCheckError::TensorNameTooLong { index, len });
        }
        let mut buf = [0u8; GGML_MAX_NAME];
        r.fill(&mut buf[..len as usize], "tensor name")?;
        let name = utf8(
            buf[..len as usize].to_vec(),
            format!("tensor #{index} name"),
        )?;
        if !names.insert(name.clone()) {
            return Err(GgufCheckError::DuplicateTensor(name));
        }

        let n_dims = r.u32(&name)?;
        if n_dims == 0 || n_dims > GGML_MAX_DIMS {
            return Err(GgufCheckError::BadDimCount {
                tensor: name,
                n_dims,
            });
        }
        let mut ne = [1u64; GGML_MAX_DIMS as usize];
        let mut nelements: u64 = 1;
        for dim in 0..n_dims {
            let v = r.i64(&name)?;
            if v < 1 {
                return Err(GgufCheckError::BadDim {
                    tensor: name,
                    dim,
                    value: v,
                });
            }
            ne[dim as usize] = v as u64;
            nelements = match nelements.checked_mul(v as u64) {
                Some(x) if x <= i64::MAX as u64 => x,
                _ => return Err(GgufCheckError::ElementOverflow { tensor: name }),
            };
        }

        let type_id = r.u32(&name)?;
        let gt = match GGML_TYPES.get(type_id as usize) {
            None => {
                return Err(GgufCheckError::UnknownTensorType {
                    tensor: name,
                    type_id,
                })
            }
            Some(None) => {
                return Err(GgufCheckError::RemovedTensorType {
                    tensor: name,
                    type_id,
                })
            }
            Some(Some(gt)) => *gt,
        };
        if ne[0] % gt.blck_size != 0 {
            return Err(GgufCheckError::RowNotBlockMultiple {
                tensor: name,
                ne0: ne[0],
                block: gt.blck_size,
            });
        }
        let nbytes = match (nelements / gt.blck_size).checked_mul(gt.type_size) {
            Some(b) if b <= usize::MAX as u64 && b <= i64::MAX as u64 => b,
            _ => return Err(GgufCheckError::ByteSizeOverflow { tensor: name }),
        };

        let offset = r.u64(&name)?;
        if offset % alignment != 0 {
            return Err(GgufCheckError::MisalignedOffset {
                tensor: name,
                offset,
                alignment,
            });
        }
        if offset != expected_offset {
            return Err(GgufCheckError::UnexpectedOffset {
                tensor: name,
                expected: expected_offset,
                found: offset,
            });
        }
        let end = offset
            .checked_add(nbytes)
            .ok_or_else(|| GgufCheckError::ByteSizeOverflow {
                tensor: name.clone(),
            })?;
        expected_offset = pad(end, alignment).ok_or_else(|| GgufCheckError::ByteSizeOverflow {
            tensor: name.clone(),
        })?;
        tensor_bytes = tensor_bytes.saturating_add(nbytes);
        furthest = Some((end, name));

        if r.pos > MAX_HEADER_BYTES {
            return Err(GgufCheckError::HeaderTooLarge {
                len: r.pos,
                max: MAX_HEADER_BYTES,
            });
        }
    }

    // --- data section -----------------------------------------------------
    let data_offset = if tensor_count > 0 {
        pad(r.pos, alignment).ok_or_else(|| r.truncated("data section"))?
    } else {
        r.pos
    };
    if data_offset > MAX_HEADER_BYTES {
        return Err(GgufCheckError::HeaderTooLarge {
            len: data_offset,
            max: MAX_HEADER_BYTES,
        });
    }
    if data_offset > file_len {
        return Err(GgufCheckError::Truncated {
            field: "data section".into(),
            offset: data_offset,
        });
    }
    if let Some((rel_end, tensor)) = furthest {
        let end = data_offset.saturating_add(rel_end);
        if end > file_len {
            return Err(GgufCheckError::TensorOutOfBounds {
                tensor,
                end,
                file_len,
            });
        }
    }

    // --- architecture + hyper-parameters ----------------------------------
    let architecture = architecture.ok_or(GgufCheckError::MissingArchitecture)?;
    if !is_known_architecture(&architecture) {
        return Err(GgufCheckError::UnknownArchitecture(architecture));
    }
    let get = |suffix: &str, min: u64, max: u64| -> Result<Option<u64>> {
        let key = format!("{architecture}.{suffix}");
        match captured.get(&key) {
            None => Ok(None),
            Some(&v) if v >= min as i128 && v <= max as i128 => Ok(Some(v as u64)),
            Some(&v) => Err(GgufCheckError::OutOfRange {
                key,
                value: v,
                min,
                max,
            }),
        }
    };
    let context_length = get("context_length", 1, MAX_CONTEXT_LENGTH)?;
    let embedding_length = get("embedding_length", 1, MAX_EMBEDDING_LENGTH)?;
    let block_count = get("block_count", 1, MAX_BLOCK_COUNT)?;
    // Recurrent architectures (mamba, rwkv) legitimately write 0 heads.
    let head_count = get("attention.head_count", 0, MAX_HEAD_COUNT)?;
    let head_count_kv = get("attention.head_count_kv", 0, MAX_HEAD_COUNT)?;
    let key_length = get("attention.key_length", 1, MAX_HEAD_DIM)?;
    let value_length = get("attention.value_length", 1, MAX_HEAD_DIM)?;
    if architecture != "clip" {
        check_hparam_invariants(&architecture, block_count, &get)?;
    }

    let expert_count = get("expert_count", 0, MAX_EXPERT_COUNT)?;
    Ok(GgufSummary {
        version,
        architecture,
        tensor_count,
        kv_count,
        alignment,
        data_offset,
        file_len,
        tensor_bytes,
        context_length,
        embedding_length,
        block_count,
        head_count,
        head_count_kv,
        key_length,
        value_length,
        expert_count,
    })
}

/// `(suffix, min, max)` → the range-checked `<arch>.<suffix>` value.
type HparamLookup<'a> = &'a dyn Fn(&str, u64, u64) -> Result<Option<u64>>;

/// The generic `GGML_ASSERT`s in `llama_model::load_hparams` (vendored
/// src/llama-model.cpp), which run for every non-CLIP architecture right
/// after the keys are read. A file violating them would abort the process.
fn check_hparam_invariants(
    arch: &str,
    block_count: Option<u64>,
    get: HparamLookup<'_>,
) -> Result<()> {
    let bad = |suffix: &str, detail: String| GgufCheckError::Inconsistent {
        key: format!("{arch}.{suffix}"),
        detail,
    };
    let n_layer = block_count.unwrap_or(0);
    let mut n_expert = get("expert_count", 0, MAX_EXPERT_COUNT)?.unwrap_or(0);
    let mut used = get("expert_used_count", 0, MAX_EXPERT_COUNT)?.unwrap_or(0);
    let groups = get("expert_group_count", 0, u32::MAX as u64)?.unwrap_or(0);
    let group_used = get("expert_group_used_count", 0, u32::MAX as u64)?.unwrap_or(0);

    if arch == "wavtokenizer-dec" {
        for sub in ["posnet.block_count", "convnext.block_count"] {
            if let Some(n) = get(sub, 0, MAX_BLOCK_COUNT)? {
                if n > n_layer {
                    return Err(bad(sub, format!("{n} exceeds block_count {n_layer}")));
                }
            }
        }
    }
    if (arch == "hunyuan_vl" || arch == "hunyuan-dense") && n_expert <= 1 {
        n_expert = 0;
        used = 0;
    }
    if used > n_expert {
        return Err(bad(
            "expert_used_count",
            format!("{used} exceeds expert_count {n_expert}"),
        ));
    }
    if n_expert > 0 {
        if used == 0 {
            return Err(bad(
                "expert_used_count",
                "must be > 0 when experts exist".into(),
            ));
        }
        if groups >= n_expert {
            return Err(bad(
                "expert_group_count",
                format!("{groups} must be < expert_count {n_expert}"),
            ));
        }
        if groups > 1 {
            if n_expert % groups != 0 {
                return Err(bad(
                    "expert_group_count",
                    format!("{n_expert} experts not divisible by {groups} groups"),
                ));
            }
            if group_used == 0 || group_used >= groups {
                return Err(bad(
                    "expert_group_used_count",
                    format!("{group_used} must be in 1..{groups}"),
                ));
            }
        }
    } else if groups != 0 {
        return Err(bad(
            "expert_group_count",
            "must be 0 without experts".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Synthetic GGUF writer for tests (and the memory-budget tests).
    pub(crate) struct Builder {
        pub version: u32,
        pub alignment: u64,
        kvs: Vec<u8>,
        n_kv: u64,
        tensors: Vec<u8>,
        n_tensors: u64,
        next_offset: u64,
        /// Bytes of tensor data appended after the aligned header; defaults
        /// to exactly what the tensors need.
        pub data_len: Option<u64>,
        /// Overrides for the header counts.
        pub n_kv_override: Option<u64>,
        pub n_tensors_override: Option<u64>,
    }

    fn put_str(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s);
    }

    impl Builder {
        pub fn new() -> Self {
            Builder {
                version: 3,
                alignment: GGUF_DEFAULT_ALIGNMENT,
                kvs: Vec::new(),
                n_kv: 0,
                tensors: Vec::new(),
                n_tensors: 0,
                next_offset: 0,
                data_len: None,
                n_kv_override: None,
                n_tensors_override: None,
            }
        }

        /// A minimal valid llama-shaped file.
        pub fn minimal() -> Self {
            let mut b = Builder::new();
            b.kv_str("general.architecture", "llama");
            b.kv_u32("llama.context_length", 2048);
            b.kv_u32("llama.embedding_length", 64);
            b.kv_u32("llama.block_count", 2);
            b.kv_u32("llama.attention.head_count", 4);
            b.kv_u32("llama.attention.head_count_kv", 2);
            b.kv_raw("tokenizer.ggml.tokens", GGUF_TYPE_ARRAY, &{
                let mut v = Vec::new();
                v.extend_from_slice(&GGUF_TYPE_STRING.to_le_bytes());
                v.extend_from_slice(&2u64.to_le_bytes());
                put_str(&mut v, b"<s>");
                put_str(&mut v, b"hi");
                v
            });
            b.tensor("token_embd.weight", &[64, 10], 0); // f32
            b.tensor("blk.0.attn_q.weight", &[256, 2], 12); // q4_K
            b
        }

        pub fn kv_raw(&mut self, key: &str, ty: u32, value: &[u8]) -> &mut Self {
            put_str(&mut self.kvs, key.as_bytes());
            self.kvs.extend_from_slice(&ty.to_le_bytes());
            self.kvs.extend_from_slice(value);
            self.n_kv += 1;
            self
        }

        pub fn kv_str(&mut self, key: &str, v: &str) -> &mut Self {
            let mut b = Vec::new();
            put_str(&mut b, v.as_bytes());
            self.kv_raw(key, GGUF_TYPE_STRING, &b)
        }

        pub fn kv_u32(&mut self, key: &str, v: u32) -> &mut Self {
            self.kv_raw(key, GGUF_TYPE_UINT32, &v.to_le_bytes())
        }

        pub fn kv_u64(&mut self, key: &str, v: u64) -> &mut Self {
            self.kv_raw(key, GGUF_TYPE_UINT64, &v.to_le_bytes())
        }

        /// Append a tensor at the next contiguous aligned offset.
        pub fn tensor(&mut self, name: &str, dims: &[i64], ty: u32) -> &mut Self {
            let off = self.next_offset;
            self.tensor_at(name, dims, ty, off)
        }

        pub fn tensor_at(&mut self, name: &str, dims: &[i64], ty: u32, offset: u64) -> &mut Self {
            put_str(&mut self.tensors, name.as_bytes());
            self.tensors
                .extend_from_slice(&(dims.len() as u32).to_le_bytes());
            for d in dims {
                self.tensors.extend_from_slice(&d.to_le_bytes());
            }
            self.tensors.extend_from_slice(&ty.to_le_bytes());
            self.tensors.extend_from_slice(&offset.to_le_bytes());
            self.n_tensors += 1;
            if let Some(gt) = ggml_type(ty) {
                let n = dims.iter().try_fold(1i64, |a, d| a.checked_mul(*d));
                if let Some(n) = n.filter(|n| *n > 0 && dims.iter().all(|d| *d > 0)) {
                    let bytes = (n as u64 / gt.blck_size).saturating_mul(gt.type_size);
                    let end = offset.saturating_add(bytes);
                    if end < (1 << 40) {
                        self.next_offset = end.div_ceil(self.alignment) * self.alignment;
                    }
                }
            }
            self
        }

        pub fn build(&self) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(b"GGUF");
            out.extend_from_slice(&self.version.to_le_bytes());
            out.extend_from_slice(
                &self
                    .n_tensors_override
                    .unwrap_or(self.n_tensors)
                    .to_le_bytes(),
            );
            out.extend_from_slice(&self.n_kv_override.unwrap_or(self.n_kv).to_le_bytes());
            out.extend_from_slice(&self.kvs);
            out.extend_from_slice(&self.tensors);
            if self.n_tensors > 0 {
                while !(out.len() as u64).is_multiple_of(self.alignment) {
                    out.push(0);
                }
            }
            let data = self.data_len.unwrap_or(self.next_offset);
            out.resize(out.len() + data as usize, 0);
            out
        }
    }

    fn check(b: &Builder) -> Result<GgufSummary> {
        let bytes = b.build();
        check_bytes(&bytes, bytes.len() as u64)
    }

    #[test]
    fn minimal_file_passes() {
        let s = check(&Builder::minimal()).expect("minimal file must pass");
        assert_eq!(s.version, 3);
        assert_eq!(s.architecture, "llama");
        assert_eq!(s.tensor_count, 2);
        assert_eq!(s.kv_count, 7);
        assert_eq!(s.alignment, 32);
        assert_eq!(s.data_offset % 32, 0);
        assert_eq!(s.tensor_bytes, 64 * 10 * 4 + 2 * 144);
        assert_eq!(s.context_length, Some(2048));
        assert_eq!(s.embedding_length, Some(64));
        assert_eq!(s.block_count, Some(2));
        assert_eq!(s.head_count, Some(4));
        assert_eq!(s.head_count_kv, Some(2));
        assert_eq!(s.key_length, None);
    }

    #[test]
    fn version_2_and_custom_alignment_pass() {
        let mut b = Builder::new();
        b.version = 2;
        b.alignment = 64;
        b.kv_str("general.architecture", "clip");
        b.kv_u32("general.alignment", 64);
        b.tensor("a", &[3], 0).tensor("b", &[5, 2], 1);
        let s = check(&b).unwrap();
        assert_eq!(
            (s.version, s.alignment, s.architecture.as_str()),
            (2, 64, "clip")
        );
    }

    #[test]
    fn zero_tensors_pass() {
        let mut b = Builder::new();
        b.kv_str("general.architecture", "bert");
        assert_eq!(check(&b).unwrap().tensor_count, 0);
    }

    #[test]
    fn per_layer_head_count_array_takes_max() {
        let mut b = Builder::minimal();
        let mut v = Vec::new();
        v.extend_from_slice(&GGUF_TYPE_INT32.to_le_bytes());
        v.extend_from_slice(&3u64.to_le_bytes());
        for x in [4i32, 8, 2] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        b.kv_raw("openelm.attention.head_count", GGUF_TYPE_ARRAY, &v);
        b.kv_str("general.name", "x");
        // Not the file's arch: captured but ignored.
        assert!(check(&b).is_ok());
        let mut b = Builder::new();
        b.kv_str("general.architecture", "openelm");
        b.kv_raw("openelm.attention.head_count", GGUF_TYPE_ARRAY, &v);
        assert_eq!(check(&b).unwrap().head_count, Some(8));
    }

    // --- one failing case per rule ---------------------------------------

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = Builder::minimal().build();
        bytes[0] = b'X';
        assert!(matches!(
            check_bytes(&bytes, bytes.len() as u64),
            Err(GgufCheckError::BadMagic(_))
        ));
    }

    #[test]
    fn rejects_versions() {
        for (v, big) in [
            (0u32, false),
            (1, false),
            (4, false),
            (3u32.swap_bytes(), true),
        ] {
            let mut b = Builder::minimal();
            b.version = v;
            let e = check(&b).unwrap_err();
            if big {
                assert!(matches!(e, GgufCheckError::EndianMismatch(_)), "{e}");
            } else {
                assert_eq!(e, GgufCheckError::UnsupportedVersion(v));
            }
        }
    }

    #[test]
    fn rejects_huge_counts() {
        let mut b = Builder::minimal();
        b.n_tensors_override = Some(u64::MAX);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::CountTooLarge {
                field: "tensor_count",
                ..
            })
        ));
        let mut b = Builder::minimal();
        b.n_kv_override = Some(MAX_KV_COUNT + 1);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::CountTooLarge {
                field: "kv_count",
                ..
            })
        ));
        // Within the cap but more than the bytes can hold.
        let mut b = Builder::minimal();
        b.n_kv_override = Some(MAX_KV_COUNT);
        assert!(matches!(check(&b), Err(GgufCheckError::Truncated { .. })));
    }

    #[test]
    fn every_truncation_is_an_error() {
        let bytes = Builder::minimal().build();
        let s = check_bytes(&bytes, bytes.len() as u64).unwrap();
        // Every prefix that stops inside the header, metadata or tensor
        // table fails; once the table is complete (only alignment padding
        // missing) the slice is enough, since file_len is given separately.
        let first_ok = (0..=s.data_offset as usize)
            .find(|cut| check_bytes(&bytes[..*cut], bytes.len() as u64).is_ok())
            .unwrap();
        assert!(first_ok as u64 > s.data_offset - s.alignment, "{first_ok}");
        for cut in first_ok..=s.data_offset as usize {
            assert_eq!(
                check_bytes(&bytes[..cut], bytes.len() as u64).as_ref(),
                Ok(&s)
            );
        }
        for cut in 0..first_ok {
            assert!(check_bytes(&bytes[..cut], bytes.len() as u64).is_err());
        }
        // Full header but the file is shorter than the tensor data.
        let r = check_bytes(&bytes, bytes.len() as u64 - 1);
        assert!(
            matches!(r, Err(GgufCheckError::TensorOutOfBounds { .. })),
            "{r:?}"
        );
    }

    #[test]
    fn rejects_long_strings() {
        let mut b = Builder::minimal();
        let mut v = Vec::new();
        v.extend_from_slice(&(MAX_STRING_LEN + 1).to_le_bytes());
        b.kv_raw("general.description", GGUF_TYPE_STRING, &v);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::StringTooLong { .. })
        ));
        let mut b = Builder::minimal();
        b.kv_u32(&"k".repeat(MAX_KEY_LEN as usize + 1), 1);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::StringTooLong { .. })
        ));
    }

    #[test]
    fn rejects_long_arrays() {
        let mut b = Builder::minimal();
        let mut v = Vec::new();
        v.extend_from_slice(&GGUF_TYPE_UINT8.to_le_bytes());
        v.extend_from_slice(&(MAX_ARRAY_LEN + 1).to_le_bytes());
        b.kv_raw("x.arr", GGUF_TYPE_ARRAY, &v);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::ArrayTooLong { .. })
        ));
        let mut b = Builder::minimal();
        let mut v = Vec::new();
        v.extend_from_slice(&GGUF_TYPE_STRING.to_le_bytes());
        v.extend_from_slice(&(1u64 << 29).to_le_bytes());
        b.kv_raw("x.arr", GGUF_TYPE_ARRAY, &v);
        assert!(matches!(check(&b), Err(GgufCheckError::Truncated { .. })));
    }

    #[test]
    fn rejects_bad_keys() {
        let mut b = Builder::minimal();
        b.kv_u32("", 1);
        assert!(matches!(check(&b), Err(GgufCheckError::EmptyKey { .. })));
        let mut b = Builder::minimal();
        b.kv_u32("llama.block_count", 2);
        assert_eq!(
            check(&b),
            Err(GgufCheckError::DuplicateKey("llama.block_count".into()))
        );
        let mut b = Builder::minimal();
        let mut raw = Vec::new();
        put_str(&mut raw, &[0xff, 0xfe]);
        b.kvs.extend_from_slice(&raw);
        b.kvs.extend_from_slice(&GGUF_TYPE_UINT8.to_le_bytes());
        b.kvs.push(0);
        b.n_kv += 1;
        assert!(matches!(check(&b), Err(GgufCheckError::InvalidUtf8 { .. })));
    }

    #[test]
    fn rejects_unknown_value_types_and_nested_arrays() {
        let mut b = Builder::minimal();
        b.kv_raw("x.y", 13, &[0; 8]);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::UnknownValueType { type_id: 13, .. })
        ));
        let mut b = Builder::minimal();
        let mut v = Vec::new();
        v.extend_from_slice(&GGUF_TYPE_ARRAY.to_le_bytes());
        v.extend_from_slice(&1u64.to_le_bytes());
        b.kv_raw("x.nested", GGUF_TYPE_ARRAY, &v);
        assert!(matches!(check(&b), Err(GgufCheckError::NestedArray { .. })));
        let mut b = Builder::minimal();
        let mut v = Vec::new();
        v.extend_from_slice(&99u32.to_le_bytes());
        v.extend_from_slice(&1u64.to_le_bytes());
        b.kv_raw("x.badarr", GGUF_TYPE_ARRAY, &v);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::UnknownValueType { type_id: 99, .. })
        ));
    }

    #[test]
    fn rejects_bad_alignment() {
        let mut b = Builder::minimal();
        b.kv_u32("general.alignment", 48);
        assert_eq!(check(&b), Err(GgufCheckError::BadAlignment(48)));
        let mut b = Builder::minimal();
        b.kv_u32("general.alignment", 0);
        assert_eq!(check(&b), Err(GgufCheckError::BadAlignment(0)));
        let mut b = Builder::minimal();
        b.kv_u64("general.alignment", 32);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::WrongValueType { .. })
        ));
    }

    #[test]
    fn rejects_bad_tensor_names() {
        let mut b = Builder::minimal();
        b.tensor(&"n".repeat(GGML_MAX_NAME), &[4], 0);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::TensorNameTooLong { len: 64, .. })
        ));
        let mut b = Builder::minimal();
        b.tensor(&"n".repeat(GGML_MAX_NAME - 1), &[4], 0);
        assert!(check(&b).is_ok());
        let mut b = Builder::minimal();
        b.tensor("token_embd.weight", &[4], 0);
        assert_eq!(
            check(&b),
            Err(GgufCheckError::DuplicateTensor("token_embd.weight".into()))
        );
    }

    #[test]
    fn rejects_bad_shapes() {
        for dims in [&[][..], &[1, 1, 1, 1, 1][..]] {
            let mut b = Builder::minimal();
            b.tensor("x", dims, 0);
            assert!(
                matches!(check(&b), Err(GgufCheckError::BadDimCount { .. })),
                "{dims:?}"
            );
        }
        for dims in [&[0i64][..], &[4, -1][..]] {
            let mut b = Builder::minimal();
            b.tensor("x", dims, 0);
            assert!(
                matches!(check(&b), Err(GgufCheckError::BadDim { .. })),
                "{dims:?}"
            );
        }
        let mut b = Builder::minimal();
        b.tensor("x", &[i64::MAX, 2], 0);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::ElementOverflow { .. })
        ));
        // Element count fits in i64 but the byte size overflows usize/i64.
        let mut b = Builder::minimal();
        b.tensor("x", &[1 << 31, 1 << 30], 28); // f64: 8 bytes each
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::ByteSizeOverflow { .. })
        ));
    }

    #[test]
    fn rejects_bad_tensor_types() {
        let mut b = Builder::minimal();
        b.tensor("x", &[32], GGML_TYPE_COUNT as u32);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::UnknownTensorType { .. })
        ));
        for removed in [4u32, 5, 31, 32, 33, 36, 37, 38] {
            let mut b = Builder::minimal();
            b.tensor("x", &[256], removed);
            assert!(
                matches!(check(&b), Err(GgufCheckError::RemovedTensorType { .. })),
                "{removed}"
            );
        }
        let mut b = Builder::minimal();
        b.tensor("x", &[100, 2], 8); // q8_0 block 32
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::RowNotBlockMultiple {
                ne0: 100,
                block: 32,
                ..
            })
        ));
    }

    #[test]
    fn rejects_bad_offsets() {
        let mut b = Builder::minimal();
        let next = b.next_offset;
        b.tensor_at("x", &[8], 0, next + 1);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::MisalignedOffset { .. })
        ));
        // Overlap with the previous tensor.
        let mut b = Builder::minimal();
        b.tensor_at("x", &[8], 0, 0);
        assert!(
            matches!(check(&b), Err(GgufCheckError::UnexpectedOffset { expected, found: 0, .. }) if expected > 0)
        );
        // A gap.
        let mut b = Builder::minimal();
        let next = b.next_offset;
        b.tensor_at("x", &[8], 0, next + 32);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::UnexpectedOffset { .. })
        ));
        // Tensor data past EOF.
        let mut b = Builder::minimal();
        b.data_len = Some(10);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::TensorOutOfBounds { .. })
        ));
    }

    #[test]
    fn rejects_architecture_problems() {
        let mut b = Builder::new();
        b.kv_u32("llama.block_count", 2);
        assert_eq!(check(&b), Err(GgufCheckError::MissingArchitecture));
        let mut b = Builder::new();
        b.kv_str("general.architecture", "skynet");
        assert_eq!(
            check(&b),
            Err(GgufCheckError::UnknownArchitecture("skynet".into()))
        );
        let mut b = Builder::new();
        b.kv_u32("general.architecture", 1);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::WrongValueType { .. })
        ));
        assert!(!is_known_architecture("(unknown)"));
        assert!(is_known_architecture("gemma-embedding2"));
        assert!(is_known_architecture("clip"));
    }

    #[test]
    fn rejects_out_of_range_hyperparameters() {
        let cases: &[(&str, u64)] = &[
            ("llama.context_length", 0),
            ("llama.context_length", MAX_CONTEXT_LENGTH + 1),
            ("llama.embedding_length", 0),
            ("llama.embedding_length", MAX_EMBEDDING_LENGTH + 1),
            ("llama.block_count", 0),
            ("llama.block_count", MAX_BLOCK_COUNT + 1),
            ("llama.attention.head_count", MAX_HEAD_COUNT + 1),
            ("llama.attention.head_count_kv", MAX_HEAD_COUNT + 1),
            ("llama.attention.key_length", 0),
            ("llama.attention.value_length", MAX_HEAD_DIM + 1),
        ];
        for (key, v) in cases {
            let mut b = Builder::new();
            b.kv_str("general.architecture", "llama");
            b.kv_u64(key, *v);
            let e = check(&b).unwrap_err();
            assert!(
                matches!(&e, GgufCheckError::OutOfRange { key: k, .. } if k == key),
                "{key}={v}: {e}"
            );
        }
        // Negative.
        let mut b = Builder::new();
        b.kv_str("general.architecture", "llama");
        b.kv_raw("llama.block_count", GGUF_TYPE_INT32, &(-3i32).to_le_bytes());
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::OutOfRange { value: -3, .. })
        ));
        // Wrong type.
        let mut b = Builder::new();
        b.kv_str("general.architecture", "llama");
        b.kv_raw(
            "llama.context_length",
            GGUF_TYPE_FLOAT32,
            &1.0f32.to_le_bytes(),
        );
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::WrongValueType { .. })
        ));
        // Zero heads is legal (recurrent models).
        let mut b = Builder::new();
        b.kv_str("general.architecture", "mamba");
        b.kv_u32("mamba.attention.head_count", 0);
        assert_eq!(check(&b).unwrap().head_count, Some(0));
    }

    #[test]
    fn rejects_llama_hparam_assert_violations() {
        let base = || {
            let mut b = Builder::new();
            b.kv_str("general.architecture", "qwen3moe");
            b.kv_u32("qwen3moe.block_count", 4);
            b
        };
        let ok = |b: &Builder| check(b).map(|s| s.expert_count);
        let mut b = base();
        b.kv_u32("qwen3moe.expert_count", 8)
            .kv_u32("qwen3moe.expert_used_count", 2);
        assert_eq!(ok(&b), Ok(Some(8)));
        let mut b = base();
        b.kv_u32("qwen3moe.expert_count", 513);
        assert!(matches!(check(&b), Err(GgufCheckError::OutOfRange { .. })));
        let cases: &[&[(&str, u32)]] = &[
            &[("expert_count", 8), ("expert_used_count", 9)],
            &[("expert_count", 8)],
            &[
                ("expert_count", 8),
                ("expert_used_count", 2),
                ("expert_group_count", 8),
            ],
            &[
                ("expert_count", 8),
                ("expert_used_count", 2),
                ("expert_group_count", 3),
                ("expert_group_used_count", 1),
            ],
            &[
                ("expert_count", 8),
                ("expert_used_count", 2),
                ("expert_group_count", 4),
            ],
            &[("expert_used_count", 1)],
            &[("expert_group_count", 2)],
        ];
        for kvs in cases {
            let mut b = base();
            for (k, v) in *kvs {
                b.kv_u32(&format!("qwen3moe.{k}"), *v);
            }
            assert!(
                matches!(check(&b), Err(GgufCheckError::Inconsistent { .. })),
                "{kvs:?}"
            );
        }
        // Hunyuan zeroes a single expert before the asserts.
        let mut b = Builder::new();
        b.kv_str("general.architecture", "hunyuan-dense");
        b.kv_u32("hunyuan-dense.expert_count", 1)
            .kv_u32("hunyuan-dense.expert_used_count", 1);
        assert!(check(&b).is_ok());
        // wavtokenizer sub-stacks may not exceed block_count.
        let mut b = Builder::new();
        b.kv_str("general.architecture", "wavtokenizer-dec");
        b.kv_u32("wavtokenizer-dec.block_count", 2)
            .kv_u32("wavtokenizer-dec.posnet.block_count", 3);
        assert!(matches!(
            check(&b),
            Err(GgufCheckError::Inconsistent { .. })
        ));
        // block_count above LLAMA_MAX_LAYERS.
        let mut b = Builder::new();
        b.kv_str("general.architecture", "llama");
        b.kv_u32("llama.block_count", 513);
        assert!(matches!(check(&b), Err(GgufCheckError::OutOfRange { .. })));
    }

    #[test]
    fn caps_match_vendored_llama_hparams() {
        let h = vendored("src/llama-hparams.h");
        let def = |name: &str| -> u64 {
            let l = h
                .lines()
                .find(|l| l.starts_with(&format!("#define {name} ")))
                .unwrap();
            l.split_whitespace().nth(2).unwrap().parse().unwrap()
        };
        assert_eq!(def("LLAMA_MAX_LAYERS"), MAX_BLOCK_COUNT);
        assert_eq!(def("LLAMA_MAX_EXPERTS"), MAX_EXPERT_COUNT);
    }

    #[test]
    fn check_file_matches_check_bytes() {
        let bytes = Builder::minimal().build();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("m.gguf");
        std::fs::write(&p, &bytes).unwrap();
        assert_eq!(check_file(&p), check_bytes(&bytes, bytes.len() as u64));
        assert!(matches!(
            check_file(&dir.path().join("missing.gguf")),
            Err(GgufCheckError::Io(_))
        ));
    }

    // --- drift tests against the vendored llama.cpp ----------------------

    fn vendored(rel: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/llama-cpp-sys-2/llama.cpp")
            .join(rel);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    #[test]
    fn arch_list_matches_vendored_llama_arch() {
        let src = vendored("src/llama-arch.cpp");
        let start = src
            .find("LLM_ARCH_NAMES = {")
            .expect("LLM_ARCH_NAMES table");
        let body = &src[start..];
        let body = &body[..body.find("};").expect("end of table")];
        let mut names = Vec::new();
        for line in body.lines() {
            let line = line.trim();
            if !line.starts_with("{ LLM_ARCH_") {
                continue;
            }
            let q1 = line.find('"').unwrap();
            let q2 = q1 + 1 + line[q1 + 1..].find('"').unwrap();
            names.push(&line[q1 + 1..q2]);
        }
        names.retain(|n| *n != "(unknown)");
        assert_eq!(
            names, KNOWN_ARCHITECTURES,
            "llama-arch.cpp changed: update KNOWN_ARCHITECTURES"
        );
    }

    /// Minimal C constant-expression evaluator: integers, macros,
    /// `sizeof(T)`, + - * / and parentheses.
    struct CEval<'a> {
        defines: HashMap<String, String>,
        block_sizes: HashMap<String, String>,
        _src: &'a str,
    }

    fn tokenize(s: &str) -> Vec<String> {
        let mut out = Vec::new();
        let cs: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            let c = cs[i];
            if c.is_whitespace() {
                i += 1;
            } else if c.is_alphanumeric() || c == '_' {
                let st = i;
                while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                    i += 1;
                }
                out.push(cs[st..i].iter().collect());
            } else {
                out.push(c.to_string());
                i += 1;
            }
        }
        out
    }

    impl CEval<'_> {
        fn sizeof(&self, ty: &str) -> i64 {
            match ty {
                "int8_t" | "uint8_t" => 1,
                "int16_t" | "uint16_t" | "ggml_half" | "ggml_fp16_t" | "ggml_bf16_t" => 2,
                "int32_t" | "uint32_t" | "float" => 4,
                "int64_t" | "uint64_t" | "double" => 8,
                b if self.block_sizes.contains_key(b) => self.eval(&self.block_sizes[b].clone()),
                other => panic!("unknown sizeof({other})"),
            }
        }

        fn expand(&self, toks: Vec<String>) -> Vec<String> {
            let mut out = Vec::new();
            let mut i = 0;
            while i < toks.len() {
                let t = &toks[i];
                if t == "sizeof" {
                    assert_eq!(toks[i + 1], "(");
                    let ty = &toks[i + 2];
                    assert_eq!(toks[i + 3], ")");
                    out.push(self.sizeof(ty).to_string());
                    i += 4;
                } else if let Some(def) = self.defines.get(t) {
                    out.extend(self.expand(tokenize(def)));
                    i += 1;
                } else {
                    out.push(t.clone());
                    i += 1;
                }
            }
            out
        }

        fn eval(&self, expr: &str) -> i64 {
            let toks = self.expand(tokenize(expr));
            let mut pos = 0;
            let v = Self::sum(&toks, &mut pos);
            assert_eq!(pos, toks.len(), "trailing tokens in {expr}: {toks:?}");
            v
        }

        fn sum(t: &[String], p: &mut usize) -> i64 {
            let mut v = Self::prod(t, p);
            while *p < t.len() && (t[*p] == "+" || t[*p] == "-") {
                let op = t[*p].clone();
                *p += 1;
                let r = Self::prod(t, p);
                v = if op == "+" { v + r } else { v - r };
            }
            v
        }

        fn prod(t: &[String], p: &mut usize) -> i64 {
            let mut v = Self::atom(t, p);
            while *p < t.len() && (t[*p] == "*" || t[*p] == "/") {
                let op = t[*p].clone();
                *p += 1;
                let r = Self::atom(t, p);
                v = if op == "*" { v * r } else { v / r };
            }
            v
        }

        fn atom(t: &[String], p: &mut usize) -> i64 {
            let tok = &t[*p];
            *p += 1;
            if tok == "(" {
                let v = Self::sum(t, p);
                assert_eq!(t[*p], ")");
                *p += 1;
                v
            } else {
                tok.parse()
                    .unwrap_or_else(|_| panic!("not a number: {tok}"))
            }
        }
    }

    #[test]
    fn type_table_matches_vendored_ggml() {
        let header = vendored("ggml/include/ggml.h");
        let ggml_c = vendored("ggml/src/ggml.c");
        let common = vendored("ggml/src/ggml-common.h");

        // enum ggml_type: name -> id, and GGML_TYPE_COUNT.
        let en = &header[header.find("enum ggml_type {").unwrap()..];
        let en = &en[..en.find("};").unwrap()];
        let mut ids: HashMap<String, usize> = HashMap::new();
        for line in en.lines() {
            let line = line.trim();
            if line.starts_with("//") || !line.starts_with("GGML_TYPE_") {
                continue;
            }
            let (name, rest) = line.split_once('=').unwrap();
            let num: String = rest
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            ids.insert(name.trim().to_string(), num.parse().unwrap());
        }
        assert_eq!(
            ids["GGML_TYPE_COUNT"], GGML_TYPE_COUNT,
            "GGML_TYPE_COUNT changed"
        );

        // ggml-common.h: #defines and sizeof(block_*) static_asserts.
        let mut ev = CEval {
            defines: HashMap::new(),
            block_sizes: HashMap::new(),
            _src: "",
        };
        for line in common.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("#define ") {
                let rest = rest.split("//").next().unwrap().trim();
                if let Some((name, body)) = rest.split_once(char::is_whitespace) {
                    if !name.contains('(') {
                        ev.defines.insert(name.to_string(), body.trim().to_string());
                    }
                }
            }
            if let Some(rest) = line.strip_prefix("static_assert(sizeof(block_") {
                let (blk, rest) = rest.split_once(')').unwrap();
                let rhs = rest.trim_start().strip_prefix("==").unwrap();
                let rhs = &rhs[..rhs.rfind(',').unwrap()];
                ev.block_sizes
                    .insert(format!("block_{blk}"), rhs.trim().to_string());
            }
        }

        // type_traits[]: one entry per `[X] = {`.
        let tt = &ggml_c[ggml_c.find("type_traits[GGML_TYPE_COUNT] = {").unwrap()..];
        let tt = &tt[..tt.find("\n};").unwrap()];
        let mut derived: Vec<Option<GgmlType>> = vec![None; GGML_TYPE_COUNT];
        let mut seen = vec![false; GGML_TYPE_COUNT];
        let mut entries = tt.split("\n    [").skip(1);
        for entry in entries.by_ref() {
            let (idx, body) = entry.split_once(']').unwrap();
            let id = idx.parse::<usize>().ok().unwrap_or_else(|| ids[idx]);
            let field = |f: &str| -> String {
                let s = &body[body.find(&format!(".{f}")).unwrap()..];
                let s = &s[s.find('=').unwrap() + 1..];
                s[..s.find(",\n").unwrap()].trim().to_string()
            };
            let blck = ev.eval(&field("blck_size"));
            let size = ev.eval(&field("type_size"));
            let name = field("type_name").trim_matches('"').to_string();
            seen[id] = true;
            if blck != 0 {
                let name: &'static str = Box::leak(name.into_boxed_str());
                derived[id] = Some(GgmlType {
                    name,
                    blck_size: blck as u64,
                    type_size: size as u64,
                });
            }
        }
        assert!(seen.iter().all(|s| *s), "type_traits missing ids: {seen:?}");
        for id in 0..GGML_TYPE_COUNT {
            assert_eq!(
                GGML_TYPES[id], derived[id],
                "ggml type id {id} drifted from vendored ggml.c"
            );
        }
    }

    #[test]
    fn fuzz_seed_corpus_agrees() {
        // core/fuzz/seed_corpus.py writes these with an independent (Python)
        // GGUF writer: valid_* must pass, everything else must be refused.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fuzz/corpus/gguf_header");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut seen = 0;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.len() == 40 {
                continue; // libFuzzer-grown entry
            }
            let bytes = std::fs::read(e.path()).unwrap();
            let r = check_bytes(&bytes, bytes.len() as u64);
            assert_eq!(r.is_ok(), name.starts_with("valid_"), "{name}: {r:?}");
            seen += 1;
        }
        assert!(seen > 0);
    }

    // --- real model files --------------------------------------------------

    fn real_ggufs() -> Vec<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut out = Vec::new();
        for dir in [root.join("../../fixtures/models"), root.join("tests")] {
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x == "gguf") {
                        out.push(p);
                    }
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    #[ignore = "reads the gitignored model fixtures; run with --ignored"]
    fn real_fixtures_pass() {
        let files = real_ggufs();
        if files.is_empty() {
            eprintln!("no .gguf fixtures present; skipping");
            return;
        }
        let mut failures = Vec::new();
        for f in &files {
            match check_file(f) {
                Ok(s) => eprintln!(
                    "ok {}: arch={} v{} tensors={} kv={} bytes={} ctx={:?} embd={:?} blocks={:?} heads={:?}/{:?} k/v={:?}/{:?}",
                    f.file_name().unwrap().to_string_lossy(),
                    s.architecture, s.version, s.tensor_count, s.kv_count, s.tensor_bytes,
                    s.context_length, s.embedding_length, s.block_count, s.head_count,
                    s.head_count_kv, s.key_length, s.value_length
                ),
                Err(e) => failures.push(format!("{}: {e}", f.display())),
            }
        }
        assert!(
            failures.is_empty(),
            "real GGUFs refused:\n{}",
            failures.join("\n")
        );
    }
}
