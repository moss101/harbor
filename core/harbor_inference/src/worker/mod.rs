//! The inference worker (decision 0022 phase 3): llama.cpp in a supervised
//! child process on platforms that allow one (macOS, Windows, Linux,
//! Android), in-process on iOS. [`GgufRuntime`] is what callers hold: the
//! same provider surface either way.

pub mod protocol;
#[cfg(feature = "gguf-backend")]
pub mod server;
pub mod supervisor;

pub use supervisor::{WorkerConfig, WorkerMode, WorkerProvider};

#[cfg(feature = "gguf-backend")]
pub use runtime::*;

#[cfg(feature = "gguf-backend")]
mod runtime {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::sync::Arc;

    use super::supervisor::{WorkerConfig, WorkerMode, WorkerProvider};
    use crate::gguf::GgufLlamaCppProvider;
    use crate::provider::{
        Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError,
    };
    use crate::resilience::RuntimeState;

    static LAST_CHOICE: std::sync::Mutex<Option<(&'static str, String)>> =
        std::sync::Mutex::new(None);

    /// The mode the most recent [`GgufRuntime::open`] chose and why (the
    /// launcher path, or the reason it fell back), for diagnostics.
    pub fn last_runtime_choice() -> Option<(&'static str, String)> {
        LAST_CHOICE.lock().unwrap().clone()
    }

    /// The GGUF runtime a caller holds: in this process, or in a worker.
    pub enum GgufRuntime {
        InProcess(GgufLlamaCppProvider),
        Worker(WorkerProvider),
    }

    impl GgufRuntime {
        /// Choose by [`WorkerMode::from_env`] and launcher discovery.
        pub fn open(models_root: impl Into<PathBuf>) -> Result<GgufRuntime, ProviderError> {
            let root = models_root.into();
            let mode = WorkerMode::from_env();
            let (runtime, why) = match mode {
                WorkerMode::Off => (Self::in_process(root), "worker off".to_string()),
                mode => match WorkerConfig::discover() {
                    Some(cfg) => {
                        let launcher = cfg.launcher.display().to_string();
                        let worker = WorkerProvider::new(cfg, root.clone());
                        // Start it now: a launcher that cannot run here (a
                        // sandbox refusal, a stale build) falls back to the
                        // in-process runtime in auto mode instead of failing
                        // every model call later.
                        match worker.start() {
                            Ok(()) => (Ok(GgufRuntime::Worker(worker)), launcher),
                            Err(e) if mode == WorkerMode::Required => (Err(e), launcher),
                            Err(e) => (
                                Self::in_process(root),
                                format!("worker {launcher} failed to start: {e}"),
                            ),
                        }
                    }
                    None if mode == WorkerMode::Required => (
                        Err(ProviderError::Backend(
                            "inference worker required (HARBOR_INFERENCE_WORKER) but its launcher was not found"
                                .into(),
                        )),
                        "launcher not found".to_string(),
                    ),
                    None => (Self::in_process(root), "launcher not found".to_string()),
                },
            };
            if let Ok(r) = &runtime {
                *LAST_CHOICE.lock().unwrap() = Some((r.mode(), why));
            }
            runtime
        }

        pub fn in_process(models_root: impl Into<PathBuf>) -> Result<GgufRuntime, ProviderError> {
            Ok(GgufRuntime::InProcess(GgufLlamaCppProvider::new(
                models_root,
            )?))
        }

        pub fn with_context_tokens(self, n: u32) -> GgufRuntime {
            match self {
                GgufRuntime::InProcess(p) => GgufRuntime::InProcess(p.with_context_tokens(n)),
                GgufRuntime::Worker(w) => GgufRuntime::Worker(w.with_context_tokens(n)),
            }
        }

        /// "in_process" or "worker" (diagnostics).
        pub fn mode(&self) -> &'static str {
            match self {
                GgufRuntime::InProcess(_) => "in_process",
                GgufRuntime::Worker(_) => "worker",
            }
        }

        pub fn as_in_process(&self) -> Option<&GgufLlamaCppProvider> {
            match self {
                GgufRuntime::InProcess(p) => Some(p),
                GgufRuntime::Worker(_) => None,
            }
        }

        pub fn as_worker(&self) -> Option<&WorkerProvider> {
            match self {
                GgufRuntime::Worker(w) => Some(w),
                GgufRuntime::InProcess(_) => None,
            }
        }

        pub fn runtime_state(&self) -> &Arc<RuntimeState> {
            match self {
                GgufRuntime::InProcess(p) => p.runtime_state(),
                GgufRuntime::Worker(w) => w.runtime_state(),
            }
        }

        pub fn generate_cancellable(
            &self,
            req: ChatRequest,
            cancel: &AtomicBool,
            tokens_generated: Option<&AtomicU64>,
        ) -> Result<ChatResponse, ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => p.generate_cancellable(req, cancel, tokens_generated),
                GgufRuntime::Worker(w) => w.generate_cancellable(req, cancel, tokens_generated),
            }
        }

        pub fn force_cpu(&self, package_id: &str) {
            match self {
                GgufRuntime::InProcess(p) => p.force_cpu(package_id),
                GgufRuntime::Worker(w) => w.force_cpu(package_id),
            }
        }

        pub fn mark_qualified(&self, package_id: &str) {
            match self {
                GgufRuntime::InProcess(p) => p.mark_qualified(package_id),
                GgufRuntime::Worker(w) => w.mark_qualified(package_id),
            }
        }

        pub fn backend_of(&self, package_id: &str) -> Option<&'static str> {
            match self {
                GgufRuntime::InProcess(p) => p.backend_of(package_id),
                GgufRuntime::Worker(w) => w.backend_of(package_id),
            }
        }

        pub fn weights_file(&self, package_id: &str) -> Result<PathBuf, ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => p.weights_file(package_id),
                GgufRuntime::Worker(w) => w.weights_file(package_id),
            }
        }

        pub fn mmproj_file(&self, package_id: &str) -> Result<PathBuf, ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => p.mmproj_file(package_id),
                GgufRuntime::Worker(w) => w.mmproj_file(package_id),
            }
        }

        /// The media towers of a loaded package, wherever the model runs.
        #[cfg(feature = "multimodal")]
        pub fn open_media(
            &self,
            package_id: &str,
        ) -> Result<Arc<dyn crate::multimodal::MediaEmbed>, ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => Ok(Arc::new(crate::multimodal::MediaEmbedder::open(
                    p, package_id,
                )?)),
                GgufRuntime::Worker(w) => {
                    Ok(Arc::new(super::client::WorkerMedia::open(w, package_id)?))
                }
            }
        }

        /// A verified vision-OCR engine over an installed package and the
        /// backend it runs on (see `VisionOcr::open_verified`).
        #[cfg(feature = "multimodal")]
        pub fn open_vision_ocr(
            &self,
            package_id: &str,
        ) -> Result<(Arc<dyn crate::ocr::OcrEngine>, String), ProviderError> {
            let model = ModelRef::InstalledPackage {
                package_id: package_id.to_string(),
            };
            match self {
                GgufRuntime::InProcess(p) => {
                    p.load(&model)?;
                    let (e, b) = crate::vision_ocr::VisionOcr::open_verified(p, &model, package_id)
                        .map_err(|e| ProviderError::Backend(e.to_string()))?;
                    Ok((Arc::new(e), b.to_string()))
                }
                GgufRuntime::Worker(w) => {
                    let (e, b) = super::client::WorkerOcr::open(w, package_id)?;
                    Ok((Arc::new(e), b))
                }
            }
        }
    }

    impl ModelProvider for GgufRuntime {
        fn id(&self) -> &str {
            match self {
                GgufRuntime::InProcess(p) => p.id(),
                GgufRuntime::Worker(w) => w.id(),
            }
        }
        fn capabilities(&self) -> &'static [Capabilities] {
            match self {
                GgufRuntime::InProcess(p) => p.capabilities(),
                GgufRuntime::Worker(w) => w.capabilities(),
            }
        }
        fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
            match self {
                GgufRuntime::InProcess(p) => p.supports(model, need),
                GgufRuntime::Worker(w) => w.supports(model, need),
            }
        }
        fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => p.load(model),
                GgufRuntime::Worker(w) => w.load(model),
            }
        }
        fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => p.unload(model),
                GgufRuntime::Worker(w) => w.unload(model),
            }
        }
        fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => p.generate(req),
                GgufRuntime::Worker(w) => w.generate(req),
            }
        }
        fn embed(
            &self,
            model: &ModelRef,
            texts: &[String],
        ) -> Result<Vec<Vec<f32>>, ProviderError> {
            match self {
                GgufRuntime::InProcess(p) => ModelProvider::embed(p, model, texts),
                GgufRuntime::Worker(w) => w.embed(model, texts),
            }
        }
        fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
            harbor_security::policy::ExecutionLocation::OnDevice
        }
    }
}

#[cfg(feature = "multimodal")]
mod client;
