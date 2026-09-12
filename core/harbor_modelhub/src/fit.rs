//! Fit Score: device-specific model suitability, expressed in user
//! language (Excellent / Good / Limited / Too large / Unsupported).
//!
//! Evaluates: architecture, physical + available RAM, GPU/NPU capability,
//! backend support, format, parameter count, quantization, context target,
//! KV-cache requirements, multimodal dependencies and expected peak
//! memory. The score NEVER recommends a model solely because it could
//! physically be downloaded.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitBand {
    Unsupported,
    TooLarge,
    Limited,
    Good,
    Excellent,
}

impl FitBand {
    pub fn as_str(&self) -> &'static str {
        match self {
            FitBand::Unsupported => "Unsupported",
            FitBand::TooLarge => "Too large",
            FitBand::Limited => "Limited",
            FitBand::Good => "Good",
            FitBand::Excellent => "Excellent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thermal {
    Normal,
    Reduced,
    Critical,
}

/// What the runtime can execute on this device.
#[derive(Debug, Clone)]
pub struct DeviceProfile {
    pub architecture: String,
    pub physical_ram: u64,
    pub available_ram: u64,
    /// Accelerator present (Metal / CUDA / Vulkan / ANE-class).
    pub gpu_backend: Option<String>,
    pub accelerated_gguf_supported: bool,
    pub thermal: Thermal,
}

/// What the model needs.
#[derive(Debug, Clone)]
pub struct ModelFootprint {
    pub format: String,
    pub weights_bytes: u64,
    /// Estimated peak memory for weights + compute buffers at default ctx.
    pub peak_memory_bytes: u64,
    /// KV cache bytes per 1k context tokens at this quantization.
    pub kv_cache_per_1k_tokens: u64,
    pub context_tokens: u64,
    pub quantization: String,
    pub multimodal: bool,
    /// "gguf/llama.cpp" or a system-managed provider id.
    pub runtime_kind: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitScore {
    pub band: FitBand,
    pub reasons: Vec<String>,
    pub estimated_peak_bytes: u64,
}

impl FitScore {
    pub fn evaluate(device: &DeviceProfile, model: &ModelFootprint) -> FitScore {
        let mut reasons = Vec::new();
        if model.format != "GGUF" && model.runtime_kind.starts_with("gguf/") {
            reasons.push("format not supported by this runtime".into());
            return FitScore { band: FitBand::Unsupported, reasons, estimated_peak_bytes: model.peak_memory_bytes };
        }
        if model.multimodal && device.gpu_backend.is_none() && model.runtime_kind.starts_with("gguf/") {
            reasons.push("multimodal without accelerator is slow".into());
        }
        // KV cache at the requested context.
        let kv = model.kv_cache_per_1k_tokens * model.context_tokens.div_ceil(1000);
        let peak = model.peak_memory_bytes.saturating_add(kv);
        // Never plan >60% of available RAM for inference.
        let budget = (device.available_ram as f64 * 0.6) as u64;
        let physical_limit = (device.physical_ram as f64 * 0.7) as u64;
        if model.format != "GGUF" {
            reasons.push("unsupported model format".into());
            return FitScore { band: FitBand::Unsupported, reasons, estimated_peak_bytes: peak };
        }
        if peak > physical_limit {
            reasons.push("exceeds physical memory even with nothing else running".into());
            return FitScore { band: FitBand::TooLarge, reasons, estimated_peak_bytes: peak };
        }
        let thermal_note = match device.thermal {
            Thermal::Critical => Some("device thermally throttled".into()),
            Thermal::Reduced => Some("reduced thermal headroom".into()),
            Thermal::Normal => None,
        };
        let band = if peak <= budget / 2 {
            if device.accelerated_gguf_supported && thermal_note.is_none() {
                FitBand::Excellent
            } else {
                reasons.extend(thermal_note.clone());
                FitBand::Good
            }
        } else if peak <= budget * 3 / 4 {
            reasons.extend(thermal_note.clone());
            FitBand::Good
        } else if peak <= budget {
            reasons.push("close to the memory comfort limit at this context".into());
            reasons.extend(thermal_note);
            FitBand::Limited
        } else {
            reasons.push("would exhaust available memory at this context".into());
            FitBand::TooLarge
        };
        if let Some(gpu) = &device.gpu_backend {
            if !device.accelerated_gguf_supported {
                reasons.push(format!("{gpu} present but runtime build lacks acceleration"));
            }
        } else {
            reasons.push("CPU-only execution".into());
        }
        if model.quantization == "Q8_0" || model.quantization == "f16" || model.quantization == "bf16" {
            reasons.push("high-precision quantization increases memory and slows inference".into());
        }
        FitScore { band, reasons, estimated_peak_bytes: peak }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    fn phone() -> DeviceProfile {
        DeviceProfile {
            architecture: "arm64".into(),
            physical_ram: 8 * GB,
            available_ram: 4 * GB,
            gpu_backend: Some("Metal".into()),
            accelerated_gguf_supported: true,
            thermal: Thermal::Normal,
        }
    }

    fn gguf(params_b: f64, q: &str) -> ModelFootprint {
        let per = match q {
            "Q4_K_M" => 0.56,
            "Q8_0" => 1.06,
            "f16" => 2.06,
            _ => 0.6,
        };
        let weights = (params_b * 1024.0 * 1024.0 * 1024.0 * per) as u64;
        ModelFootprint {
            format: "GGUF".into(),
            weights_bytes: weights,
            peak_memory_bytes: weights + weights / 8,
            kv_cache_per_1k_tokens: 8 * 1024 * 1024,
            context_tokens: 8192,
            quantization: q.into(),
            multimodal: false,
            runtime_kind: "gguf/llama.cpp".into(),
        }
    }

    #[test]
    fn small_model_on_phone_is_excellent() {
        let s = FitScore::evaluate(&phone(), &gguf(1.5, "Q4_K_M"));
        assert_eq!(s.band, FitBand::Excellent, "reasons: {:?}", s.reasons);
    }

    #[test]
    fn big_model_on_phone_is_too_large() {
        let s = FitScore::evaluate(&phone(), &gguf(70.0, "Q4_K_M"));
        assert_eq!(s.band, FitBand::TooLarge);
        assert!(s.reasons.iter().any(|r| r.contains("physical memory")));
    }

    #[test]
    fn mid_model_lands_in_good_or_limited() {
        // 7B @ Q4_K_M ~ 4.8 GB peak: genuinely too large for an 8 GB phone
        // (the result must be TooLarge there), Limited on a 16 GB device
        // with 8 GB free.
        assert_eq!(FitScore::evaluate(&phone(), &gguf(7.0, "Q4_K_M")).band, FitBand::TooLarge);
        let mut big = phone();
        big.physical_ram = 16 * GB;
        big.available_ram = 8 * GB;
        let s = FitScore::evaluate(&big, &gguf(7.0, "Q4_K_M"));
        assert!(matches!(s.band, FitBand::Good | FitBand::Limited), "got {:?}", s.band);
    }

    #[test]
    fn thermal_throttle_caps_excellent() {
        let mut d = phone();
        d.thermal = Thermal::Reduced;
        let s = FitScore::evaluate(&d, &gguf(1.0, "Q4_K_M"));
        assert!(matches!(s.band, FitBand::Good | FitBand::Limited));
        assert!(s.reasons.iter().any(|r| r.contains("thermal")));
    }

    #[test]
    fn huge_context_can_downgrade_band() {
        let mut m = gguf(1.5, "Q4_K_M");
        m.context_tokens = 8 * 1024 * 1024;
        m.kv_cache_per_1k_tokens = 256 * 1024 * 1024;
        let s = FitScore::evaluate(&phone(), &m);
        assert!(matches!(s.band, FitBand::Limited | FitBand::TooLarge));
    }

    #[test]
    fn non_gguf_is_unsupported_for_gguf_runtime() {
        let mut m = gguf(1.0, "f16");
        m.format = "SAFETENSORS".to_string();
        let s = FitScore::evaluate(&phone(), &m);
        assert_eq!(s.band, FitBand::Unsupported);
    }
}
