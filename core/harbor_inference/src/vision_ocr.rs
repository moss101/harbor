//! OCR with a vision-language model (decision 0019): the same GGUF runtime
//! that runs the chat models reads text off an image, so every platform that
//! can run a model has an OCR engine - no platform API, no cloud call.
//!
//! The model is told to transcribe, nothing else. Its output is only ever
//! TEXT for the checks that follow (field grounding, numeric verification);
//! it decides nothing. Confidence is the mean probability of the tokens it
//! chose, so a page it was unsure about is flagged for a human look.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
use llama_cpp_2::mtmd::{
    mtmd_default_marker, MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText,
};
use llama_cpp_2::sampling::LlamaSampler;

use crate::gguf::GgufLlamaCppProvider;
use crate::ocr::{OcrEngine, OcrError, OcrImage, OcrOutput, OcrPage};

/// Upper bound on transcribed tokens per page.
const MAX_NEW_TOKENS: usize = 1536;
const MAX_CTX: u32 = 12_288;
const N_BATCH: i32 = 512;

const INSTRUCTION: &str = "Transcribe all the text in this image exactly as written, line by line, in reading order. Keep numbers, dates, codes and punctuation exactly. Output only the text, with no commentary.";

pub struct VisionOcr {
    model: Arc<LlamaModel>,
    mtmd: MtmdContext,
    backend: &'static llama_cpp_2::llama_backend::LlamaBackend,
    package: String,
    guard: Mutex<()>,
}

fn err(m: impl std::fmt::Display) -> OcrError {
    OcrError::Failed(m.to_string())
}

impl VisionOcr {
    /// Open the vision tower of an installed, LOADED package (weights +
    /// `mmproj`). Typed error when the package has no vision projector.
    pub fn open(provider: &GgufLlamaCppProvider, package_id: &str) -> Result<Self, OcrError> {
        let model = provider
            .loaded_model(package_id)
            .ok_or_else(|| err(format!("{package_id} is not loaded")))?;
        let mmproj = provider
            .mmproj_file(package_id)
            .map_err(|e| err(format!("{package_id} has no vision projector: {e}")))?;
        let params = MtmdContextParams {
            use_gpu: !provider.is_cpu_pinned(package_id)
                && std::env::var("HARBOR_GGUF_CPU_ONLY").as_deref() != Ok("1"),
            n_threads: crate::gguf::compute_threads(),
            ..MtmdContextParams::default()
        };
        let mtmd = MtmdContext::init_from_file(
            mmproj.to_str().ok_or_else(|| err("non-utf8 mmproj path"))?,
            &model,
            &params,
        )
        .map_err(|e| err(format!("projector: {e}")))?;
        if !mtmd.support_vision() {
            return Err(err(format!("{package_id} has no vision tower")));
        }
        Ok(VisionOcr {
            model,
            mtmd,
            backend: provider.backend(),
            package: package_id.to_string(),
            guard: Mutex::new(()),
        })
    }

    /// Open and VERIFY: read a small synthetic image once; if the backend
    /// produces non-finite scores (a GPU path that loads fine and computes
    /// wrongly - measured on Metal with Qwen2.5-VL), pin the package to the
    /// CPU, reload and reopen. Returns the engine and which backend it runs on
    /// (`"gpu"`, `"cpu_fallback"`).
    pub fn open_verified(
        provider: &GgufLlamaCppProvider,
        model: &crate::provider::ModelRef,
        package_id: &str,
    ) -> Result<(Self, &'static str), OcrError> {
        use crate::provider::ModelProvider as _;
        let cancel = AtomicBool::new(false);
        let canary = |e: &VisionOcr| -> Result<(), OcrError> {
            // 96x96: white page, a black bar and a black block.
            let (w, h) = (96u32, 96u32);
            let mut px = vec![255u8; (w * h * 3) as usize];
            for y in 0..h {
                for x in 0..w {
                    let ink = (y > 20 && y < 30 && x > 10 && x < 80)
                        || (y > 50 && x > 40 && x < 56 && y < 70);
                    if ink {
                        let i = ((y * w + x) * 3) as usize;
                        px[i..i + 3].copy_from_slice(&[0, 0, 0]);
                    }
                }
            }
            e.recognize_image(
                OcrImage::Rgb {
                    width: w,
                    height: h,
                    data: &px,
                },
                &[],
                &cancel,
            )
            .map(|_| ())
        };
        let first = Self::open(provider, package_id)?;
        match canary(&first) {
            Ok(()) => Ok((first, "gpu")),
            Err(OcrError::Failed(m)) if m.contains("non-finite") => {
                drop(first);
                provider.force_cpu(package_id);
                provider
                    .load(model)
                    .map_err(|e| err(format!("reload on CPU: {e}")))?;
                let second = Self::open(provider, package_id)?;
                canary(&second)?;
                Ok((second, "cpu_fallback"))
            }
            Err(e) => Err(e),
        }
    }

    fn read(&self, bitmap: MtmdBitmap, cancel: &AtomicBool) -> Result<(String, f64), OcrError> {
        let _g = self.guard.lock().unwrap();
        let marker = mtmd_default_marker();
        let content = format!("{marker}\n{INSTRUCTION}");
        let chat = vec![LlamaChatMessage::new("user".into(), content.clone()).map_err(err)?];
        let prompt = match self.model.chat_template(None) {
            Ok(t) => self
                .model
                .apply_chat_template(&t, &chat, true)
                .map_err(err)?,
            Err(_) => format!("User: {content}\nAssistant:"),
        };
        let chunks = self
            .mtmd
            .tokenize(
                MtmdInputText {
                    text: prompt,
                    add_special: true,
                    parse_special: true,
                },
                &[&bitmap],
            )
            .map_err(|e| err(format!("tokenize: {e}")))?;
        // KV cells are TOKENS (M-RoPE models use fewer positions than tokens),
        // and an image chunk must fit one decode batch.
        let n_tokens = chunks.total_tokens() as u32;
        let widest = (0..chunks.len())
            .filter_map(|i| chunks.get(i))
            .map(|c| c.n_tokens() as u32)
            .max()
            .unwrap_or(0);
        let need = n_tokens + MAX_NEW_TOKENS as u32 + 64;
        let n_ctx = need
            .next_multiple_of(256)
            .min(MAX_CTX)
            .min(self.model.n_ctx_train().max(2048));
        if n_tokens + 32 > n_ctx {
            return Err(err(format!(
                "the image needs {n_tokens} tokens; the context allows {n_ctx}"
            )));
        }
        let n_batch = widest.max(N_BATCH as u32).min(n_ctx);
        let params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(n_ctx))
            .with_n_batch(n_batch)
            .with_n_ubatch(n_batch)
            .with_n_threads(crate::gguf::compute_threads())
            .with_n_threads_batch(crate::gguf::compute_threads());
        let mut ctx = self.model.new_context(self.backend, params).map_err(err)?;
        let mut n_past = chunks
            .eval_chunks(&self.mtmd, &ctx, 0, 0, n_batch as i32, true)
            .map_err(|e| err(format!("image eval: {e}")))?;
        let mut sampler = LlamaSampler::chain_simple(vec![LlamaSampler::greedy()]);
        let mut text = String::new();
        let (mut prob_sum, mut prob_n, mut n) = (0.0f64, 0usize, 0usize);
        let mut logits_index: i32 = -1;
        let mut degenerate = false;
        while n < MAX_NEW_TOKENS {
            if cancel.load(Ordering::Relaxed) {
                return Err(OcrError::Cancelled);
            }
            let tok = sampler.sample(&ctx, logits_index);
            if self.model.is_eog_token(tok) {
                break;
            }
            // Probability of the chosen (argmax) token under the softmax.
            // The first token's logits come from the C eval helper, which the
            // safe wrapper does not track, so it is left out of the mean.
            if logits_index == 0 {
                let logits = ctx.get_logits_ith(0);
                let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let z: f64 = logits.iter().map(|l| f64::from((l - max).exp())).sum();
                if !z.is_finite() || max.is_nan() {
                    return Err(err(
                        "the model produced non-finite scores on this backend (numerics); \
                         refusing its output",
                    ));
                }
                prob_sum += 1.0 / z;
                prob_n += 1;
            }
            n += 1;
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            text.push_str(
                &self
                    .model
                    .token_to_piece(tok, &mut decoder, false, None)
                    .map_err(err)?,
            );
            let mut step = LlamaBatch::new(1, 1);
            step.add(tok, n_past, &[0], true).map_err(err)?;
            ctx.decode(&mut step)
                .map_err(|e| err(format!("decode: {e}")))?;
            n_past += 1;
            logits_index = 0;
            // A model that loses the page loops or counts upward. Stop and
            // report zero confidence so the page is flagged, not trusted.
            if n.is_multiple_of(32) && is_looping(&text) {
                degenerate = true;
                break;
            }
        }
        if degenerate {
            return Ok((text.trim().to_string(), 0.0));
        }
        Ok((
            text.trim().to_string(),
            if prob_n == 0 {
                0.0
            } else {
                prob_sum / prob_n as f64
            },
        ))
    }
}

/// True when the tail of `text` already occurred several times before it:
/// the signature of a generation loop, never of a real page.
fn is_looping(text: &str) -> bool {
    counts_upward(text) || repeats_tail(text)
}

/// A run of 8+ consecutive integers ("1001 1002 1003 ...") is a model that
/// lost the page and started counting, not a document.
fn counts_upward(text: &str) -> bool {
    let mut run = 1usize;
    let mut prev: Option<i64> = None;
    for tok in text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|t| !t.is_empty())
    {
        let Ok(n) = tok.parse::<i64>() else {
            prev = None;
            run = 1;
            continue;
        };
        if prev.is_some_and(|p| n == p + 1) {
            run += 1;
            if run >= 8 {
                return true;
            }
        } else {
            run = 1;
        }
        prev = Some(n);
    }
    false
}

fn repeats_tail(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 200 {
        return false;
    }
    let tail: String = chars[chars.len() - 40..].iter().collect();
    text.matches(&tail).count() >= 4
}

impl OcrEngine for VisionOcr {
    fn name(&self) -> String {
        format!("llm-vision:{}", self.package)
    }
    fn languages(&self) -> Vec<String> {
        Vec::new() // multilingual by model; no fixed list to promise
    }
    fn reads_pdf(&self) -> bool {
        false
    }
    fn recognize(
        &self,
        kind: &str,
        bytes: &[u8],
        languages: &[String],
        _max_pages: u32,
        cancel: &AtomicBool,
    ) -> Result<OcrOutput, OcrError> {
        if kind != "image" {
            return Err(err(
                "this engine reads images; the tool splits PDFs into page images",
            ));
        }
        let (text, confidence) =
            self.recognize_image(OcrImage::Encoded(bytes), languages, cancel)?;
        Ok(OcrOutput {
            engine: self.name(),
            pages: vec![OcrPage {
                page: 1,
                text,
                source: "ocr".into(),
                confidence,
            }],
        })
    }
    fn recognize_image(
        &self,
        image: OcrImage<'_>,
        _languages: &[String],
        cancel: &AtomicBool,
    ) -> Result<(String, f64), OcrError> {
        let bitmap = match image {
            OcrImage::Encoded(b) => MtmdBitmap::from_buffer(&self.mtmd, b, false)
                .map_err(|e| err(format!("image decode: {e}")))?,
            OcrImage::Rgb {
                width,
                height,
                data,
            } => MtmdBitmap::from_image_data(width, height, data)
                .map_err(|e| err(format!("image: {e}")))?,
        };
        self.read(bitmap, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::is_looping;

    #[test]
    fn loops_are_detected_and_real_pages_are_not() {
        let looped: String = (1001..1100).map(|n| format!("{n} ")).collect();
        assert!(is_looping(&format!(
            "{looped}{looped}{looped}{looped}{looped}"
        )));
        let page = "Invoice INV-1001 Acme Supplies Issued 2026-08-01, due 2026-08-31 Amount 4,200.00 USD - unpaid. \
                    Invoice INV-1002 Northwind Parts Issued 2026-09-05, due 2026-10-05 Amount 1,850.50 USD - unpaid. \
                    Invoice INV-1003 Acme Supplies Issued 2026-09-20, due 2026-10-20 Amount 900.00 USD - unpaid.";
        assert!(!is_looping(page));
        assert!(!is_looping("short"));
        // Counting upward is a lost page; a few dates and invoice numbers are not.
        assert!(is_looping("1001 1002 1003 1004 1005 1006 1007 1008 1009"));
        assert!(!is_looping(
            "INV-1001 INV-1002 2026-08-01 2026-08-31 4,200.00"
        ));
    }
}
