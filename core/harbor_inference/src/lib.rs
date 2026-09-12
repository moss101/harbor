//! Harbor inference: ModelProvider v3 contracts.
//!
//! Authority: `03_Architecture_Contracts.md` §1,
//! `17_Model_Provider_and_Package_Contract.md`.
//!
//! Providers operate on explicit [`ModelRef`] variants
//! (InstalledPackage / SystemManaged / RemoteEndpoint), declare
//! capabilities (chat, tools, structured_output, embeddings, vision,
//! audio), and expose load/generate/embed with cancellation and cleanup.
//! Unsupported capability returns a typed error; router substitution
//! requires explicit policy and visible UI. The llama.cpp backend
//! implements this contract on top of `harbor_modelhub` installed
//! packages; platform providers implement it via their native adapters.

pub mod backend;
pub mod provider;
pub mod router;
#[cfg(feature = "gguf-backend")]
pub mod gguf;

pub use backend::TestBackend;
#[cfg(feature = "gguf-backend")]
pub use gguf::{GgufLlamaCppProvider, runtime_revision};
pub use provider::{
    Capabilities, ChatRequest, ChatResponse, GenerationHandle, ModelProvider, ModelRef,
    ProviderError, Usage,
};
pub use router::{RouterPolicy, Substitution};
