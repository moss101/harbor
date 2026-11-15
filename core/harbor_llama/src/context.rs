//! Safe wrapper around `llama_context`.

use std::fmt::{Debug, Formatter};
use std::num::NonZeroI32;
use std::ptr::NonNull;
use std::slice;

use crate::llama_batch::LlamaBatch;
use crate::model::LlamaModel;
use crate::{DecodeError, EmbeddingsError};

pub mod params;

/// Safe wrapper around `llama_context`.
#[allow(clippy::module_name_repetitions)]
pub struct LlamaContext<'a> {
    pub(crate) context: NonNull<harbor_llama_sys::llama_context>,
    /// a reference to the contexts model.
    pub model: &'a LlamaModel,
    initialized_logits: Vec<i32>,
    embeddings_enabled: bool,
}

impl Debug for LlamaContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaContext")
            .field("context", &self.context)
            .finish()
    }
}

impl<'model> LlamaContext<'model> {
    pub(crate) fn new(
        llama_model: &'model LlamaModel,
        llama_context: NonNull<harbor_llama_sys::llama_context>,
        embeddings_enabled: bool,
    ) -> Self {
        Self {
            context: llama_context,
            model: llama_model,
            initialized_logits: Vec::new(),
            embeddings_enabled,
        }
    }

    /// Gets the max number of logical tokens that can be submitted to decode.
    #[must_use]
    pub fn n_batch(&self) -> u32 {
        unsafe { harbor_llama_sys::llama_n_batch(self.context.as_ptr()) }
    }

    /// Gets the size of the context.
    #[must_use]
    pub fn n_ctx(&self) -> u32 {
        unsafe { harbor_llama_sys::llama_n_ctx(self.context.as_ptr()) }
    }

    /// Decodes the batch.
    ///
    /// # Errors
    ///
    /// - `DecodeError` if the decoding failed.
    pub fn decode(&mut self, batch: &mut LlamaBatch) -> Result<(), DecodeError> {
        let result =
            unsafe { harbor_llama_sys::llama_decode(self.context.as_ptr(), batch.llama_batch) };

        match NonZeroI32::new(result) {
            None => {
                self.initialized_logits
                    .clone_from(&batch.initialized_logits);
                Ok(())
            }
            Some(error) => Err(DecodeError::from(error)),
        }
    }

    /// Get the pooled embeddings for the `i`th sequence of the last decoded batch.
    ///
    /// The slice width is the pooling-derived output width: `n_cls_out` for
    /// RANK pooling, `n_embd_out` otherwise — NOT `n_embd`.
    ///
    /// # Errors
    ///
    /// - When the current context was constructed without enabling embeddings.
    /// - If the current model had a pooling type of [`harbor_llama_sys::LLAMA_POOLING_TYPE_NONE`]
    /// - If the given sequence index exceeds the max sequence id.
    pub fn embeddings_seq_ith(&self, i: i32) -> Result<&[f32], EmbeddingsError> {
        if !self.embeddings_enabled {
            return Err(EmbeddingsError::NotEnabled);
        }

        unsafe {
            let embedding = harbor_llama_sys::llama_get_embeddings_seq(self.context.as_ptr(), i);

            // Technically also possible whenever `i >= max(batch.n_seq)`, but can't check that here.
            if embedding.is_null() {
                Err(EmbeddingsError::NonePoolType)
            } else {
                Ok(slice::from_raw_parts(embedding, self.embeddings_out_len()))
            }
        }
    }

    /// The output width of an embeddings read, keyed on the context's LIVE
    /// pooling type rather than the model's `n_embd`.
    fn embeddings_out_len(&self) -> usize {
        let pooling = unsafe { harbor_llama_sys::llama_pooling_type(self.context.as_ptr()) };
        if pooling == harbor_llama_sys::LLAMA_POOLING_TYPE_RANK {
            usize::try_from(self.model.n_cls_out()).expect("n_cls_out does not fit into a usize")
        } else {
            usize::try_from(self.model.n_embd_out()).expect("n_embd_out does not fit into a usize")
        }
    }

    /// Get the logits for the ith token of the last decoded batch.
    ///
    /// # Panics
    ///
    /// - `i` is greater than `n_ctx`
    /// - `n_vocab` does not fit into a usize
    /// - logit `i` is not initialized.
    #[must_use]
    pub fn get_logits_ith(&self, i: i32) -> &[f32] {
        assert!(
            self.initialized_logits.contains(&i),
            "logit {i} is not initialized. only {:?} is",
            self.initialized_logits
        );
        assert!(
            self.n_ctx() > u32::try_from(i).expect("i does not fit into a u32"),
            "n_ctx ({}) must be greater than i ({})",
            self.n_ctx(),
            i
        );

        let data = unsafe { harbor_llama_sys::llama_get_logits_ith(self.context.as_ptr(), i) };
        let len = usize::try_from(self.model.n_vocab()).expect("n_vocab does not fit into a usize");

        unsafe { slice::from_raw_parts(data, len) }
    }
}

impl Drop for LlamaContext<'_> {
    fn drop(&mut self) {
        unsafe { harbor_llama_sys::llama_free(self.context.as_ptr()) }
    }
}
