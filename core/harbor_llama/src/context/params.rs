//! A safe wrapper around `llama_context_params`.
use std::num::NonZeroU32;

/// A rusty wrapper around `LLAMA_POOLING_TYPE`.
#[repr(i8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LlamaPoolingType {
    /// The pooling type is unspecified
    Unspecified = -1,
    /// No pooling
    None = 0,
    /// Mean pooling
    Mean = 1,
    /// CLS pooling
    Cls = 2,
    /// Last pooling
    Last = 3,
    /// Rank pooling
    Rank = 4,
}

/// Create a `LlamaPoolingType` from a `c_int` - returns `LlamaPoolingType::Unspecified` if
/// the value is not recognized.
impl From<i32> for LlamaPoolingType {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Mean,
            2 => Self::Cls,
            3 => Self::Last,
            4 => Self::Rank,
            _ => Self::Unspecified,
        }
    }
}

/// Create a `c_int` from a `LlamaPoolingType`.
impl From<LlamaPoolingType> for i32 {
    fn from(value: LlamaPoolingType) -> Self {
        match value {
            LlamaPoolingType::None => 0,
            LlamaPoolingType::Mean => 1,
            LlamaPoolingType::Cls => 2,
            LlamaPoolingType::Last => 3,
            LlamaPoolingType::Rank => 4,
            LlamaPoolingType::Unspecified => -1,
        }
    }
}

/// A safe wrapper around `llama_context_params`.
///
/// ```
/// # use std::num::NonZeroU32;
/// use harbor_llama::context::params::LlamaContextParams;
///
/// let ctx_params = LlamaContextParams::default()
///     .with_n_ctx(NonZeroU32::new(2048));
///
/// assert_eq!(ctx_params.n_ctx(), NonZeroU32::new(2048));
/// ```
#[derive(Debug, Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct LlamaContextParams {
    pub(crate) context_params: harbor_llama_sys::llama_context_params,
}

/// SAFETY: we do not allow setting or reading the pointers that cause this to not be automatically send or sync.
unsafe impl Send for LlamaContextParams {}
unsafe impl Sync for LlamaContextParams {}

/// Default parameters for `LlamaContext` (`llama_context_default_params`).
impl Default for LlamaContextParams {
    fn default() -> Self {
        let context_params = unsafe { harbor_llama_sys::llama_context_default_params() };
        Self { context_params }
    }
}

impl LlamaContextParams {
    /// Set the size of the context (`None` = the model's training context).
    #[must_use]
    pub fn with_n_ctx(mut self, n_ctx: Option<NonZeroU32>) -> Self {
        self.context_params.n_ctx = n_ctx.map_or(0, NonZeroU32::get);
        self
    }

    /// Get the size of the context; [`None`] if the model decides.
    #[must_use]
    pub fn n_ctx(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(self.context_params.n_ctx)
    }

    /// Set the max number of logical tokens per `decode` call.
    #[must_use]
    pub fn with_n_batch(mut self, n_batch: u32) -> Self {
        self.context_params.n_batch = n_batch;
        self
    }

    /// Get `n_batch`.
    #[must_use]
    pub fn n_batch(&self) -> u32 {
        self.context_params.n_batch
    }

    /// Set the physical (micro-)batch size. Non-causal embedding models need
    /// the whole input in one ubatch.
    #[must_use]
    pub fn with_n_ubatch(mut self, n_ubatch: u32) -> Self {
        self.context_params.n_ubatch = n_ubatch;
        self
    }

    /// Get `n_ubatch`.
    #[must_use]
    pub fn n_ubatch(&self) -> u32 {
        self.context_params.n_ubatch
    }

    /// Set the number of threads used for generation.
    #[must_use]
    pub fn with_n_threads(mut self, n_threads: i32) -> Self {
        self.context_params.n_threads = n_threads;
        self
    }

    /// Set the number of threads used for batch / prompt processing.
    #[must_use]
    pub fn with_n_threads_batch(mut self, n_threads: i32) -> Self {
        self.context_params.n_threads_batch = n_threads;
        self
    }

    /// Set the pooling type for embeddings.
    #[must_use]
    pub fn with_pooling_type(mut self, pooling_type: LlamaPoolingType) -> Self {
        self.context_params.pooling_type = i32::from(pooling_type);
        self
    }

    /// Get the pooling type.
    #[must_use]
    pub fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::from(self.context_params.pooling_type)
    }

    /// Enable embedding extraction.
    #[must_use]
    pub fn with_embeddings(mut self, embedding: bool) -> Self {
        self.context_params.embeddings = embedding;
        self
    }

    /// Whether embedding extraction is enabled.
    #[must_use]
    pub fn embeddings(&self) -> bool {
        self.context_params.embeddings
    }
}

#[cfg(test)]
mod tests {
    use super::{LlamaContextParams, LlamaPoolingType};
    use std::num::NonZeroU32;

    #[test]
    fn builders_round_trip() {
        let p = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(2048))
            .with_n_batch(512)
            .with_n_ubatch(256)
            .with_embeddings(true)
            .with_pooling_type(LlamaPoolingType::Mean);
        assert_eq!(p.n_ctx(), NonZeroU32::new(2048));
        assert_eq!(p.n_batch(), 512);
        assert_eq!(p.n_ubatch(), 256);
        assert!(p.embeddings());
        assert_eq!(p.pooling_type(), LlamaPoolingType::Mean);
    }
}
