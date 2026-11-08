//! Safe wrapper over the pinned llama.cpp (`harbor_llama_sys`), trimmed to
//! the surface Harbor uses (decision 0022, phase 4).
//!
//! Adapted from `llama-cpp-2` 0.1.156 (MIT OR Apache-2.0, Dial AI); see
//! NOTICE and HARBOR.md for what was kept, removed and added.
//! `harbor_inference` depends on this crate under the dependency name
//! `llama-cpp-2`, so its `harbor_llama::...` paths are unchanged.

use std::ffi::{c_char, NulError};
#[cfg(feature = "common")]
use std::ffi::{CStr, CString};
use std::fmt::Debug;
use std::num::NonZeroI32;
use std::os::raw::c_int;
use std::path::PathBuf;
use std::string::FromUtf8Error;

use crate::llama_batch::BatchAddError;

pub mod context;
pub mod llama_backend;
pub mod llama_batch;
pub mod model;
#[cfg(feature = "mtmd")]
pub mod mtmd;
pub mod sampling;
pub mod token;

/// Upstream llama.cpp release tag this build links (from HARBOR_PIN.json).
pub const LLAMA_CPP_TAG: &str = harbor_llama_sys::LLAMA_CPP_TAG;
/// Full upstream llama.cpp commit this build links (from HARBOR_PIN.json).
pub const LLAMA_CPP_COMMIT: &str = harbor_llama_sys::LLAMA_CPP_COMMIT;
/// Runtime identity, `llama.cpp@<tag>+<commit[..10]>`, e.g.
/// `llama.cpp@v0.8.0+8642541b25`.
pub const RUNTIME_REVISION: &str = harbor_llama_sys::LLAMA_CPP_RUNTIME;

#[cfg(feature = "common")]
pub(crate) fn status_is_ok(status: harbor_llama_sys::llama_rs_status) -> bool {
    status == harbor_llama_sys::LLAMA_RS_STATUS_OK
}

/// A failable result from a llama.cpp function.
pub type Result<T> = std::result::Result<T, LlamaCppError>;

/// All errors that can occur in this crate.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum LlamaCppError {
    /// The backend was already initialized. This can generally be ignored as initializing the backend
    /// is idempotent.
    #[error("BackendAlreadyInitialized")]
    BackendAlreadyInitialized,
    /// There was an error while get the chat template from model.
    #[error("{0}")]
    ChatTemplateError(#[from] ChatTemplateError),
    /// There was an error while decoding a batch.
    #[error("{0}")]
    DecodeError(#[from] DecodeError),
    /// There was an error loading a model.
    #[error("{0}")]
    LlamaModelLoadError(#[from] LlamaModelLoadError),
    /// There was an error creating a new model context.
    #[error("{0}")]
    LlamaContextLoadError(#[from] LlamaContextLoadError),
    /// There was an error adding a token to a batch.
    #[error["{0}"]]
    BatchAddError(#[from] BatchAddError),
    /// see [`EmbeddingsError`]
    #[error(transparent)]
    EmbeddingError(#[from] EmbeddingsError),
    /// Backend device not found
    #[error("Backend device {0} not found")]
    BackendDeviceNotFound(usize),
    /// Max devices exceeded
    #[error("Max devices exceeded. Max devices is {0}")]
    MaxDevicesExceeded(usize),
    /// Failed to convert JSON schema to grammar.
    #[cfg(feature = "common")]
    #[error("JsonSchemaToGrammarError: {0}")]
    JsonSchemaToGrammarError(String),
}

/// There was an error while getting the chat template from a model.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum ChatTemplateError {
    /// gguf has no chat template (by that name)
    #[error("chat template not found - returned null pointer")]
    MissingTemplate,

    /// chat template contained a null byte
    #[error("null byte in string {0}")]
    NullError(#[from] NulError),

    /// The chat template was not valid utf8.
    #[error(transparent)]
    Utf8Error(#[from] std::str::Utf8Error),
}

/// Failed to Load context
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum LlamaContextLoadError {
    /// llama.cpp returned null
    #[error("null reference from llama.cpp")]
    NullReturn,
}

/// Failed to decode a batch.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum DecodeError {
    /// No kv cache slot was available.
    #[error("Decode Error 1: NoKvCacheSlot")]
    NoKvCacheSlot,
    /// The number of tokens in the batch was 0.
    #[error("Decode Error -1: n_tokens == 0")]
    NTokensZero,
    /// An unknown error occurred.
    #[error("Decode Error {0}: unknown")]
    Unknown(c_int),
}

/// When embedding related functions fail
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum EmbeddingsError {
    /// Embeddings weren't enabled in the context options
    #[error("Embeddings weren't enabled in the context options")]
    NotEnabled,
    /// Logits weren't enabled for the given token
    #[error("Logits were not enabled for the given token")]
    LogitsNotEnabled,
    /// The given sequence index exceeds the max sequence id
    #[error("Can't use sequence embeddings with a model supporting only LLAMA_POOLING_TYPE_NONE")]
    NonePoolType,
}

/// Errors that can occur when initializing a grammar sampler
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum GrammarError {
    /// The grammar root was not found in the grammar string
    #[error("Grammar root not found in grammar string")]
    RootNotFound,
    /// The grammar string or root contains null bytes
    #[error("Grammar string or root contains null bytes")]
    GrammarNullBytes,
    /// The grammar call returned null
    #[error("Grammar call returned null")]
    NullGrammar,
}

/// Decode a error from llama.cpp into a [`DecodeError`].
impl From<NonZeroI32> for DecodeError {
    fn from(value: NonZeroI32) -> Self {
        match value.get() {
            1 => DecodeError::NoKvCacheSlot,
            -1 => DecodeError::NTokensZero,
            i => DecodeError::Unknown(i),
        }
    }
}

/// An error that can occur when loading a model.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum LlamaModelLoadError {
    /// There was a null byte in a provided string and thus it could not be converted to a C string.
    #[error("null byte in string {0}")]
    NullError(#[from] NulError),
    /// llama.cpp returned a nullptr - this could be many different causes.
    #[error("null result from llama cpp")]
    NullResult,
    /// Failed to convert the path to a rust str. This means the path was not valid unicode
    #[error("failed to convert path {0} to str")]
    PathToStrError(PathBuf),
}

/// get the max number of devices according to llama.cpp
#[must_use]
pub fn max_devices() -> usize {
    unsafe { harbor_llama_sys::llama_max_devices() }
}

/// Convert a JSON schema string into a llama.cpp grammar string.
#[cfg(feature = "common")]
pub fn json_schema_to_grammar(schema_json: &str) -> Result<String> {
    let schema_cstr = CString::new(schema_json)
        .map_err(|err| LlamaCppError::JsonSchemaToGrammarError(err.to_string()))?;
    let mut out = std::ptr::null_mut();
    let rc = unsafe {
        harbor_llama_sys::llama_rs_json_schema_to_grammar(schema_cstr.as_ptr(), false, &mut out)
    };

    if !status_is_ok(rc) || out.is_null() {
        return Err(LlamaCppError::JsonSchemaToGrammarError(format!(
            "ffi error {rc}"
        )));
    }
    let grammar_bytes = unsafe { CStr::from_ptr(out) }.to_bytes().to_vec();
    unsafe { harbor_llama_sys::llama_rs_string_free(out) };
    String::from_utf8(grammar_bytes)
        .map_err(|err| LlamaCppError::JsonSchemaToGrammarError(err.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_revision_names_the_pin() {
        assert_eq!(
            super::RUNTIME_REVISION,
            format!(
                "llama.cpp@{}+{}",
                super::LLAMA_CPP_TAG,
                &super::LLAMA_CPP_COMMIT[..10]
            )
        );
        assert_eq!(super::LLAMA_CPP_COMMIT.len(), 40);
    }

    #[cfg(feature = "common")]
    #[test]
    fn json_schema_string_api_returns_grammar() {
        let schema = r#"{
            "type": "object",
            "properties": {
                "city": { "type": "string" },
                "unit": { "enum": ["c", "f"] }
            },
            "required": ["city"]
        }"#;

        let grammar = super::json_schema_to_grammar(schema)
            .expect("string-based schema conversion should succeed");

        assert!(grammar.contains("root ::="));
    }

    #[cfg(feature = "common")]
    #[test]
    fn json_schema_parse_error_is_an_error_not_a_crash() {
        assert!(super::json_schema_to_grammar("{not json").is_err());
    }
}

/// An error that can occur when converting a token to a string.
#[derive(Debug, thiserror::Error, Clone)]
#[non_exhaustive]
pub enum TokenToStringError {
    /// the token type was unknown
    #[error("Unknown Token Type")]
    UnknownTokenType,
    /// There was insufficient buffer space to convert the token to a string.
    #[error("Insufficient Buffer Space {0}")]
    InsufficientBufferSpace(c_int),
    /// The token was not valid utf8.
    #[error("FromUtf8Error {0}")]
    FromUtf8Error(#[from] FromUtf8Error),
}

/// Failed to convert a string to a token sequence.
#[derive(Debug, thiserror::Error)]
pub enum StringToTokenError {
    /// the string contained a null byte and thus could not be converted to a c string.
    #[error("{0}")]
    NulError(#[from] NulError),
    #[error("{0}")]
    /// Failed to convert a provided integer to a [`c_int`].
    CIntConversionError(#[from] std::num::TryFromIntError),
}

/// Failed to apply model chat template.
#[derive(Debug, thiserror::Error)]
pub enum NewLlamaChatMessageError {
    /// the string contained a null byte and thus could not be converted to a c string.
    #[error("{0}")]
    NulError(#[from] NulError),
}

/// Failed to apply model chat template.
#[derive(Debug, thiserror::Error)]
pub enum ApplyChatTemplateError {
    /// the string contained a null byte and thus could not be converted to a c string.
    #[error("{0}")]
    NulError(#[from] NulError),
    /// the string could not be converted to utf8.
    #[error("{0}")]
    FromUtf8Error(#[from] FromUtf8Error),
    /// llama.cpp returned a null pointer for the template result.
    #[error("null result from llama.cpp")]
    NullResult,
    /// llama.cpp returned an error code.
    #[error("ffi error {0}")]
    FfiError(i32),
}

/// Failed to accept a token in a sampler.
#[derive(Debug, thiserror::Error)]
pub enum SamplerAcceptError {
    /// llama.cpp returned an error code.
    #[error("ffi error {0}")]
    FfiError(i32),
}

/// Backend device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlamaBackendDeviceType {
    /// CPU device
    Cpu,
    /// ACCEL device
    Accelerator,
    /// GPU device
    Gpu,
    /// iGPU device
    IntegratedGpu,
    /// Unknown device type
    Unknown,
}

/// A ggml backend device
///
/// The index can be used with `LlamaModelParams::with_devices` to select specific devices.
#[derive(Debug, Clone)]
pub struct LlamaBackendDevice {
    /// The index of the device
    pub index: usize,
    /// The name of the device (e.g. "Vulkan0")
    pub name: String,
    /// A description of the device (e.g. "Adreno (TM) 740")
    pub description: String,
    /// The backend of the device (e.g. "Vulkan", "Metal", "CPU")
    pub backend: String,
    /// Total memory of the device in bytes
    pub memory_total: usize,
    /// Free memory of the device in bytes
    pub memory_free: usize,
    /// Device type
    pub device_type: LlamaBackendDeviceType,
}

/// List ggml backend devices
#[must_use]
pub fn list_llama_ggml_backend_devices() -> Vec<LlamaBackendDevice> {
    fn cstr_to_string(ptr: *const c_char) -> String {
        if ptr.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_string_lossy()
                .to_string()
        }
    }
    let mut devices = Vec::new();
    for i in 0..unsafe { harbor_llama_sys::ggml_backend_dev_count() } {
        let dev = unsafe { harbor_llama_sys::ggml_backend_dev_get(i) };
        let props = unsafe {
            let mut props = std::mem::zeroed();
            harbor_llama_sys::ggml_backend_dev_get_props(dev, &raw mut props);
            props
        };
        let name = cstr_to_string(props.name);
        let description = cstr_to_string(props.description);
        let backend = unsafe { harbor_llama_sys::ggml_backend_dev_backend_reg(dev) };
        let backend_name = unsafe { harbor_llama_sys::ggml_backend_reg_name(backend) };
        let backend = cstr_to_string(backend_name);
        let device_type = match props.type_ {
            harbor_llama_sys::GGML_BACKEND_DEVICE_TYPE_CPU => LlamaBackendDeviceType::Cpu,
            harbor_llama_sys::GGML_BACKEND_DEVICE_TYPE_ACCEL => LlamaBackendDeviceType::Accelerator,
            harbor_llama_sys::GGML_BACKEND_DEVICE_TYPE_GPU => LlamaBackendDeviceType::Gpu,
            harbor_llama_sys::GGML_BACKEND_DEVICE_TYPE_IGPU => {
                LlamaBackendDeviceType::IntegratedGpu
            }
            _ => LlamaBackendDeviceType::Unknown,
        };
        devices.push(LlamaBackendDevice {
            index: i,
            name,
            description,
            backend,
            memory_total: props.memory_total,
            memory_free: props.memory_free,
            device_type,
        });
    }
    devices
}
