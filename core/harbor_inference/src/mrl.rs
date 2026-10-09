//! Matryoshka (MRL) truncation as a provider wrapper.
//!
//! EmbeddingGemma 2 is trained so that the leading 512/256/128 of its
//! 768 dimensions are themselves a usable embedding, *provided the
//! shortened vector is re-normalized* (the model card is explicit that
//! skipping this degrades ranking silently). Wrapping the provider keeps
//! truncation in exactly one place: retrieval, memory lookup and skill
//! routing all embed through the same wrapper, so queries and documents
//! can never disagree on dimension.

use crate::provider::{
    Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError,
};

/// Dimensions EmbeddingGemma 2 supports (native first).
pub const GEMMA_MRL_DIMENSIONS: [usize; 4] = [768, 512, 256, 128];

/// Keep the leading `dim` components and L2-normalize. Errors on a
/// request longer than the vector, or a degenerate (zero / non-finite)
/// result — never silently returns a bad vector.
pub fn truncate_normalize(v: &[f32], dim: usize) -> Result<Vec<f32>, ProviderError> {
    if dim == 0 || dim > v.len() {
        return Err(ProviderError::Backend(format!(
            "cannot truncate a {}-d embedding to {dim}",
            v.len()
        )));
    }
    let head = &v[..dim];
    let norm = head.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return Err(ProviderError::Backend(
            "truncated embedding is degenerate".into(),
        ));
    }
    Ok(head.iter().map(|x| x / norm).collect())
}

/// Wraps any embedding provider and serves truncated, re-normalized
/// vectors. Everything except `embed` delegates.
pub struct TruncatedEmbedder<'a> {
    inner: &'a dyn ModelProvider,
    dim: Option<usize>,
}

impl<'a> TruncatedEmbedder<'a> {
    pub fn new(inner: &'a dyn ModelProvider, dim: usize) -> Self {
        TruncatedEmbedder {
            inner,
            dim: Some(dim),
        }
    }

    /// Native dimension: `embed` passes through untouched, so call sites
    /// need one code path whether or not truncation is configured.
    pub fn with_dim(inner: &'a dyn ModelProvider, dim: Option<usize>) -> Self {
        TruncatedEmbedder { inner, dim }
    }
}

impl ModelProvider for TruncatedEmbedder<'_> {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn capabilities(&self) -> &'static [Capabilities] {
        self.inner.capabilities()
    }
    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        self.inner.supports(model, need)
    }
    fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
        self.inner.load(model)
    }
    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
        self.inner.unload(model)
    }
    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.inner.generate(req)
    }
    fn embed(&self, model: &ModelRef, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        let full = self.inner.embed(model, texts)?;
        match self.dim {
            None => Ok(full),
            Some(dim) => full.iter().map(|v| truncate_normalize(v, dim)).collect(),
        }
    }
    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        self.inner.execution_location()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TestBackend;

    #[test]
    fn truncation_keeps_the_head_and_renormalizes() {
        let v = [3.0, 4.0, 12.0, 0.0];
        let t = truncate_normalize(&v, 2).unwrap();
        assert_eq!(t, vec![0.6, 0.8]);
        let n: f32 = t.iter().map(|x| x * x).sum();
        assert!((n - 1.0).abs() < 1e-6, "unit length after truncation");
    }

    #[test]
    fn refuses_growth_zero_and_degenerate() {
        assert!(truncate_normalize(&[1.0, 2.0], 3).is_err());
        assert!(truncate_normalize(&[1.0, 2.0], 0).is_err());
        assert!(truncate_normalize(&[0.0, 0.0, 5.0], 2).is_err());
        assert!(truncate_normalize(&[f32::NAN, 1.0], 2).is_err());
    }

    #[test]
    fn wrapper_truncates_through_the_provider_contract() {
        let base = TestBackend::default();
        let model = ModelRef::InstalledPackage {
            package_id: "m".into(),
        };
        let full = base.embed(&model, &["hello".to_string()]).unwrap();
        let cut = TruncatedEmbedder::new(&base, 4);
        let out = cut.embed(&model, &["hello".to_string()]).unwrap();
        assert_eq!(out[0].len(), 4);
        assert!(full[0].len() > 4);
        assert!(cut.supports(&model, &Capabilities::Embeddings));
    }
}
