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
pub mod cassette;
#[cfg(feature = "gguf-backend")]
pub mod gguf;
pub mod provider;
pub mod router;

pub use backend::TestBackend;
pub use cassette::{Cassette, CassetteMode, RecordReplayProvider};
#[cfg(feature = "gguf-backend")]
pub use gguf::{runtime_revision, GgufLlamaCppProvider};
pub use provider::{
    Capabilities, ChatRequest, ChatResponse, GenerationHandle, ModelProvider, ModelRef,
    ProviderError, Usage,
};
pub use router::{RouterPolicy, Substitution};

/// The inference runtime identity this build links, feature-agnostic so
/// eval reports can always record it.
pub fn runtime_identity() -> &'static str {
    #[cfg(feature = "gguf-backend")]
    {
        gguf::runtime_revision()
    }
    #[cfg(not(feature = "gguf-backend"))]
    {
        "llama.cpp/not-linked"
    }
}
