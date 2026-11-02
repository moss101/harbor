//! GGUF backend over the pinned llama.cpp runtime.
//!
//! The llama.cpp source snapshot is vendored inside `llama-cpp-sys-2`
//! (pinned `=0.1.156`), so the crate lockfile pins the exact runtime
//! revision; `runtime_revision()` reports it and the integrity hash of the
//! packed `.crate` archive is recorded by `tools/pin_engine.py` (same
//! mechanism as the formula engine).
//!
//! Implementation notes (honest scope):
//! - Prompt assembly prefers the model-native chat template carried in
//!   the GGUF metadata and falls back to a minimal documented template;
//!   greedy decoding at temperature 0 keeps behavior deterministic.
//! - `ChatRequest::response_schema` constrains decoding to a JSON Schema
//!   through a llama.cpp grammar (`StructuredOutput` capability).
//! - Generation is cancellable through [`GgufLlamaCppProvider::generate_cancellable`];
//!   the trait method runs to completion (or EOS / token budget).
//! - No model file executes code: GGUF is parsed as data by llama.cpp.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use llama_cpp_2::context::params::{LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use harbor_canonical::JsonValue;
use harbor_modelhub::install::{PackageFile, PackageInstaller};

use crate::provider::{
    Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError, Usage,
};
use crate::resilience::{DeviceKey, RuntimeState, SentinelGuard, WorkTicket};

/// The pinned runtime identity (crate pin -> llama.cpp snapshot).
pub fn runtime_revision() -> &'static str {
    "llama.cpp/llama-cpp-sys-2@0.1.156"
}

pub struct GgufLlamaCppProvider {
    /// Zero-sized marker; real state is process-global (see shared_backend).
    backend: &'static LlamaBackend,
    installed_root: PathBuf,
    loaded: Mutex<BTreeMap<String, Arc<LlamaModel>>>,
    /// Packages pinned to the CPU backend because their GPU path failed a
    /// numerics canary (see [`GgufLlamaCppProvider::force_cpu`]). Sticky
    /// across unload/reload so a memory release never re-enables a GPU
    /// path already proven wrong on this device.
    cpu_only: Mutex<std::collections::BTreeSet<String>>,
    /// Crash sentinels and the per-device qualification store (decision
    /// 0022 phase 1), shared by every provider over the same data root.
    runtime: Arc<RuntimeState>,
    /// The GPU key each GPU-loaded package runs under. Absent = CPU.
    gpu_keys: Mutex<BTreeMap<String, DeviceKey>>,
    /// Packages the qualification store sent to the CPU at their last load.
    store_pinned: Mutex<std::collections::BTreeSet<String>>,
    /// Context window used for generation contexts.
    context_tokens: u32,
}

/// The first GPU device ggml enumerates (backend name, description), or
/// None when this build or device has no GPU backend.
fn gpu_device() -> Option<(String, String)> {
    use llama_cpp_2::LlamaBackendDeviceType as T;
    llama_cpp_2::list_llama_ggml_backend_devices()
        .into_iter()
        .find(|d| matches!(d.device_type, T::Gpu | T::IntegratedGpu))
        .map(|d| (d.backend, d.description))
}

/// Whether this build and device expose a GPU backend.
pub fn gpu_backend_present() -> bool {
    gpu_device().is_some()
}

pub use crate::resilience::runtime_dir_for;

/// Threads for CPU compute. llama.cpp defaults to 4 whatever the device
/// has; `HARBOR_GGUF_THREADS` overrides, otherwise [`default_threads`].
pub fn compute_threads() -> i32 {
    std::env::var("HARBOR_GGUF_THREADS")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or_else(default_threads)
}

fn default_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4)
        // Measured on a OnePlus (8 cores, Snapdragon 8-class): images go
        // 3.5 s -> 2.6 s from 4 to 6 threads and gain nothing from 8;
        // audio does not benefit from more than 4.
        .clamp(1, 6)
}

/// Process-global backend: llama.cpp's `llama_backend_init` is
/// process-global and may only be marked initialized once.
fn shared_backend() -> Result<&'static LlamaBackend, ProviderError> {
    static BACKEND: std::sync::OnceLock<LlamaBackend> = std::sync::OnceLock::new();
    BACKEND
        .get_or_init(|| LlamaBackend::init().expect("llama backend single init"))
        .pipe(Ok)
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

impl GgufLlamaCppProvider {
    /// Pin `package_id` to the CPU backend and drop any GPU-loaded copy;
    /// the next `load` reloads it on the CPU. Used when a numerics canary
    /// shows the device's GPU path produces wrong embeddings (the
    /// simulator's GPU, or hardware without bfloat — EmbeddingGemma 2's
    /// activations overflow float16, which then degrades silently).
    pub fn force_cpu(&self, package_id: &str) {
        if let Some(key) = self.gpu_keys.lock().unwrap().remove(package_id) {
            self.runtime.record_canary(&key, false);
        }
        self.cpu_only.lock().unwrap().insert(package_id.to_string());
        self.loaded.lock().unwrap().remove(package_id);
    }

    /// Record that `package_id` passed a numerics canary on its current
    /// backend (a no-op on the CPU path, which needs no qualification).
    pub fn mark_qualified(&self, package_id: &str) {
        if let Some(key) = self.gpu_keys.lock().unwrap().get(package_id) {
            self.runtime.record_canary(key, true);
        }
    }

    /// Where `package_id` is loaded now (None = not loaded): "gpu";
    /// "cpu_pinned" when a GPU exists but this package is pinned off it (a
    /// failed canary in this process, or a crash or failed canary the
    /// qualification store remembers); "cpu" when there is no GPU path.
    pub fn backend_of(&self, package_id: &str) -> Option<&'static str> {
        if !self.loaded.lock().unwrap().contains_key(package_id) {
            return None;
        }
        Some(if self.gpu_keys.lock().unwrap().contains_key(package_id) {
            "gpu"
        } else if self.store_pinned.lock().unwrap().contains(package_id)
            || (self.cpu_only.lock().unwrap().contains(package_id) && gpu_device().is_some())
        {
            "cpu_pinned"
        } else {
            "cpu"
        })
    }

    /// The qualification store this provider records into.
    pub fn runtime_state(&self) -> &Arc<RuntimeState> {
        &self.runtime
    }

    /// The GPU key of a GPU-loaded package, for engines that run their own
    /// GPU work on its model (media embedder, vision OCR). None on the CPU.
    pub fn work_ticket(&self, package_id: &str) -> Option<WorkTicket> {
        self.gpu_keys
            .lock()
            .unwrap()
            .get(package_id)
            .map(|key| WorkTicket {
                state: self.runtime.clone(),
                key: key.clone(),
            })
    }

    fn work_guard(&self, package_id: &str) -> Option<SentinelGuard> {
        self.work_ticket(package_id).map(|t| t.arm())
    }

    /// The GPU key `package_id` would load under, or None when no GPU
    /// backend is present (or the weights hash is unknown).
    fn device_key(&self, package_id: &str) -> Option<DeviceKey> {
        let (backend, device) = gpu_device()?;
        let manifest = PackageInstaller::new(&self.installed_root)
            .load_manifest(package_id)
            .ok()?;
        let weights = manifest
            .files
            .iter()
            .find(|f| f.role == "weights" || f.role == "weights_shard")?;
        Some(DeviceKey {
            backend,
            device,
            platform: crate::resilience::platform_fingerprint(),
            model_sha256: weights.sha256.clone(),
            runtime: runtime_revision().to_string(),
        })
    }

    /// Whether `package_id` is pinned to the CPU backend.
    pub fn is_cpu_pinned(&self, package_id: &str) -> bool {
        self.cpu_only.lock().unwrap().contains(package_id)
    }

    pub fn new(installed_root: impl Into<PathBuf>) -> Result<Self, ProviderError> {
        let installed_root = installed_root.into();
        let runtime_dir = runtime_dir_for(&installed_root);
        Self::with_runtime_dir(installed_root, runtime_dir)
    }

    /// Like [`new`](Self::new) with an explicit runtime state directory.
    pub fn with_runtime_dir(
        installed_root: impl Into<PathBuf>,
        runtime_dir: impl Into<PathBuf>,
    ) -> Result<Self, ProviderError> {
        let backend = shared_backend()?;
        Ok(GgufLlamaCppProvider {
            backend,
            installed_root: installed_root.into(),
            loaded: Mutex::new(BTreeMap::new()),
            cpu_only: Mutex::new(Default::default()),
            runtime: RuntimeState::open(runtime_dir),
            gpu_keys: Mutex::new(BTreeMap::new()),
            store_pinned: Mutex::new(Default::default()),
            context_tokens: 2048,
        })
    }

    pub fn with_context_tokens(mut self, n: u32) -> Self {
        self.context_tokens = n;
        self
    }

    /// The process-global llama backend (needed to open extra contexts).
    pub fn backend(&self) -> &'static LlamaBackend {
        self.backend
    }

    /// The loaded model for `package_id`, if resident.
    pub fn loaded_model(&self, package_id: &str) -> Option<Arc<LlamaModel>> {
        self.loaded.lock().unwrap().get(package_id).cloned()
    }

    /// Path of the package's primary weights file.
    pub fn weights_file(&self, package_id: &str) -> Result<PathBuf, ProviderError> {
        self.weights_path(package_id)
    }

    /// Path of the package's multimodal projector (`mmproj` role), or a
    /// typed error when the package is text-only.
    pub fn mmproj_file(&self, package_id: &str) -> Result<PathBuf, ProviderError> {
        let manifest = PackageInstaller::new(&self.installed_root)
            .load_manifest(package_id)
            .map_err(|e| ProviderError::ModelNotFound(format!("{package_id}: {e}")))?;
        let f = manifest.files.iter().find(|f| f.role == "mmproj").ok_or(
            ProviderError::UnsupportedCapability("package has no multimodal projector"),
        )?;
        let path = self.installed_root.join(package_id).join(&f.path);
        if path.exists() {
            Ok(path)
        } else {
            Err(ProviderError::ModelNotFound(format!(
                "missing file {}",
                f.path
            )))
        }
    }

    fn weights_path(&self, package_id: &str) -> Result<PathBuf, ProviderError> {
        let installer = PackageInstaller::new(&self.installed_root);
        let manifest = installer
            .load_manifest(package_id)
            .map_err(|e| ProviderError::ModelNotFound(format!("{package_id}: {e}")))?;
        let weights: Option<&PackageFile> = manifest
            .files
            .iter()
            .find(|f| f.role == "weights" || f.role == "weights_shard");
        let Some(f) = weights else {
            return Err(ProviderError::ModelNotFound(format!(
                "{package_id}: no weights"
            )));
        };
        let path = self.installed_root.join(package_id).join(&f.path);
        if !path.exists() {
            return Err(ProviderError::ModelNotFound(format!(
                "missing file {}",
                f.path
            )));
        }
        Ok(path)
    }

    /// Convert canonical JSON messages into engine chat messages.
    fn to_chat_messages(messages: &[JsonValue]) -> Result<Vec<LlamaChatMessage>, ProviderError> {
        messages
            .iter()
            .map(|m| {
                let role = m
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user")
                    .to_string();
                let content = m
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                LlamaChatMessage::new(role, content)
                    .map_err(|e| ProviderError::Backend(format!("chat message: {e}")))
            })
            .collect()
    }

    /// Assemble the prompt. Preference order:
    /// 1. the MODEL-NATIVE chat template carried in the GGUF metadata,
    /// 2. the documented minimal template (fallback for models without one).
    fn build_prompt(model: &LlamaModel, messages: &[JsonValue]) -> Result<String, ProviderError> {
        let chat = Self::to_chat_messages(messages)?;
        if let Ok(template) = model.chat_template(None) {
            let rendered = model
                .apply_chat_template(&template, &chat, true)
                .map_err(|e| ProviderError::Backend(format!("template: {e}")))?;
            if !rendered.trim().is_empty() {
                return Ok(rendered);
            }
        }
        // Fallback: minimal documented template.
        let mut out = String::new();
        for m in messages {
            let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("");
            match role {
                "system" => out.push_str(&format!("<system>{content}</system>\n")),
                "user" => out.push_str(&format!("<user>{content}</user>\n")),
                "assistant" => out.push_str(&format!("<assistant>{content}</assistant>\n")),
                other => out.push_str(&format!("<{other}>{content}</{other}>\n")),
            }
        }
        out.push_str("<assistant>\n");
        Ok(out)
    }

    /// Generate with cooperative cancellation. `cancel` checked between
    /// tokens; `tokens_generated` (when given) is updated after every
    /// decoded token so runtime layers can surface real progress.
    pub fn generate_cancellable(
        &self,
        req: ChatRequest,
        cancel: &AtomicBool,
        tokens_generated: Option<&AtomicU64>,
    ) -> Result<ChatResponse, ProviderError> {
        let package = match &req.model {
            ModelRef::InstalledPackage { package_id } => package_id.clone(),
            other => {
                return Err(ProviderError::ModelNotFound(format!(
                    "gguf backend only loads installed packages, got {other:?}"
                )))
            }
        };
        let model = {
            let loaded = self.loaded.lock().unwrap();
            loaded
                .get(&package)
                .cloned()
                .ok_or_else(|| ProviderError::ModelNotFound(package.clone()))?
        };

        let _work = self.work_guard(&package);
        let prompt = Self::build_prompt(&model, &req.messages)?;
        let tokens = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| ProviderError::Backend(format!("tokenize: {e}")))?;
        let prompt_len = tokens.len();
        if prompt_len == 0 {
            return Err(ProviderError::Backend("empty prompt".into()));
        }
        // A prompt that cannot fit the model's trained context together
        // with the requested completion is refused here as a typed error;
        // llama.cpp would otherwise abort the whole process.
        let needed = prompt_len as u32 + req.max_tokens.max(1);
        if needed > model.n_ctx_train() {
            return Err(ProviderError::Backend(format!(
                "prompt of {prompt_len} tokens plus {} completion tokens exceeds the model context ({})",
                req.max_tokens,
                model.n_ctx_train()
            )));
        }
        let n_ctx = self.context_tokens.max(needed).min(model.n_ctx_train());
        let n_ctx =
            std::num::NonZeroU32::new(n_ctx).ok_or(ProviderError::Backend("n_ctx 0".into()))?;
        let ctx_params = LlamaContextParams::default().with_n_ctx(Some(n_ctx));
        let mut ctx = model
            .new_context(self.backend, ctx_params)
            .map_err(|e| ProviderError::Backend(format!("context: {e}")))?;

        // Prefill the prompt in batches no larger than the context's
        // n_batch (llama.cpp asserts n_tokens <= n_batch and aborts the
        // process otherwise); logits are requested for the last token only.
        let n_batch = (ctx.n_batch() as usize).max(1);
        let mut last_chunk_len = 0usize;
        for (chunk_index, chunk) in tokens.chunks(n_batch).enumerate() {
            let offset = chunk_index * n_batch;
            last_chunk_len = chunk.len();
            let mut batch = LlamaBatch::new(chunk.len(), 1);
            for (i, t) in chunk.iter().enumerate() {
                let pos = offset + i;
                batch
                    .add(*t, pos as i32, &[0], pos + 1 == prompt_len)
                    .map_err(|e| ProviderError::Backend(format!("batch: {e}")))?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| ProviderError::Backend(format!("decode: {e}")))?;
        }

        // Structured output: constrain decoding to the caller's JSON
        // Schema through a llama.cpp grammar. The grammar is a constraint
        // on generation only; the executor still validates the parsed
        // result below the model (03 §3).
        let grammar = match &req.response_schema {
            Some(schema) => {
                let schema_json = schema
                    .to_canonical_bytes()
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .ok_or_else(|| {
                        ProviderError::Backend("response_schema is not canonical JSON".into())
                    })?;
                let gbnf = llama_cpp_2::json_schema_to_grammar(&schema_json)
                    .map_err(|e| ProviderError::Backend(format!("schema grammar: {e}")))?;
                Some(
                    LlamaSampler::grammar(&model, &gbnf, "root")
                        .map_err(|e| ProviderError::Backend(format!("grammar sampler: {e}")))?,
                )
            }
            None => None,
        };
        let mut chain: Vec<LlamaSampler> = Vec::new();
        if let Some(g) = grammar {
            chain.push(g);
        }
        if req.temperature <= 0.0 {
            chain.push(LlamaSampler::greedy());
        } else {
            chain.push(LlamaSampler::temp(req.temperature));
            chain.push(LlamaSampler::dist(0x48415242)); // "HARB"
        }
        let mut sampler = LlamaSampler::chain_simple(chain);
        let eos = model.token_eos();
        let mut out = String::new();
        // One decoder for the whole generation: it holds the bytes of a
        // multi-byte character that a token only starts (byte-level BPE
        // splits e.g. Arabic-Indic digits across tokens). A fresh decoder
        // per token turned each fragment into U+FFFD.
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut generated: u64 = 0;
        let mut next_pos = prompt_len as i32;
        let max_tokens = req.max_tokens.max(1);
        // After each decode, the logits-bearing index within the LAST
        // batch: the last token of the final prefill chunk, then 0 after
        // each 1-token step.
        let mut logits_index: i32 = last_chunk_len as i32 - 1;
        while generated < max_tokens as u64 {
            if cancel.load(Ordering::Relaxed) {
                return Err(ProviderError::Cancelled);
            }
            let tok = sampler.sample(&ctx, logits_index);
            if tok == eos {
                break;
            }
            let piece = model
                .token_to_piece(tok, &mut decoder, false, None)
                .map_err(|e| ProviderError::Backend(format!("detokenize: {e}")))?;
            out.push_str(&piece);
            generated += 1;
            if let Some(counter) = tokens_generated {
                counter.store(generated, Ordering::Relaxed);
            }
            // Fault injection for the crash-sentinel tests (decision 0022):
            // a "GPU crash" — only while a GPU workload is armed — in debug
            // builds only, never compiled into a release.
            // Debug builds only: slow decoding down so cancellation can be
            // observed across the worker boundary in tests.
            #[cfg(debug_assertions)]
            if let Some(ms) = std::env::var("HARBOR_FAULT_SLOW_TOKEN_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
            {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            #[cfg(debug_assertions)]
            if _work.is_some()
                && std::env::var("HARBOR_FAULT_ABORT_AT_TOKEN")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    == Some(generated)
            {
                std::process::abort();
            }
            // Feed the sampled token back.
            let mut step = LlamaBatch::new(1, 1);
            step.add(tok, next_pos, &[0], true)
                .map_err(|e| ProviderError::Backend(format!("batch: {e}")))?;
            ctx.decode(&mut step)
                .map_err(|e| ProviderError::Backend(format!("decode: {e}")))?;
            next_pos += 1;
            logits_index = 0;
        }
        Ok(ChatResponse {
            content: out,
            usage: Usage {
                prompt_tokens: prompt_len as u64,
                completion_tokens: generated,
            },
            executed_on: package,
            execution_location: harbor_security::policy::ExecutionLocation::OnDevice,
        })
    }
}

impl GgufLlamaCppProvider {
    /// Embed texts with the model's pooling (mean). The caller MUST record
    /// `model identity + revision + dimension` in the knowledge index
    /// identity (`harbor_knowledge`) — vectors from a different embedding
    /// identity are never combinable.
    pub fn embed(
        &self,
        model_ref: &ModelRef,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, ProviderError> {
        let package = match model_ref {
            ModelRef::InstalledPackage { package_id } => package_id.clone(),
            other => {
                return Err(ProviderError::ModelNotFound(format!(
                    "gguf backend only embeds installed packages, got {other:?}"
                )))
            }
        };
        let model = {
            let loaded = self.loaded.lock().unwrap();
            loaded
                .get(&package)
                .cloned()
                .ok_or_else(|| ProviderError::ModelNotFound(package.clone()))?
        };
        let _work = self.work_guard(&package);
        let n_ctx = self.context_tokens.max(512).min(model.n_ctx_train());
        let n_ctx =
            std::num::NonZeroU32::new(n_ctx).ok_or(ProviderError::Backend("n_ctx 0".into()))?;
        // Pooled embeddings need the whole text in one batch: size the
        // batch to the context so long chunks cannot trip llama.cpp's
        // n_tokens <= n_batch assertion (texts longer than the context are
        // refused below).
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(n_ctx))
            .with_n_batch(n_ctx.get())
            .with_n_ubatch(n_ctx.get())
            .with_embeddings(true)
            .with_pooling_type(LlamaPoolingType::Mean)
            .with_n_threads(compute_threads())
            .with_n_threads_batch(compute_threads());
        let mut ctx = model
            .new_context(self.backend, ctx_params)
            .map_err(|e| ProviderError::Backend(format!("context: {e}")))?;
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            let tokens = model
                .str_to_token(text, AddBos::Always)
                .map_err(|e| ProviderError::Backend(format!("tokenize: {e}")))?;
            if tokens.is_empty() {
                out.push(Vec::new());
                continue;
            }
            if tokens.len() > n_ctx.get() as usize {
                return Err(ProviderError::Backend(format!(
                    "text of {} tokens exceeds the embedding context ({})",
                    tokens.len(),
                    n_ctx.get()
                )));
            }
            let mut batch = LlamaBatch::new(tokens.len(), 1);
            for (pos, t) in tokens.iter().enumerate() {
                batch
                    .add(*t, pos as i32, &[0], false)
                    .map_err(|e| ProviderError::Backend(format!("batch: {e}")))?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| ProviderError::Backend(format!("decode: {e}")))?;
            let emb = ctx
                .embeddings_seq_ith(0)
                .map_err(|e| ProviderError::Backend(format!("embeddings: {e}")))?;
            out.push(emb.to_vec());
        }
        Ok(out)
    }
}

impl ModelProvider for GgufLlamaCppProvider {
    fn id(&self) -> &str {
        "harbor.gguf.llama-cpp"
    }

    fn capabilities(&self) -> &'static [Capabilities] {
        &[
            Capabilities::Chat,
            Capabilities::Embeddings,
            Capabilities::StructuredOutput,
        ]
    }

    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        match (model, need) {
            (ModelRef::InstalledPackage { package_id }, Capabilities::Chat)
            | (ModelRef::InstalledPackage { package_id }, Capabilities::Embeddings)
            | (ModelRef::InstalledPackage { package_id }, Capabilities::StructuredOutput) => {
                self.weights_path(package_id).is_ok()
            }
            _ => false,
        }
    }

    fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
        let ModelRef::InstalledPackage { package_id } = model else {
            return Err(ProviderError::ModelNotFound(
                "gguf loads installed packages only".into(),
            ));
        };
        {
            let loaded = self.loaded.lock().unwrap();
            if loaded.contains_key(package_id) {
                return Ok(());
            }
        }
        let path = self.weights_path(package_id)?;
        // Decision 0022 phase 2: validate the file and the memory budget
        // before llama.cpp sees it. A malformed file or a model that cannot
        // fit becomes a typed error here instead of an abort or an OOM kill.
        let summary = crate::gguf_check::check_file(&path)
            .map_err(|e| ProviderError::Backend(format!("model file rejected before load: {e}")))?;
        let mut estimate = crate::memory_budget::estimate_load_bytes(&summary, self.context_tokens);
        if let Ok(mmproj) = self.mmproj_file(package_id) {
            let projector = crate::gguf_check::check_file(&mmproj).map_err(|e| {
                ProviderError::Backend(format!("projector file rejected before load: {e}"))
            })?;
            estimate = estimate.saturating_add(projector.tensor_bytes);
        }
        crate::memory_budget::check_budget(estimate)
            .map_err(|e| ProviderError::Backend(format!("model {package_id} does not fit: {e}")))?;
        // HARBOR_GGUF_CPU_ONLY=1 keeps every layer on the CPU.
        //
        // A diagnostic escape hatch, not a feature. The iOS simulator
        // returns schema-valid token soup from this same code path while
        // macOS returns a correct answer from the same weights, prompt
        // and grammar, and the two plausible causes — wrong numerics in
        // the GPU kernels, or a wrong template/tokenizer path — are only
        // separable by taking the GPU out of the loop. It doubles as a
        // way to keep a device usable if its GPU kernels are broken.
        let mut params = llama_cpp_2::model::params::LlamaModelParams::default();
        // The GPU key, unless this load is CPU-only anyway. A key the
        // qualification store has pinned (a crash or a failed canary on this
        // device, driver and runtime) loads on the CPU like an explicit pin.
        let explicit_cpu = std::env::var("HARBOR_GGUF_CPU_ONLY").as_deref() == Ok("1")
            || self.cpu_only.lock().unwrap().contains(package_id);
        let gpu_key = if explicit_cpu {
            None
        } else {
            self.device_key(package_id)
        };
        let store_pinned = gpu_key.as_ref().is_some_and(|k| self.runtime.pins_cpu(k));
        let gpu_key = gpu_key.filter(|_| !store_pinned);
        if explicit_cpu || store_pinned {
            // Zero GPU layers is NOT enough when a GPU backend is compiled
            // in: ggml's scheduler still offloads heavy host-weight ops to
            // the device. An empty device list is the real CPU-only switch
            // (found on an Adreno Vulkan phone where the "CPU fallback"
            // kept producing the GPU's wrong embeddings).
            params = params
                .with_n_gpu_layers(0)
                .with_devices(&[])
                .map_err(|e| ProviderError::Backend(format!("cpu-only devices: {e}")))?;
        }
        let model = {
            // Armed across the GPU load: a crash here pins the key on the
            // next launch (decision 0022 phase 1).
            let _init = gpu_key.as_ref().map(|k| self.runtime.arm_load(k));
            LlamaModel::load_from_file(self.backend, path, &params)
                .map_err(|e| ProviderError::Backend(format!("model load: {e}")))?
        };
        match gpu_key {
            Some(k) => self.gpu_keys.lock().unwrap().insert(package_id.clone(), k),
            None => self.gpu_keys.lock().unwrap().remove(package_id),
        };
        if store_pinned {
            self.store_pinned.lock().unwrap().insert(package_id.clone());
        } else {
            self.store_pinned.lock().unwrap().remove(package_id);
        }
        self.loaded
            .lock()
            .unwrap()
            .insert(package_id.clone(), Arc::new(model));
        Ok(())
    }

    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
        if let ModelRef::InstalledPackage { package_id } = model {
            self.loaded.lock().unwrap().remove(package_id);
            self.gpu_keys.lock().unwrap().remove(package_id);
        }
        Ok(())
    }

    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let never = AtomicBool::new(false);
        self.generate_cancellable(req, &never, None)
    }

    // The embedding implementation is inherent (takes &self, returns the
    // pooled vectors); without this override the TRAIT default would
    // serve UnsupportedCapability to every &dyn ModelProvider consumer
    // — found by the live skill-routing tier (decision 0011).
    fn embed(&self, model: &ModelRef, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        GgufLlamaCppProvider::embed(self, model, texts)
    }

    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        harbor_security::policy::ExecutionLocation::OnDevice
    }
}
