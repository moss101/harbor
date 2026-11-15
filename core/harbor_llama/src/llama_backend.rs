//! Representation of an initialized llama backend

use crate::LlamaCppError;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;

/// Representation of an initialized llama backend.
/// This is required as a parameter for most llama functions as the backend must be initialized
/// before any llama functions are called. This type is proof of initialization.
#[derive(Eq, PartialEq, Debug)]
pub struct LlamaBackend {}

static LLAMA_BACKEND_INITIALIZED: AtomicBool = AtomicBool::new(false);

impl LlamaBackend {
    /// Initialize the llama backend (without numa). It can be initialized
    /// once at a time; a second call before the first backend is dropped
    /// returns [`LlamaCppError::BackendAlreadyInitialized`].
    ///
    /// # Errors
    /// The backend is already initialized.
    pub fn init() -> crate::Result<LlamaBackend> {
        match LLAMA_BACKEND_INITIALIZED.compare_exchange(false, true, SeqCst, SeqCst) {
            Ok(_) => {}
            Err(_) => return Err(LlamaCppError::BackendAlreadyInitialized),
        }
        unsafe { harbor_llama_sys::llama_backend_init() }
        Ok(LlamaBackend {})
    }
}

impl Drop for LlamaBackend {
    fn drop(&mut self) {
        match LLAMA_BACKEND_INITIALIZED.compare_exchange(true, false, SeqCst, SeqCst) {
            Ok(_) => {}
            Err(_) => {
                unreachable!("the only way to obtain a llama backend marks it as initialized")
            }
        }
        unsafe { harbor_llama_sys::llama_backend_free() }
    }
}
