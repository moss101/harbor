//! Deterministic test backend implementing the full provider contract.
//!
//! This is NOT a mocked product capability: it is the executable
//! specification of the provider contract used by higher layers
//! (router, runtime, FFI) for their tests. The production GGUF backend
//! (llama.cpp) and platform providers implement the same trait; its
//! conformance tests run against both.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::provider::{
    Capabilities, ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError, Usage,
};

#[derive(Default)]
pub struct TestBackend {
    loaded: Mutex<BTreeMap<String, ()>>,
    known_packages: Vec<String>,
}

impl TestBackend {
    pub fn with_packages(mut self, ids: &[&str]) -> Self {
        self.known_packages = ids.iter().map(|s| s.to_string()).collect();
        self
    }
}

impl ModelProvider for TestBackend {
    fn id(&self) -> &str {
        "test.backend"
    }

    fn capabilities(&self) -> &'static [Capabilities] {
        &[Capabilities::Chat, Capabilities::Embeddings]
    }

    fn supports(&self, model: &ModelRef, need: &Capabilities) -> bool {
        let ok_model = match model {
            ModelRef::InstalledPackage { package_id } => {
                self.known_packages.is_empty() || self.known_packages.contains(package_id)
            }
            _ => false,
        };
        ok_model && self.capabilities().contains(need)
    }

    fn load(&self, model: &ModelRef) -> Result<(), ProviderError> {
        match model {
            ModelRef::InstalledPackage { package_id } => {
                if !self.known_packages.is_empty() && !self.known_packages.contains(package_id) {
                    return Err(ProviderError::ModelNotFound(package_id.clone()));
                }
                self.loaded.lock().unwrap().insert(package_id.clone(), ());
                Ok(())
            }
            _ => Err(ProviderError::ModelNotFound("unknown ref".into())),
        }
    }

    fn unload(&self, model: &ModelRef) -> Result<(), ProviderError> {
        if let ModelRef::InstalledPackage { package_id } = model {
            self.loaded.lock().unwrap().remove(package_id);
        }
        Ok(())
    }

    fn generate(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        for need in &req.requires {
            if !self.supports(&req.model, need) {
                return Err(ProviderError::UnsupportedCapability(need.as_str()));
            }
        }
        let package = match &req.model {
            ModelRef::InstalledPackage { package_id } => package_id.clone(),
            _ => return Err(ProviderError::ModelNotFound("unknown ref".into())),
        };
        let prompt = req
            .messages
            .last()
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(ChatResponse {
            content: format!("[{package}] echo: {prompt}"),
            usage: Usage {
                prompt_tokens: prompt.len() as u64 / 4,
                completion_tokens: 8,
            },
            executed_on: package,
            execution_location: harbor_security::policy::ExecutionLocation::OnDevice,
        })
    }

    fn embed(&self, model: &ModelRef, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        if !self.supports(model, &Capabilities::Embeddings) {
            return Err(ProviderError::UnsupportedCapability("embeddings"));
        }
        // Deterministic hashed embedding: stable across runs for index
        // identity tests. Not a product-quality embedding.
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0f32; 8];
                for (i, b) in t.as_bytes().iter().enumerate() {
                    v[i % 8] += *b as f32;
                }
                v
            })
            .collect())
    }

    fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
        harbor_security::policy::ExecutionLocation::OnDevice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_canonical::JsonValue;

    #[test]
    fn capability_declarations_are_enforced() {
        let b = TestBackend::default().with_packages(&["pkg-1"]);
        let req = ChatRequest {
            model: ModelRef::InstalledPackage {
                package_id: "pkg-1".into(),
            },
            messages: vec![JsonValue::object([
                ("role", JsonValue::str("user")),
                ("content", JsonValue::str("hi")),
            ])],
            max_tokens: 16,
            temperature: 0.2,
            requires: vec![Capabilities::Vision],
            response_schema: None,
            trace_key: None,
        };
        assert!(matches!(
            b.generate(req),
            Err(ProviderError::UnsupportedCapability("vision"))
        ));
    }

    #[test]
    fn load_unload_lifecycle_and_missing_model() {
        let b = TestBackend::default().with_packages(&["pkg-1"]);
        let ok = ModelRef::InstalledPackage {
            package_id: "pkg-1".into(),
        };
        let missing = ModelRef::InstalledPackage {
            package_id: "nope".into(),
        };
        b.load(&ok).unwrap();
        b.unload(&ok).unwrap();
        assert!(matches!(
            b.load(&missing),
            Err(ProviderError::ModelNotFound(_))
        ));
    }

    #[test]
    fn embed_deterministic() {
        let b = TestBackend::default();
        let m = ModelRef::InstalledPackage {
            package_id: "e".into(),
        };
        let v1 = b.embed(&m, &["hello".into()]).unwrap();
        let v2 = b.embed(&m, &["hello".into()]).unwrap();
        assert_eq!(v1, v2);
    }
}
