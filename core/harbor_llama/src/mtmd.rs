//! Safe wrapper around llama.cpp's multimodal library (`mtmd`): image and
//! audio projectors feeding a text model.
use std::ffi::{CStr, CString};
use std::ptr::NonNull;
use std::slice;

use crate::context::LlamaContext;
use crate::model::LlamaModel;
use crate::token::LlamaToken;

/// Input chunk types for multimodal data
///
/// ```
/// use harbor_llama::mtmd::MtmdInputChunkType;
/// assert_eq!(MtmdInputChunkType::Text, harbor_llama_sys::MTMD_INPUT_CHUNK_TYPE_TEXT.into());
/// assert_ne!(MtmdInputChunkType::Text, MtmdInputChunkType::Image);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MtmdInputChunkType {
    /// Text input chunk
    Text = harbor_llama_sys::MTMD_INPUT_CHUNK_TYPE_TEXT as _,
    /// Image input chunk
    Image = harbor_llama_sys::MTMD_INPUT_CHUNK_TYPE_IMAGE as _,
    /// Audio input chunk
    Audio = harbor_llama_sys::MTMD_INPUT_CHUNK_TYPE_AUDIO as _,
}

impl From<harbor_llama_sys::mtmd_input_chunk_type> for MtmdInputChunkType {
    fn from(chunk_type: harbor_llama_sys::mtmd_input_chunk_type) -> Self {
        match chunk_type {
            harbor_llama_sys::MTMD_INPUT_CHUNK_TYPE_TEXT => MtmdInputChunkType::Text,
            harbor_llama_sys::MTMD_INPUT_CHUNK_TYPE_IMAGE => MtmdInputChunkType::Image,
            harbor_llama_sys::MTMD_INPUT_CHUNK_TYPE_AUDIO => MtmdInputChunkType::Audio,
            _ => panic!("Unknown MTMD input chunk type: {chunk_type}"),
        }
    }
}

/// Configuration parameters for an MTMD context. Fields not listed here
/// keep llama.cpp's `mtmd_context_params_default()` values.
#[derive(Debug, Clone)]
pub struct MtmdContextParams {
    /// Whether to use GPU acceleration
    pub use_gpu: bool,
    /// Whether to print timing information
    pub print_timings: bool,
    /// Number of threads to use for processing
    pub n_threads: i32,
    /// Media marker string used to identify media positions in text
    pub media_marker: CString,
    /// Minimum number of tokens used to represent an image (-1 = model default).
    pub image_min_tokens: i32,
    /// Maximum number of tokens used to represent an image (-1 = model default).
    pub image_max_tokens: i32,
}

impl Default for MtmdContextParams {
    fn default() -> Self {
        let params = unsafe { harbor_llama_sys::mtmd_context_params_default() };
        Self {
            use_gpu: params.use_gpu,
            print_timings: params.print_timings,
            n_threads: params.n_threads,
            media_marker: unsafe { CStr::from_ptr(params.media_marker) }.to_owned(),
            image_min_tokens: params.image_min_tokens,
            image_max_tokens: params.image_max_tokens,
        }
    }
}

impl MtmdContextParams {
    /// The C params; borrows `media_marker`, so `self` must outlive the result's use.
    fn to_c(&self) -> harbor_llama_sys::mtmd_context_params {
        let mut context = unsafe { harbor_llama_sys::mtmd_context_params_default() };
        context.use_gpu = self.use_gpu;
        context.print_timings = self.print_timings;
        context.n_threads = self.n_threads;
        context.media_marker = self.media_marker.as_ptr();
        context.image_min_tokens = self.image_min_tokens;
        context.image_max_tokens = self.image_max_tokens;
        context
    }
}

/// Text input for [`MtmdContext::tokenize`].
#[derive(Debug, Clone)]
pub struct MtmdInputText {
    /// The input text string
    pub text: String,
    /// Whether to add special tokens
    pub add_special: bool,
    /// Whether to parse special tokens
    pub parse_special: bool,
}

/// Safe wrapper around `mtmd_context`.
#[derive(Debug)]
pub struct MtmdContext {
    pub(crate) context: NonNull<harbor_llama_sys::mtmd_context>,
}

// MtmdContext is thread safe
unsafe impl Send for MtmdContext {}
unsafe impl Sync for MtmdContext {}

impl MtmdContext {
    /// Initialize an MTMD context from a multimodal projector (mmproj) file.
    ///
    /// # Errors
    ///
    /// - The path cannot be converted to a C string
    /// - llama.cpp returned null (initialization failure)
    pub fn init_from_file(
        mmproj_path: &str,
        text_model: &LlamaModel,
        params: &MtmdContextParams,
    ) -> Result<Self, MtmdInitError> {
        let path_cstr = CString::new(mmproj_path)?;
        let ctx_params = params.to_c();

        let context = unsafe {
            harbor_llama_sys::mtmd_init_from_file(
                path_cstr.as_ptr(),
                text_model.model.as_ptr(),
                ctx_params,
            )
        };

        let context = NonNull::new(context).ok_or(MtmdInitError::NullResult)?;
        Ok(Self { context })
    }

    /// Check whether the current model supports vision input.
    #[must_use]
    pub fn support_vision(&self) -> bool {
        unsafe { harbor_llama_sys::mtmd_support_vision(self.context.as_ptr()) }
    }

    /// Check whether the current model supports audio input.
    #[must_use]
    pub fn support_audio(&self) -> bool {
        unsafe { harbor_llama_sys::mtmd_support_audio(self.context.as_ptr()) }
    }

    /// Get audio sample rate in Hz (e.g., 16000 for Whisper).
    /// Returns None if audio is not supported.
    #[must_use]
    pub fn get_audio_sample_rate(&self) -> Option<u32> {
        let rate = unsafe { harbor_llama_sys::mtmd_get_audio_sample_rate(self.context.as_ptr()) };
        (rate > 0).then_some(rate.unsigned_abs())
    }

    /// Tokenize text and bitmaps into chunks. The text must contain one media
    /// marker (default: [`mtmd_default_marker`]) per bitmap.
    ///
    /// # Errors
    ///
    /// * `BitmapCountMismatch` - Number of bitmaps doesn't match number of markers
    /// * `ImagePreprocessingError` - Error occurred during image preprocessing
    /// * `UnknownError` - Other tokenization error occurred
    pub fn tokenize(
        &self,
        text: MtmdInputText,
        bitmaps: &[&MtmdBitmap],
    ) -> Result<MtmdInputChunks, MtmdTokenizeError> {
        let chunks = MtmdInputChunks::new();
        let text_cstring = CString::new(text.text)?;
        let input_text = harbor_llama_sys::mtmd_input_text {
            text: text_cstring.as_ptr(),
            text_len: text_cstring.as_bytes().len(),
            add_special: text.add_special,
            parse_special: text.parse_special,
        };

        let bitmap_ptrs: Vec<*const harbor_llama_sys::mtmd_bitmap> = bitmaps
            .iter()
            .map(|b| b.bitmap.as_ptr().cast_const())
            .collect();

        let result = unsafe {
            harbor_llama_sys::mtmd_tokenize(
                self.context.as_ptr(),
                chunks.chunks.as_ptr(),
                &raw const input_text,
                bitmap_ptrs.as_ptr().cast_mut(),
                bitmaps.len(),
            )
        };

        match result {
            0 => Ok(chunks),
            1 => Err(MtmdTokenizeError::BitmapCountMismatch),
            2 => Err(MtmdTokenizeError::ImagePreprocessingError),
            _ => Err(MtmdTokenizeError::UnknownError(result)),
        }
    }

    /// HARBOR ADDITION (decision 0015): the embeddings produced by the last
    /// [`encode_chunk`](Self::encode_chunk), `n_floats` long
    /// (`chunk.n_tokens() * n_embd`). Borrowed from the context: copy
    /// before the next encode.
    ///
    /// # Safety
    /// `n_floats` must not exceed what the encoded chunk produced.
    #[must_use]
    pub unsafe fn output_embeddings(&self, n_floats: usize) -> &[f32] {
        let ptr = harbor_llama_sys::mtmd_get_output_embd(self.context.as_ptr());
        if ptr.is_null() {
            return &[];
        }
        std::slice::from_raw_parts(ptr, n_floats)
    }

    /// Encode an image or audio chunk into embeddings (read them with
    /// [`output_embeddings`](Self::output_embeddings)).
    ///
    /// # Errors
    ///
    /// Returns `MtmdEncodeError::EncodeFailure` if encoding fails.
    pub fn encode_chunk(&self, chunk: &MtmdInputChunk) -> Result<(), MtmdEncodeError> {
        let result = unsafe {
            harbor_llama_sys::mtmd_encode_chunk(self.context.as_ptr(), chunk.chunk.as_ptr())
        };

        if result == 0 {
            Ok(())
        } else {
            Err(MtmdEncodeError::EncodeFailure(result))
        }
    }
}

impl Drop for MtmdContext {
    fn drop(&mut self) {
        unsafe { harbor_llama_sys::mtmd_free(self.context.as_ptr()) }
    }
}

/// Safe wrapper around `mtmd_bitmap`: RGB image pixels or PCM F32 audio.
///
/// Not `Clone`: it owns the C bitmap (the upstream wrapper derived `Clone`,
/// which would double-free).
#[derive(Debug)]
pub struct MtmdBitmap {
    pub(crate) bitmap: NonNull<harbor_llama_sys::mtmd_bitmap>,
}

// MtmdBitmap is thread safe
unsafe impl Send for MtmdBitmap {}
unsafe impl Sync for MtmdBitmap {}

impl MtmdBitmap {
    /// Create a bitmap from RGB image data (`nx * ny * 3` bytes).
    ///
    /// # Errors
    ///
    /// * `InvalidDataSize` - Data length doesn't match `nx * ny * 3`
    /// * `NullResult` - Underlying C function returned null
    ///
    /// ```
    /// use harbor_llama::mtmd::MtmdBitmap;
    /// let image_data = [255u8, 0, 0].repeat(4); // 2x2 red
    /// assert!(MtmdBitmap::from_image_data(2, 2, &image_data).is_ok());
    /// assert!(MtmdBitmap::from_image_data(3, 2, &image_data).is_err());
    /// ```
    pub fn from_image_data(nx: u32, ny: u32, data: &[u8]) -> Result<Self, MtmdBitmapError> {
        let expected = (nx as usize)
            .checked_mul(ny as usize)
            .and_then(|n| n.checked_mul(3))
            .ok_or(MtmdBitmapError::InvalidDataSize)?;
        if data.len() != expected {
            return Err(MtmdBitmapError::InvalidDataSize);
        }

        let bitmap = unsafe { harbor_llama_sys::mtmd_bitmap_init(nx, ny, data.as_ptr()) };

        let bitmap = NonNull::new(bitmap).ok_or(MtmdBitmapError::NullResult)?;
        Ok(Self { bitmap })
    }

    /// Create a bitmap from PCM F32 audio samples (at the context's
    /// [`MtmdContext::get_audio_sample_rate`]).
    ///
    /// # Errors
    ///
    /// * `NullResult` - Underlying C function returned null
    pub fn from_audio_data(data: &[f32]) -> Result<Self, MtmdBitmapError> {
        let bitmap =
            unsafe { harbor_llama_sys::mtmd_bitmap_init_from_audio(data.len(), data.as_ptr()) };

        let bitmap = NonNull::new(bitmap).ok_or(MtmdBitmapError::NullResult)?;
        Ok(Self { bitmap })
    }

    /// Create a bitmap from an encoded file in memory: images via `stb_image`
    /// (jpg, png, bmp, gif, ...), audio via miniaudio (wav, mp3, flac; detected
    /// by magic bytes). `placeholder` builds a data-less bitmap for token
    /// counting. Thread-safe.
    ///
    /// # Errors
    ///
    /// * `NullResult` - Buffer could not be decoded
    pub fn from_buffer(
        ctx: &MtmdContext,
        data: &[u8],
        placeholder: bool,
    ) -> Result<Self, MtmdBitmapError> {
        // Since llama.cpp b10xxx the helper takes init options and returns a
        // wrapper (bitmap + optional video decoder). This build has no
        // MTMD_VIDEO, so `video_ctx` stays null; free it anyway if set.
        let wrapper = unsafe {
            harbor_llama_sys::mtmd_helper_bitmap_init_from_buf(
                ctx.context.as_ptr(),
                data.as_ptr(),
                data.len(),
                placeholder,
                harbor_llama_sys::mtmd_helper_init_opt_default(),
            )
        };
        if !wrapper.video_ctx.is_null() {
            unsafe { harbor_llama_sys::mtmd_helper_video_free(wrapper.video_ctx) };
        }

        let bitmap = NonNull::new(wrapper.bitmap).ok_or(MtmdBitmapError::NullResult)?;
        Ok(Self { bitmap })
    }
}

impl Drop for MtmdBitmap {
    fn drop(&mut self) {
        unsafe { harbor_llama_sys::mtmd_bitmap_free(self.bitmap.as_ptr()) }
    }
}

/// Safe wrapper around `mtmd_input_chunks`: the tokenized text + media input.
#[derive(Debug)]
pub struct MtmdInputChunks {
    pub(crate) chunks: NonNull<harbor_llama_sys::mtmd_input_chunks>,
}

impl Default for MtmdInputChunks {
    fn default() -> Self {
        Self::new()
    }
}

impl MtmdInputChunks {
    /// Create a new empty input chunks collection
    ///
    /// # Panics
    /// If llama.cpp returns null (allocation failure).
    ///
    /// ```
    /// use harbor_llama::mtmd::MtmdInputChunks;
    /// let chunks = MtmdInputChunks::new();
    /// assert_eq!(chunks.len(), 0);
    /// assert!(chunks.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        let chunks = unsafe { harbor_llama_sys::mtmd_input_chunks_init() };
        let chunks = NonNull::new(chunks).expect("mtmd_input_chunks_init returned null");
        Self { chunks }
    }

    /// Get the number of chunks
    #[must_use]
    pub fn len(&self) -> usize {
        unsafe { harbor_llama_sys::mtmd_input_chunks_size(self.chunks.as_ptr()) }
    }

    /// Check if chunks collection is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a chunk by index (borrowed from this collection).
    #[must_use]
    pub fn get(&self, index: usize) -> Option<MtmdInputChunk<'_>> {
        if index >= self.len() {
            return None;
        }

        let chunk_ptr =
            unsafe { harbor_llama_sys::mtmd_input_chunks_get(self.chunks.as_ptr(), index) };

        NonNull::new(chunk_ptr.cast_mut()).map(|ptr| MtmdInputChunk {
            chunk: ptr,
            _chunks: std::marker::PhantomData,
        })
    }

    /// Get total number of tokens across all chunks.
    #[must_use]
    pub fn total_tokens(&self) -> usize {
        unsafe { harbor_llama_sys::mtmd_helper_get_n_tokens(self.chunks.as_ptr()) }
    }

    /// Evaluate the chunks into `llama_ctx` (`mtmd_helper_eval_chunks`): text
    /// chunks are decoded, media chunks encoded then decoded. Returns the new
    /// `n_past`. Not thread-safe.
    ///
    /// # Errors
    ///
    /// Returns `MtmdEvalError::EvalFailure` if any encoding or decoding operation fails.
    pub fn eval_chunks(
        &self,
        mtmd_ctx: &MtmdContext,
        llama_ctx: &LlamaContext,
        n_past: harbor_llama_sys::llama_pos,
        seq_id: harbor_llama_sys::llama_seq_id,
        n_batch: i32,
        logits_last: bool,
    ) -> Result<harbor_llama_sys::llama_pos, MtmdEvalError> {
        let mut new_n_past: harbor_llama_sys::llama_pos = 0;

        let result = unsafe {
            harbor_llama_sys::mtmd_helper_eval_chunks(
                mtmd_ctx.context.as_ptr(),
                llama_ctx.context.as_ptr(),
                self.chunks.as_ptr(),
                n_past,
                seq_id,
                n_batch,
                logits_last,
                &raw mut new_n_past,
            )
        };

        if result == 0 {
            Ok(new_n_past)
        } else {
            Err(MtmdEvalError::EvalFailure(result))
        }
    }
}

impl Drop for MtmdInputChunks {
    fn drop(&mut self) {
        unsafe { harbor_llama_sys::mtmd_input_chunks_free(self.chunks.as_ptr()) }
    }
}

/// One chunk (text tokens, image or audio) borrowed from [`MtmdInputChunks`].
#[derive(Debug)]
pub struct MtmdInputChunk<'a> {
    pub(crate) chunk: NonNull<harbor_llama_sys::mtmd_input_chunk>,
    _chunks: std::marker::PhantomData<&'a MtmdInputChunks>,
}

impl MtmdInputChunk<'_> {
    /// Get the type of this chunk
    #[must_use]
    pub fn chunk_type(&self) -> MtmdInputChunkType {
        let chunk_type =
            unsafe { harbor_llama_sys::mtmd_input_chunk_get_type(self.chunk.as_ptr()) };
        MtmdInputChunkType::from(chunk_type)
    }

    /// Text tokens of a text chunk; `None` for image or audio chunks.
    #[must_use]
    pub fn text_tokens(&self) -> Option<&[LlamaToken]> {
        if self.chunk_type() != MtmdInputChunkType::Text {
            return None;
        }

        let mut n_tokens = 0usize;
        let tokens_ptr = unsafe {
            harbor_llama_sys::mtmd_input_chunk_get_tokens_text(
                self.chunk.as_ptr(),
                &raw mut n_tokens,
            )
        };

        if tokens_ptr.is_null() || n_tokens == 0 {
            None
        } else {
            // LlamaToken is #[repr(transparent)] over llama_token.
            unsafe {
                Some(slice::from_raw_parts(
                    tokens_ptr.cast::<LlamaToken>(),
                    n_tokens,
                ))
            }
        }
    }

    /// Get the number of tokens in this chunk
    #[must_use]
    pub fn n_tokens(&self) -> usize {
        unsafe { harbor_llama_sys::mtmd_input_chunk_get_n_tokens(self.chunk.as_ptr()) }
    }
}

/// The default media marker (typically `"<__media__>"`) placed in the text
/// where each bitmap goes.
///
/// ```
/// use harbor_llama::mtmd::mtmd_default_marker;
/// assert!(!mtmd_default_marker().is_empty());
/// ```
#[must_use]
pub fn mtmd_default_marker() -> &'static str {
    unsafe {
        let c_str = harbor_llama_sys::mtmd_default_marker();
        CStr::from_ptr(c_str).to_str().unwrap_or("<__media__>")
    }
}

/// Errors that can occur when initializing MTMD context
#[derive(thiserror::Error, Debug)]
pub enum MtmdInitError {
    /// Failed to create `CString` from input
    #[error("Failed to create CString: {0}")]
    CStringError(#[from] std::ffi::NulError),
    /// MTMD context initialization returned null
    #[error("MTMD context initialization returned null")]
    NullResult,
}

/// Errors that can occur when working with MTMD bitmaps
#[derive(thiserror::Error, Debug)]
pub enum MtmdBitmapError {
    /// Failed to create `CString` from input
    #[error("Failed to create CString: {0}")]
    CStringError(#[from] std::ffi::NulError),
    /// Invalid data size for bitmap
    #[error("Invalid data size for bitmap")]
    InvalidDataSize,
    /// Bitmap creation returned null
    #[error("Bitmap creation returned null")]
    NullResult,
}

/// Errors that can occur during tokenization
#[derive(thiserror::Error, Debug)]
pub enum MtmdTokenizeError {
    /// Number of bitmaps does not match number of markers in text
    #[error("Number of bitmaps does not match number of markers")]
    BitmapCountMismatch,
    /// Image preprocessing error occurred
    #[error("Image preprocessing error")]
    ImagePreprocessingError,
    /// Text contains characters that cannot be converted to C string
    #[error("Failed to create CString from text: {0}")]
    CStringError(#[from] std::ffi::NulError),
    /// Unknown error occurred during tokenization
    #[error("Unknown error: {0}")]
    UnknownError(i32),
}

/// Errors that can occur during encoding
#[derive(thiserror::Error, Debug)]
pub enum MtmdEncodeError {
    /// Encode operation failed
    #[error("Encode failed with code: {0}")]
    EncodeFailure(i32),
}

/// Errors that can occur during evaluation
#[derive(thiserror::Error, Debug)]
pub enum MtmdEvalError {
    /// Evaluation operation failed
    #[error("Eval failed with code: {0}")]
    EvalFailure(i32),
}
