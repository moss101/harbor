//! Provider contracts: ModelRef, capabilities, cancellable generation.

use harbor_canonical::JsonValue;

#[derive(Debug, Clone, PartialEq)]
pub enum ModelRef {
    /// A validated installed package (harbor_modelhub).
    InstalledPackage { package_id: String },
    /// A platform system model (Apple system model, Windows model...).
    SystemManaged {
        provider_id: String,
        model_id: String,
    },
    /// An explicit user-configured remote endpoint.
    RemoteEndpoint {
        profile_id: String,
        endpoint: String,
        model: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capabilities {
    Chat,
    Tools,
    StructuredOutput,
    Embeddings,
    Vision,
    Audio,
}

impl Capabilities {
    pub fn as_str(&self) -> &'static str {
        match self {
            Capabilities::Chat => "chat",
            Capabilities::Tools => "tools",
            Capabilities::StructuredOutput => "structured_output",
            Capabilities::Embeddings => "embeddings",
            Capabilities::Vision => "vision",
            Capabilities::Audio => "audio",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: ModelRef,
    /// Chat messages as canonical JSON array
    /// ([{"role": "...", "content": "..."}]).
    pub messages: Vec<JsonValue>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Requested capabilities (checked against provider declarations).
    pub requires: Vec<Capabilities>,
    /// JSON Schema the response must satisfy. Providers declaring
    /// `StructuredOutput` constrain decoding to it (grammar); others must
    /// refuse when the caller requires `StructuredOutput`. The caller
    /// still validates the parsed response — the schema is a constraint on
    /// generation, never a substitute for validation below the model.
    pub response_schema: Option<JsonValue>,
    /// Opaque caller key for tracing and record/replay (e.g.
    /// `graph_id/node_id#iteration`). Real providers ignore it; the
    /// cassette provider matches on it so hand-authored fixtures survive
    /// instruction edits.
    pub trace_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub content: String,
    pub usage: Usage,
    /// The exact model that executed (Trust Pulse / Model Dock surface).
    pub executed_on: String,
    /// Where execution happened; a LocalOnly run that executed remote is a
    /// contract violation the router must never produce.
    pub execution_location: harbor_security::policy::ExecutionLocation,
}

/// Streaming generation handle with cooperative cancellation.
pub struct GenerationHandle {
    pub cancel: Box<dyn Fn() + Send + Sync>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("capability '{0}' not supported by this provider")]
    UnsupportedCapability(&'static str),
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("cancelled")]
    Cancelled,
    #[error("backend failure: {0}")]
    Backend(String),
    #[error("policy violation: {0}")]
    Policy(String),
}

/// A model provider. `capabilities` is the declaration; every call checks
/// it and returns a typed error when unsupported. Load/generate cleanup
/// happens on drop and via explicit unload.
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> &'static [Capabilities];
    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool;
    fn load(&self, model: &ModelRef) -> Result<(), ProviderError>;
    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError>;
    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;
    /// Embed a document chunk (index identity binds the model identity).
    fn embed(&self, model: &ModelRef, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        let _ = (model, texts);
        Err(ProviderError::UnsupportedCapability("embeddings"))
    }
    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation;
}
