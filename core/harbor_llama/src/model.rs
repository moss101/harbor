//! A safe wrapper around `llama_model`.
use std::ffi::{c_char, CStr, CString};
use std::num::NonZeroU16;
use std::os::raw::c_int;
use std::path::Path;
use std::ptr::NonNull;
use std::str::Utf8Error;

use crate::context::params::LlamaContextParams;
use crate::context::LlamaContext;
use crate::llama_backend::LlamaBackend;
use crate::model::params::LlamaModelParams;
use crate::token::LlamaToken;
use crate::{
    ApplyChatTemplateError, ChatTemplateError, LlamaContextLoadError, LlamaModelLoadError,
    NewLlamaChatMessageError, StringToTokenError, TokenToStringError,
};

pub mod params;

/// A safe wrapper around `llama_model`.
#[derive(Debug)]
#[repr(transparent)]
#[allow(clippy::module_name_repetitions)]
pub struct LlamaModel {
    pub(crate) model: NonNull<harbor_llama_sys::llama_model>,
}

/// A chat template from [`LlamaModel::chat_template`], fed into
/// [`LlamaModel::apply_chat_template`]. Stored as a `CString` to avoid
/// round-trip conversions within the FFI.
#[derive(Eq, PartialEq, Clone, PartialOrd, Ord, Hash)]
pub struct LlamaChatTemplate(CString);

impl LlamaChatTemplate {
    /// Create a new template from a string: either the name of a llama.cpp
    /// built-in chat template (e.g. "chatml") or a Jinja template.
    pub fn new(template: &str) -> Result<Self, std::ffi::NulError> {
        Ok(Self(CString::new(template)?))
    }

    /// Accesses the template as a c string reference.
    pub fn as_c_str(&self) -> &CStr {
        &self.0
    }

    /// Attempts to convert the `CString` into a Rust str reference.
    pub fn to_str(&self) -> Result<&str, Utf8Error> {
        self.0.to_str()
    }
}

impl std::fmt::Debug for LlamaChatTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A Safe wrapper around `llama_chat_message`
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct LlamaChatMessage {
    role: CString,
    content: CString,
}

impl LlamaChatMessage {
    /// Create a new `LlamaChatMessage`
    ///
    /// # Errors
    /// If either of ``role`` or ``content`` contain null bytes.
    pub fn new(role: String, content: String) -> Result<Self, NewLlamaChatMessageError> {
        Ok(Self {
            role: CString::new(role)?,
            content: CString::new(content)?,
        })
    }
}

/// How to determine if we should prepend a bos token to tokens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddBos {
    /// Add the beginning of stream token to the start of the string.
    Always,
    /// Do not add the beginning of stream token to the start of the string.
    Never,
}

unsafe impl Send for LlamaModel {}

unsafe impl Sync for LlamaModel {}

impl LlamaModel {
    pub(crate) fn vocab_ptr(&self) -> *const harbor_llama_sys::llama_vocab {
        unsafe { harbor_llama_sys::llama_model_get_vocab(self.model.as_ptr()) }
    }

    /// get the number of tokens the model was trained on
    ///
    /// # Panics
    ///
    /// If the value does not fit into an `u32` (llama.cpp returns a positive `int32_t`).
    #[must_use]
    pub fn n_ctx_train(&self) -> u32 {
        let n_ctx_train = unsafe { harbor_llama_sys::llama_model_n_ctx_train(self.model.as_ptr()) };
        u32::try_from(n_ctx_train).expect("n_ctx_train fits into an u32")
    }

    /// Get the end of stream token.
    #[must_use]
    pub fn token_eos(&self) -> LlamaToken {
        let token = unsafe { harbor_llama_sys::llama_vocab_eos(self.vocab_ptr()) };
        LlamaToken(token)
    }

    /// Check if a token represents the end of generation (end of turn, end of sequence, etc.)
    #[must_use]
    pub fn is_eog_token(&self, token: LlamaToken) -> bool {
        unsafe { harbor_llama_sys::llama_vocab_is_eog(self.vocab_ptr(), token.0) }
    }

    /// Convert a string to a Vector of tokens (special tokens are parsed).
    ///
    /// # Errors
    ///
    /// - if [`str`] contains a null byte.
    ///
    /// # Panics
    ///
    /// - if there is more than [`usize::MAX`] [`LlamaToken`]s in [`str`].
    pub fn str_to_token(
        &self,
        str: &str,
        add_bos: AddBos,
    ) -> Result<Vec<LlamaToken>, StringToTokenError> {
        let add_bos = match add_bos {
            AddBos::Always => true,
            AddBos::Never => false,
        };

        let tokens_estimation = std::cmp::max(8, (str.len() / 2) + usize::from(add_bos));
        let mut buffer: Vec<LlamaToken> = Vec::with_capacity(tokens_estimation);

        let c_string = CString::new(str)?;
        let buffer_capacity =
            c_int::try_from(buffer.capacity()).expect("buffer capacity should fit into a c_int");

        let size = unsafe {
            harbor_llama_sys::llama_tokenize(
                self.vocab_ptr(),
                c_string.as_ptr(),
                c_int::try_from(c_string.as_bytes().len())?,
                buffer.as_mut_ptr().cast::<harbor_llama_sys::llama_token>(),
                buffer_capacity,
                add_bos,
                true,
            )
        };

        // if we fail the first time we can resize the vector to the correct size and try again.
        let size = if size.is_negative() {
            buffer.reserve_exact(usize::try_from(-size).expect("usize's are larger "));
            unsafe {
                harbor_llama_sys::llama_tokenize(
                    self.vocab_ptr(),
                    c_string.as_ptr(),
                    c_int::try_from(c_string.as_bytes().len())?,
                    buffer.as_mut_ptr().cast::<harbor_llama_sys::llama_token>(),
                    -size,
                    add_bos,
                    true,
                )
            }
        } else {
            size
        };

        let size = usize::try_from(size).expect("size is positive and usize ");

        // Safety: `size` < `capacity` and llama-cpp has initialized elements up to `size`
        unsafe { buffer.set_len(size) }
        Ok(buffer)
    }

    /// Convert a token to a string (`llama_token_to_piece`), decoding through
    /// a stateful UTF-8 decoder so multi-byte characters split across tokens
    /// survive. Invalid bytes become U+FFFD.
    ///
    /// # Errors
    ///
    /// - if the token type is unknown
    pub fn token_to_piece(
        &self,
        token: LlamaToken,
        decoder: &mut encoding_rs::Decoder,
        special: bool,
        lstrip: Option<NonZeroU16>,
    ) -> Result<String, TokenToStringError> {
        let bytes = match self.token_to_piece_bytes(token, 8, special, lstrip) {
            // when there is insufficient space `token_to_piece` returns the negated required size
            Err(TokenToStringError::InsufficientBufferSpace(i)) => self.token_to_piece_bytes(
                token,
                (-i).try_into().expect("Error buffer size is positive"),
                special,
                lstrip,
            ),
            x => x,
        }?;
        Ok(decode_piece(decoder, &bytes))
    }

    /// Raw token decoding to bytes (`llama_token_to_piece`).
    ///
    /// # Errors
    ///
    /// - if the token type is unknown
    /// - the resultant token is larger than `buffer_size`.
    pub fn token_to_piece_bytes(
        &self,
        token: LlamaToken,
        buffer_size: usize,
        special: bool,
        lstrip: Option<NonZeroU16>,
    ) -> Result<Vec<u8>, TokenToStringError> {
        let string = CString::new(vec![b'*'; buffer_size]).expect("no null");
        let len = string.as_bytes().len();
        let len = c_int::try_from(len).expect("length fits into c_int");
        let buf = string.into_raw();
        let lstrip = lstrip.map_or(0, |it| i32::from(it.get()));
        let size = unsafe {
            harbor_llama_sys::llama_token_to_piece(
                self.vocab_ptr(),
                token.0,
                buf,
                len,
                lstrip,
                special,
            )
        };
        // Reclaim the buffer on every path (the original leaked it on error).
        let string = unsafe { CString::from_raw(buf) };

        match size {
            0 => Err(TokenToStringError::UnknownTokenType),
            i if i.is_negative() => Err(TokenToStringError::InsufficientBufferSpace(i)),
            size => {
                let mut bytes = string.into_bytes();
                let len = usize::try_from(size).expect("size is positive and fits into usize");
                bytes.truncate(len);
                Ok(bytes)
            }
        }
    }

    /// The number of tokens in the model's vocabulary.
    #[must_use]
    pub fn n_vocab(&self) -> i32 {
        unsafe { harbor_llama_sys::llama_vocab_n_tokens(self.vocab_ptr()) }
    }

    /// The model's hidden (input embedding) width.
    #[must_use]
    pub fn n_embd(&self) -> c_int {
        unsafe { harbor_llama_sys::llama_model_n_embd(self.model.as_ptr()) }
    }

    /// The model's *output* embedding width (`n_embd_out`): the width
    /// llama.cpp extracts embeddings at (diverges from `n_embd` when
    /// `{arch}.embedding_length_out` is present).
    #[must_use]
    pub fn n_embd_out(&self) -> c_int {
        unsafe { harbor_llama_sys::llama_model_n_embd_out(self.model.as_ptr()) }
    }

    /// The model's classification output width (`n_cls_out`, default 1) — the
    /// width of a RANK-pooled embeddings read.
    #[must_use]
    pub fn n_cls_out(&self) -> u32 {
        unsafe { harbor_llama_sys::llama_model_n_cls_out(self.model.as_ptr()) }
    }

    /// Get chat template from model by name. If the name parameter is None, the default chat
    /// template will be returned.
    ///
    /// # Errors
    ///
    /// * If the model has no chat template by that name
    /// * If the chat template is not a valid [`CString`].
    pub fn chat_template(
        &self,
        name: Option<&str>,
    ) -> Result<LlamaChatTemplate, ChatTemplateError> {
        let name_cstr = name.map(CString::new);
        let name_ptr = match name_cstr {
            Some(Ok(ref name)) => name.as_ptr(),
            _ => std::ptr::null(),
        };
        let result =
            unsafe { harbor_llama_sys::llama_model_chat_template(self.model.as_ptr(), name_ptr) };

        if result.is_null() {
            Err(ChatTemplateError::MissingTemplate)
        } else {
            let chat_template_cstr = unsafe { CStr::from_ptr(result) };
            let chat_template = CString::new(chat_template_cstr.to_bytes())?;
            Ok(LlamaChatTemplate(chat_template))
        }
    }

    /// Loads a model from a file.
    ///
    /// # Errors
    ///
    /// See [`LlamaModelLoadError`] for more information.
    pub fn load_from_file(
        _: &LlamaBackend,
        path: impl AsRef<Path>,
        params: &LlamaModelParams,
    ) -> Result<Self, LlamaModelLoadError> {
        let path = path.as_ref();
        debug_assert!(Path::new(path).exists(), "{path:?} does not exist");
        let path = path
            .to_str()
            .ok_or(LlamaModelLoadError::PathToStrError(path.to_path_buf()))?;

        let cstr = CString::new(path)?;
        let llama_model =
            unsafe { harbor_llama_sys::llama_model_load_from_file(cstr.as_ptr(), params.params) };

        let model = NonNull::new(llama_model).ok_or(LlamaModelLoadError::NullResult)?;

        tracing::debug!(?path, "Loaded model");
        Ok(LlamaModel { model })
    }

    /// Create a new context from this model.
    ///
    /// # Errors
    ///
    /// There is many ways this can fail. See [`LlamaContextLoadError`] for more information.
    #[allow(clippy::needless_pass_by_value)]
    pub fn new_context<'a>(
        &'a self,
        _: &LlamaBackend,
        params: LlamaContextParams,
    ) -> Result<LlamaContext<'a>, LlamaContextLoadError> {
        let context_params = params.context_params;
        let context =
            unsafe { harbor_llama_sys::llama_init_from_model(self.model.as_ptr(), context_params) };
        let context = NonNull::new(context).ok_or(LlamaContextLoadError::NullReturn)?;

        Ok(LlamaContext::new(self, context, params.embeddings()))
    }

    /// Apply a chat template to some messages (`llama_chat_apply_template`).
    /// Use [`Self::chat_template`] to retrieve the template baked into the model.
    ///
    /// # Errors
    /// There are many ways this can fail. See [`ApplyChatTemplateError`] for more information.
    pub fn apply_chat_template(
        &self,
        tmpl: &LlamaChatTemplate,
        chat: &[LlamaChatMessage],
        add_ass: bool,
    ) -> Result<String, ApplyChatTemplateError> {
        // Buffer is twice the length of messages per their recommendation
        let message_length = chat.iter().fold(0, |acc, c| {
            acc + c.role.to_bytes().len() + c.content.to_bytes().len()
        });
        let mut buff: Vec<u8> = vec![0; message_length * 2];

        let chat: Vec<harbor_llama_sys::llama_chat_message> = chat
            .iter()
            .map(|c| harbor_llama_sys::llama_chat_message {
                role: c.role.as_ptr(),
                content: c.content.as_ptr(),
            })
            .collect();

        let tmpl_ptr = tmpl.0.as_ptr();

        let res = unsafe {
            harbor_llama_sys::llama_chat_apply_template(
                tmpl_ptr,
                chat.as_ptr(),
                chat.len(),
                add_ass,
                buff.as_mut_ptr().cast::<c_char>(),
                buff.len().try_into().expect("Buffer size exceeds i32::MAX"),
            )
        };

        if res < 0 {
            return Err(ApplyChatTemplateError::FfiError(res));
        }

        if res > buff.len().try_into().expect("Buffer size exceeds i32::MAX") {
            buff.resize(res.try_into().expect("res is negative"), 0);

            let res = unsafe {
                harbor_llama_sys::llama_chat_apply_template(
                    tmpl_ptr,
                    chat.as_ptr(),
                    chat.len(),
                    add_ass,
                    buff.as_mut_ptr().cast::<c_char>(),
                    buff.len().try_into().expect("Buffer size exceeds i32::MAX"),
                )
            };
            if res < 0 {
                return Err(ApplyChatTemplateError::FfiError(res));
            }
            assert_eq!(Ok(res), buff.len().try_into());
        }
        buff.truncate(res.try_into().expect("res is negative"));
        Ok(String::from_utf8(buff)?)
    }
}

impl Drop for LlamaModel {
    fn drop(&mut self) {
        unsafe { harbor_llama_sys::llama_model_free(self.model.as_ptr()) }
    }
}

fn decode_piece(decoder: &mut encoding_rs::Decoder, bytes: &[u8]) -> String {
    // `decode_to_string` never grows its destination. The decoder's bound also accounts
    // for an incomplete UTF-8 sequence retained from the previous token.
    let mut output = String::with_capacity(
        decoder
            .max_utf8_buffer_length(bytes.len())
            .expect("token output is too large to decode"),
    );
    let (result, read, _) = decoder.decode_to_string(bytes, &mut output, false);
    assert!(
        matches!(result, encoding_rs::CoderResult::InputEmpty) && read == bytes.len(),
        "UTF-8 decoder capacity bound must consume the complete token"
    );
    output
}

#[cfg(test)]
mod tests {
    use super::decode_piece;

    #[test]
    fn token_decoder_preserves_utf8_split_across_pieces() {
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        assert_eq!(decode_piece(&mut decoder, &[0xE5, 0x9B]), "");
        assert_eq!(decode_piece(&mut decoder, &[0xB2]), "\u{56F2}");
    }
}
