//! Safe wrapper around `llama_batch`.

use crate::token::LlamaToken;
use harbor_llama_sys::{llama_batch, llama_batch_free, llama_batch_init, llama_pos, llama_seq_id};
use std::marker::PhantomData;

/// A safe wrapper around `llama_batch`.
#[derive(Debug)]
pub struct LlamaBatch<'a> {
    /// The number of tokens the batch was allocated with. they are safe to write to - but not necessarily read from as they are not necessarily initialized
    allocated: usize,
    /// The logits that are initialized. Used by [`LlamaContext`] to ensure that only initialized logits are accessed.
    pub(crate) initialized_logits: Vec<i32>,
    #[allow(clippy::doc_markdown)]
    /// The llama_cpp batch. always initialize by `harbor_llama_sys::llama_batch_init(allocated, <unknown>, <unknown>)`
    pub(crate) llama_batch: llama_batch,
    phantom: PhantomData<&'a [LlamaToken]>,
}

/// Errors that can occur when adding a token to a batch.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum BatchAddError {
    /// There was not enough space in the batch to add the token.
    #[error("Insufficient Space of {0}")]
    InsufficientSpace(usize),
}

impl<'a> LlamaBatch<'a> {
    /// add a token to the batch for sequences `seq_ids` at position `pos`. If `logits` is true, the
    /// token will be initialized and can be read from after the next decode.
    ///
    /// # Panics
    ///
    /// - [`self.llama_batch.n_tokens`] does not fit into a usize
    /// - [`seq_ids.len()`] does not fit into a [`llama_seq_id`]
    ///
    /// # Errors
    ///
    /// returns a error if there is insufficient space in the buffer
    pub fn add(
        &mut self,
        LlamaToken(id): LlamaToken,
        pos: llama_pos,
        seq_ids: &[i32],
        logits: bool,
    ) -> Result<(), BatchAddError> {
        if self.allocated
            < usize::try_from(self.n_tokens() + 1).expect("cannot fit n_tokens into a usize")
        {
            return Err(BatchAddError::InsufficientSpace(self.allocated));
        }
        let offset = self.llama_batch.n_tokens;
        let offset_usize = usize::try_from(offset).expect("cannot fit n_tokens into a usize");
        unsafe {
            // batch.token   [batch.n_tokens] = id;
            self.llama_batch.token.add(offset_usize).write(id);
            // batch.pos     [batch.n_tokens] = pos,
            self.llama_batch.pos.add(offset_usize).write(pos);
            // batch.n_seq_id[batch.n_tokens] = seq_ids.size();
            self.llama_batch.n_seq_id.add(offset_usize).write(
                llama_seq_id::try_from(seq_ids.len())
                    .expect("cannot fit seq_ids.len() into a llama_seq_id"),
            );
            // for (size_t i = 0; i < seq_ids.size(); ++i) {
            //     batch.seq_id[batch.n_tokens][i] = seq_ids[i];
            // }
            for (i, seq_id) in seq_ids.iter().enumerate() {
                let tmp = *self.llama_batch.seq_id.add(offset_usize);
                tmp.add(i).write(*seq_id);
            }
            // batch.logits  [batch.n_tokens] = logits;
            self.llama_batch
                .logits
                .add(offset_usize)
                .write(i8::from(logits));
        }

        if logits {
            self.initialized_logits.push(offset);
        } else {
            self.initialized_logits.retain(|l| l != &offset);
        }

        // batch.n_tokens++;
        self.llama_batch.n_tokens += 1;

        Ok(())
    }

    /// Create a new `LlamaBatch` that can contain up to `n_tokens` tokens.
    ///
    /// # Arguments
    ///
    /// - `n_tokens`: the maximum number of tokens that can be added to the batch
    /// - `n_seq_max`: the maximum number of sequences that can be added to the batch (generally 1 unless you know what you are doing)
    ///
    /// # Panics
    ///
    /// Panics if `n_tokens` is greater than `i32::MAX`.
    #[must_use]
    pub fn new(n_tokens: usize, n_seq_max: i32) -> Self {
        let n_tokens_i32 = i32::try_from(n_tokens).expect("cannot fit n_tokens into a i32");
        let batch = unsafe { llama_batch_init(n_tokens_i32, 0, n_seq_max) };

        LlamaBatch {
            allocated: n_tokens,
            initialized_logits: vec![],
            llama_batch: batch,
            phantom: PhantomData,
        }
    }

    /// HARBOR ADDITION (decision 0015): a batch of raw input EMBEDDINGS
    /// instead of token ids — `n_embd` floats per position. Needed to run
    /// text, image and audio positions through a non-causal embedding
    /// model in ONE forward pass.
    ///
    /// # Panics
    /// Panics if `n_tokens` or `n_embd` do not fit an `i32`.
    #[must_use]
    pub fn new_embd(n_tokens: usize, n_embd: usize, n_seq_max: i32) -> Self {
        let n_tokens_i32 = i32::try_from(n_tokens).expect("cannot fit n_tokens into a i32");
        let n_embd_i32 = i32::try_from(n_embd).expect("cannot fit n_embd into a i32");
        let batch = unsafe { llama_batch_init(n_tokens_i32, n_embd_i32, n_seq_max) };
        LlamaBatch {
            allocated: n_tokens,
            initialized_logits: vec![],
            llama_batch: batch,
            phantom: PhantomData,
        }
    }

    /// HARBOR ADDITION (decision 0015): append one embedding row (must be exactly the
    /// `n_embd` this batch was created with) at `pos` for `seq_id`.
    ///
    /// # Errors
    /// [`BatchAddError::InsufficientSpace`] when the batch is full.
    ///
    /// # Panics
    /// Panics if the batch was not created by [`Self::new_embd`], or on
    /// index conversion overflow.
    pub fn add_embd(
        &mut self,
        row: &[f32],
        n_embd: usize,
        pos: llama_pos,
        seq_id: i32,
        logits: bool,
    ) -> Result<(), BatchAddError> {
        assert!(
            !self.llama_batch.embd.is_null() && row.len() == n_embd,
            "add_embd needs an embd batch and an n_embd-sized row"
        );
        let offset = self.llama_batch.n_tokens;
        let offset_usize = usize::try_from(offset).expect("cannot fit n_tokens into a usize");
        if self.allocated <= offset_usize {
            return Err(BatchAddError::InsufficientSpace(self.allocated));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                row.as_ptr(),
                self.llama_batch.embd.add(offset_usize * n_embd),
                n_embd,
            );
            self.llama_batch.pos.add(offset_usize).write(pos);
            self.llama_batch.n_seq_id.add(offset_usize).write(1);
            (*self.llama_batch.seq_id.add(offset_usize)).write(seq_id);
            self.llama_batch
                .logits
                .add(offset_usize)
                .write(i8::from(logits));
        }
        if logits {
            self.initialized_logits.push(offset);
        }
        self.llama_batch.n_tokens += 1;
        Ok(())
    }

    /// Returns the number of tokens in the batch.
    #[must_use]
    pub fn n_tokens(&self) -> i32 {
        self.llama_batch.n_tokens
    }
}

impl<'a> Drop for LlamaBatch<'a> {
    /// Drops the `LlamaBatch`.
    ///
    /// ```
    /// # use harbor_llama::llama_batch::LlamaBatch;
    /// # use std::error::Error;
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// let batch = LlamaBatch::new(512, 1);
    /// // frees the memory associated with the batch. (allocated by llama.cpp)
    /// drop(batch);
    /// # Ok(())
    /// # }
    fn drop(&mut self) {
        unsafe {
            llama_batch_free(self.llama_batch);
        }
    }
}
