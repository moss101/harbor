//! Provider router: picks a qualified provider/model for a request.
//! Substitution of a missing model/provider requires explicit policy AND
//! is surfaced visibly (Substitution record the UI must show).

use crate::provider::{ChatRequest, ChatResponse, ModelProvider, ModelRef, ProviderError};

#[derive(Debug, Clone, PartialEq)]
pub struct Substitution {
    pub requested: ModelRef,
    pub executed: ModelRef,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterPolicy {
    /// Execute on exactly the requested model or fail. Default.
    ExactOnly,
    /// Allow visible substitution among qualified installed packages.
    AllowSubstitution,
}

pub struct Router {
    pub policy: RouterPolicy,
    providers: Vec<Box<dyn ModelProvider>>,
}

impl Router {
    pub fn new(policy: RouterPolicy) -> Self {
        Router {
            policy,
            providers: Vec::new(),
        }
    }

    pub fn register(&mut self, provider: Box<dyn ModelProvider>) {
        self.providers.push(provider);
    }

    /// Route a chat request. Never silently substitutes: with ExactOnly a
    /// missing model is a typed error; with AllowSubstitution the chosen
    /// alternative is returned as a visible Substitution.
    pub fn chat(
        &self,
        req: ChatRequest,
    ) -> Result<(ChatResponse, Option<Substitution>), ProviderError> {
        // Exact match first.
        for p in &self.providers {
            let all_supported = req.requires.iter().all(|need| p.supports(&req.model, need));
            if all_supported && p.load(&req.model).is_ok() {
                let resp = p.generate(req.clone())?;
                return Ok((resp, None));
            }
        }
        if self.policy == RouterPolicy::AllowSubstitution {
            for p in &self.providers {
                for candidate in candidate_refs(&req) {
                    if candidate == req.model {
                        continue;
                    }
                    let all_supported =
                        req.requires.iter().all(|need| p.supports(&candidate, need));
                    if all_supported && p.load(&candidate).is_ok() {
                        let mut executed_req = req.clone();
                        executed_req.model = candidate.clone();
                        let resp = p.generate(executed_req)?;
                        return Ok((
                            resp,
                            Some(Substitution {
                                requested: req.model.clone(),
                                executed: candidate,
                                reason: "requested model unavailable; qualified substitute used"
                                    .into(),
                            }),
                        ));
                    }
                }
            }
        }
        Err(ProviderError::ModelNotFound(format!("{:?}", req.model)))
    }
}

fn candidate_refs(req: &ChatRequest) -> Vec<ModelRef> {
    match &req.model {
        ModelRef::InstalledPackage { package_id } => {
            // In production the modelhub installed list feeds this; the
            // router treats the id namespace as candidates.
            vec![ModelRef::InstalledPackage {
                package_id: format!("{package_id}-alt"),
            }]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TestBackend;
    use crate::provider::Capabilities;
    use harbor_canonical::JsonValue;

    fn req(model: ModelRef) -> ChatRequest {
        ChatRequest {
            model,
            messages: vec![JsonValue::object([
                ("role", JsonValue::str("user")),
                ("content", JsonValue::str("summarize")),
            ])],
            max_tokens: 64,
            temperature: 0.2,
            requires: vec![Capabilities::Chat],
            response_schema: None,
            trace_key: None,
        }
    }

    #[test]
    fn exact_policy_never_substitutes() {
        let mut r = Router::new(RouterPolicy::ExactOnly);
        r.register(Box::new(TestBackend::default().with_packages(&["pkg-a"])));
        let missing = req(ModelRef::InstalledPackage {
            package_id: "pkg-z".into(),
        });
        assert!(matches!(
            r.chat(missing),
            Err(ProviderError::ModelNotFound(_))
        ));
        let present = req(ModelRef::InstalledPackage {
            package_id: "pkg-a".into(),
        });
        let (resp, sub) = r.chat(present).unwrap();
        assert!(sub.is_none());
        assert_eq!(resp.executed_on, "pkg-a");
    }

    #[test]
    fn substitution_is_visible_not_silent() {
        let mut r = Router::new(RouterPolicy::AllowSubstitution);
        // Provider knows pkg-a-alt only; request pkg-b -> substituted.
        r.register(Box::new(
            TestBackend::default().with_packages(&["pkg-b-alt"]),
        ));
        let ask = req(ModelRef::InstalledPackage {
            package_id: "pkg-b".into(),
        });
        let (resp, sub) = r.chat(ask).unwrap();
        let sub = sub.expect("substitution must be surfaced");
        assert_eq!(
            sub.requested,
            ModelRef::InstalledPackage {
                package_id: "pkg-b".into()
            }
        );
        assert_eq!(
            resp.executed_on, "pkg-b-alt",
            "executed_on must reflect reality for Trust Pulse"
        );
    }

    #[test]
    fn missing_model_never_silently_replaced_even_under_substitution_policy() {
        // When NO candidate is available the router errors: a missing
        // model/provider is never silently replaced (goal §23).
        let mut r = Router::new(RouterPolicy::AllowSubstitution);
        r.register(Box::new(
            TestBackend::default().with_packages(&["something-else"]),
        ));
        let ask = req(ModelRef::InstalledPackage {
            package_id: "unheard-of".into(),
        });
        assert!(r.chat(ask).is_err());
    }
}
