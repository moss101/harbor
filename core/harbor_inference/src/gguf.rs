//! GGUF backend over the pinned llama.cpp runtime.
//!
//! The llama.cpp source snapshot is vendored inside `llama-cpp-sys-2`
//! (pinned `=0.1.156`), so the crate lockfile pins the exact runtime
//! revision; `runtime_revision()` reports it and the integrity hash of the
//! packed `.crate` archive is recorded by `tools/pin_engine.py` (same
//! mechanism as the formula engine).
//!
//! Implementation notes (honest scope):
//! - Prompt assembly uses a minimal documented template (system + turns)
//!   and greedy decoding at temperature 0 for deterministic behavior.
//!   Model-native chat templates are a follow-up keyed to GGUF metadata.
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

/// The pinned runtime identity (crate pin -> llama.cpp snapshot).
pub fn runtime_revision() -> &'static str {
    "llama.cpp/llama-cpp-sys-2@0.1.156"
}

pub struct GgufLlamaCppProvider {
    /// Zero-sized marker; real state is process-global (see shared_backend).
    backend: &'static LlamaBackend,
    installed_root: PathBuf,
    loaded: Mutex<BTreeMap<String, Arc<LlamaModel>>>,
    /// Context window used for generation contexts.
    context_tokens: u32,
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
    pub fn new(installed_root: impl Into<PathBuf>) -> Result<Self, ProviderError> {
        let backend = shared_backend()?;
        Ok(GgufLlamaCppProvider {
            backend,
            installed_root: installed_root.into(),
            loaded: Mutex::new(BTreeMap::new()),
            context_tokens: 2048,
        })
    }

    pub fn with_context_tokens(mut self, n: u32) -> Self {
        self.context_tokens = n;
        self
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

        let prompt = Self::build_prompt(&model, &req.messages)?;
        let tokens = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| ProviderError::Backend(format!("tokenize: {e}")))?;
        let prompt_len = tokens.len();
        if prompt_len == 0 {
            return Err(ProviderError::Backend("empty prompt".into()));
        }
        let n_ctx = self
            .context_tokens
            .max(prompt_len as u32 + req.max_tokens)
            .min(model.n_ctx_train());
        let n_ctx =
            std::num::NonZeroU32::new(n_ctx).ok_or(ProviderError::Backend("n_ctx 0".into()))?;
        let ctx_params = LlamaContextParams::default().with_n_ctx(Some(n_ctx));
        let mut ctx = model
            .new_context(self.backend, ctx_params)
            .map_err(|e| ProviderError::Backend(format!("context: {e}")))?;

        // Prefill the prompt in one batch.
        let mut batch = LlamaBatch::new(prompt_len.max(1), 1);
        for (pos, t) in tokens.iter().enumerate() {
            batch
                .add(*t, pos as i32, &[0], pos + 1 == prompt_len)
                .map_err(|e| ProviderError::Backend(format!("batch: {e}")))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| ProviderError::Backend(format!("decode: {e}")))?;

        let sampler: LlamaSampler = if req.temperature <= 0.0 {
            LlamaSampler::chain_simple([LlamaSampler::greedy()])
        } else {
            LlamaSampler::chain_simple([
                LlamaSampler::temp(req.temperature),
                LlamaSampler::dist(0x48415242), // "HARB"
            ])
        };
        let mut sampler = sampler;
        let eos = model.token_eos();
        let mut out = String::new();
        let mut generated: u64 = 0;
        let mut next_pos = prompt_len as i32;
        let max_tokens = req.max_tokens.max(1);
        // After each decode, the logits-bearing index within the LAST
        // batch: prompt_len-1 after prefill, 0 after each 1-token step.
        let mut logits_index: i32 = prompt_len as i32 - 1;
        while generated < max_tokens as u64 {
            if cancel.load(Ordering::Relaxed) {
                return Err(ProviderError::Cancelled);
            }
            let tok = sampler.sample(&ctx, logits_index);
            if tok == eos {
                break;
            }
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            let piece = model
                .token_to_piece(tok, &mut decoder, false, None)
                .map_err(|e| ProviderError::Backend(format!("detokenize: {e}")))?;
            out.push_str(&piece);
            generated += 1;
            if let Some(counter) = tokens_generated {
                counter.store(generated, Ordering::Relaxed);
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
        let n_ctx = self.context_tokens.max(512).min(model.n_ctx_train());
        let n_ctx =
            std::num::NonZeroU32::new(n_ctx).ok_or(ProviderError::Backend("n_ctx 0".into()))?;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(n_ctx))
            .with_embeddings(true)
            .with_pooling_type(LlamaPoolingType::Mean);
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
        &[Capabilities::Chat, Capabilities::Embeddings]
    }

    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        match (model, need) {
            (ModelRef::InstalledPackage { package_id }, Capabilities::Chat)
            | (ModelRef::InstalledPackage { package_id }, Capabilities::Embeddings) => {
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
        let params = llama_cpp_2::model::params::LlamaModelParams::default();
        let model = LlamaModel::load_from_file(self.backend, path, &params)
            .map_err(|e| ProviderError::Backend(format!("model load: {e}")))?;
        self.loaded
            .lock()
            .unwrap()
            .insert(package_id.clone(), Arc::new(model));
        Ok(())
    }

    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
        if let ModelRef::InstalledPackage { package_id } = model {
            self.loaded.lock().unwrap().remove(package_id);
        }
        Ok(())
    }

    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let never = AtomicBool::new(false);
        self.generate_cancellable(req, &never, None)
    }

    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        harbor_security::policy::ExecutionLocation::OnDevice
    }
}
