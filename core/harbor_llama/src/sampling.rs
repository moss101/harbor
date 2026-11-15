//! Safe wrapper around `llama_sampler`.

use std::ffi::CString;
use std::fmt::{Debug, Formatter};

use crate::context::LlamaContext;
use crate::model::LlamaModel;
use crate::token::LlamaToken;
use crate::GrammarError;

/// A safe wrapper around `llama_sampler`.
pub struct LlamaSampler {
    pub(crate) sampler: *mut harbor_llama_sys::llama_sampler,
}

impl Debug for LlamaSampler {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaSamplerChain").finish()
    }
}

impl LlamaSampler {
    /// Sample and accept a token from the idx-th output of the last evaluation
    #[must_use]
    pub fn sample(&mut self, ctx: &LlamaContext, idx: i32) -> LlamaToken {
        let token = unsafe {
            harbor_llama_sys::llama_sampler_sample(self.sampler, ctx.context.as_ptr(), idx)
        };

        LlamaToken(token)
    }

    /// Combines a list of samplers into a single sampler chain that applies
    /// each component sampler one after another (perf counters on). The chain
    /// should end with [`LlamaSampler::greedy`] or [`LlamaSampler::dist`].
    #[must_use]
    pub fn chain_simple(samplers: impl IntoIterator<Item = Self>) -> Self {
        unsafe {
            let chain = harbor_llama_sys::llama_sampler_chain_init(
                harbor_llama_sys::llama_sampler_chain_params { no_perf: false },
            );

            for sampler in samplers {
                harbor_llama_sys::llama_sampler_chain_add(chain, sampler.sampler);

                // Do not call `llama_sampler_free` on the sampler, as the internal sampler is now
                // owned by the chain
                std::mem::forget(sampler);
            }

            Self { sampler: chain }
        }
    }

    /// Updates the logits l_i' = l_i/t. When t <= 0.0f, the maximum logit is kept at its original
    /// value, the rest are set to -inf
    #[must_use]
    pub fn temp(t: f32) -> Self {
        let sampler = unsafe { harbor_llama_sys::llama_sampler_init_temp(t) };
        Self { sampler }
    }

    /// Grammar (GBNF) constrained sampling.
    ///
    /// # Errors
    /// - the root rule is not in the grammar, or either string has a NUL byte
    /// - llama.cpp rejected the grammar (returned null / threw)
    pub fn grammar(
        model: &LlamaModel,
        grammar_str: &str,
        grammar_root: &str,
    ) -> Result<Self, GrammarError> {
        let (grammar_str, grammar_root) =
            Self::sanitize_grammar_strings(grammar_str, grammar_root)?;

        // The C shim catches C++ exceptions from the grammar parser.
        #[cfg(feature = "common")]
        let sampler = unsafe {
            harbor_llama_sys::llama_rs_sampler_init_grammar(
                model.vocab_ptr(),
                grammar_str.as_ptr(),
                grammar_root.as_ptr(),
            )
        };
        #[cfg(not(feature = "common"))]
        let sampler = unsafe {
            harbor_llama_sys::llama_sampler_init_grammar(
                model.vocab_ptr(),
                grammar_str.as_ptr(),
                grammar_root.as_ptr(),
            )
        };

        if sampler.is_null() {
            Err(GrammarError::NullGrammar)
        } else {
            Ok(Self { sampler })
        }
    }

    fn sanitize_grammar_strings(
        grammar_str: &str,
        grammar_root: &str,
    ) -> Result<(CString, CString), GrammarError> {
        if !grammar_str.contains(grammar_root) {
            return Err(GrammarError::RootNotFound);
        }
        let grammar_str = CString::new(grammar_str).map_err(|_| GrammarError::GrammarNullBytes)?;
        let grammar_root =
            CString::new(grammar_root).map_err(|_| GrammarError::GrammarNullBytes)?;
        Ok((grammar_str, grammar_root))
    }

    /// Selects a token at random based on each token's probabilities
    #[must_use]
    pub fn dist(seed: u32) -> Self {
        let sampler = unsafe { harbor_llama_sys::llama_sampler_init_dist(seed) };
        Self { sampler }
    }

    /// Selects the most likely token
    #[must_use]
    pub fn greedy() -> Self {
        let sampler = unsafe { harbor_llama_sys::llama_sampler_init_greedy() };
        Self { sampler }
    }
}

impl Drop for LlamaSampler {
    fn drop(&mut self) {
        unsafe {
            harbor_llama_sys::llama_sampler_free(self.sampler);
        }
    }
}
