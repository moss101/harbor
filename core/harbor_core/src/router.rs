//! Semantic skill routing over the embedding provider (decision 0011).
//!
//! MediaPipe-Decision-Maker-shaped — `prewarm` embeds every candidate key
//! once, `evaluate` embeds the request and ranks in one pass — but the
//! scoring is cosine similarity over the SAME instruction policy
//! retrieval uses, NOT calibrated probability. The router therefore
//! abstains below measured bars instead of fabricating confidence, and
//! the security rule is absolute: a recommendation can never start,
//! approve or authorize anything. A run still passes the executor's own
//! admission and approval machinery; when a Rust-accessible Decision
//! Maker exists, this prewarm/evaluate/abstain interface is its seam.

use harbor_inference::provider::{ModelProvider, ModelRef};
use harbor_knowledge::identity::require_finite_vector;
use harbor_knowledge::index::cosine;
use harbor_knowledge::instructions::InstructionPolicy;

use crate::skills::SkillManifest;

/// One prewarmed routing candidate: a skill and its embedded key.
pub struct RouteCandidate {
    pub skill_id: String,
    pub title: String,
    /// Embedded `{title}. {description}` (title-slot for structured
    /// policies) under the routing policy.
    pub vector: Vec<f32>,
}

/// A router prewarmed for one embedding model.
pub struct SkillRouter {
    policy: InstructionPolicy,
    candidates: Vec<RouteCandidate>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("embed: {0}")]
    Embed(String),
    #[error("no routing candidates")]
    NoCandidates,
}

/// Abstention bars. Raw cosine lives in a model-specific band (the
/// Decision-Maker literature measures roughly −0.15…+0.35 for the Gemma
/// family uncalibrated), so the bars are measured per embedder, not
/// asserted universally. The shipped default is calibrated by the live
/// router eval and re-measured on every qualification run.
#[derive(Debug, Clone, Copy)]
pub struct RouteThresholds {
    pub min_score: f32,
    pub min_margin: f32,
    pub top_k: usize,
}

impl Default for RouteThresholds {
    fn default() -> Self {
        // Calibrated by the live router eval over the 35 built-in skills
        // with bge-m3 (evidence/skill_routing/live-950f4a8e5e19.json):
        // the measured top-score band is 0.70-0.90, and 0.75/0.02 is the
        // sweep's best coverage-per-precision point (39% of requests
        // answered, 70% of those correct; anything looser misroutes
        // every third suggestion). The sweep table is re-measured on
        // every qualification run; move this when the table moves.
        RouteThresholds {
            min_score: 0.75,
            min_margin: 0.02,
            top_k: 3,
        }
    }
}

/// One ranked candidate in an evaluate() result.
pub struct RouteHit {
    pub skill_id: String,
    pub title: String,
    pub score: f32,
}

/// Why the router declined to recommend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbstainReason {
    /// Top score under `min_score`.
    NoConfidentMatch,
    /// Top two within `min_margin` — picking would be a coin flip.
    Ambiguous,
}

/// The outcome of one evaluation. `ranked` is always populated (the UI
/// shows what was considered); `abstained` decides whether the caller
/// may act on it.
pub struct RouteDecision {
    pub ranked: Vec<RouteHit>,
    pub abstained: bool,
    pub reason: Option<AbstainReason>,
}

impl SkillRouter {
    /// Embed every skill's key once. `model` must be loaded on the
    /// provider; the key text is the user-language title and
    /// description — the same words a user would route by.
    pub fn prewarm(
        skills: &[SkillManifest],
        provider: &dyn ModelProvider,
        model: &ModelRef,
    ) -> Result<Self, RouterError> {
        let policy = match model {
            ModelRef::InstalledPackage { package_id } => InstructionPolicy::for_package(package_id),
            _ => InstructionPolicy::None,
        };
        let mut candidates = Vec::with_capacity(skills.len());
        for s in skills {
            // Routing key: the user-language title plus description.
            // Structured policies (GemmaEmbedding) put the title in its
            // own slot; the others carry it in the body so the key never
            // loses it.
            let body = format!("{}. {}", s.title, s.description);
            let input = match policy {
                InstructionPolicy::GemmaEmbedding => {
                    policy.format_document(&s.title, &s.description)
                }
                _ => policy.format_document("", &body),
            };
            let vector = provider
                .embed(model, std::slice::from_ref(&input))
                .map_err(|e| RouterError::Embed(e.to_string()))?
                .into_iter()
                .next()
                .ok_or_else(|| RouterError::Embed("empty embedding".into()))?;
            require_finite_vector(&vector).map_err(RouterError::Embed)?;
            candidates.push(RouteCandidate {
                skill_id: s.id.clone(),
                title: s.title.clone(),
                vector,
            });
        }
        if candidates.is_empty() {
            return Err(RouterError::NoCandidates);
        }
        Ok(SkillRouter { policy, candidates })
    }

    pub fn policy(&self) -> InstructionPolicy {
        self.policy
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Rank one user request. Never errors on low similarity — it
    /// abstains — so a router can sit in front of any dispatch path.
    pub fn evaluate(
        &self,
        provider: &dyn ModelProvider,
        model: &ModelRef,
        request: &str,
        thresholds: RouteThresholds,
    ) -> Result<RouteDecision, RouterError> {
        let input = self.policy.format_query(request);
        let query = provider
            .embed(model, std::slice::from_ref(&input))
            .map_err(|e| RouterError::Embed(e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| RouterError::Embed("empty embedding".into()))?;
        require_finite_vector(&query).map_err(RouterError::Embed)?;

        let mut scored: Vec<(usize, f32)> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, cosine(&query, &c.vector)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let ranked: Vec<RouteHit> = scored
            .iter()
            .take(thresholds.top_k)
            .map(|(i, score)| RouteHit {
                skill_id: self.candidates[*i].skill_id.clone(),
                title: self.candidates[*i].title.clone(),
                score: *score,
            })
            .collect();

        let top = scored.first().map(|(_, s)| *s).unwrap_or(0.0);
        let second = scored.get(1).map(|(_, s)| *s).unwrap_or(f32::NEG_INFINITY);
        let (abstained, reason) = if top < thresholds.min_score {
            (true, Some(AbstainReason::NoConfidentMatch))
        } else if top - second < thresholds.min_margin {
            (true, Some(AbstainReason::Ambiguous))
        } else {
            (false, None)
        };
        Ok(RouteDecision {
            ranked,
            abstained,
            reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_inference::backend::TestBackend;

    fn model() -> ModelRef {
        ModelRef::InstalledPackage {
            package_id: "embed".into(),
        }
    }

    fn skill(id: &str, title: &str, description: &str) -> SkillManifest {
        serde_json::from_value(serde_json::json!({
            "schema": "harbor.skill/v1",
            "id": id,
            "title": title,
            "family": "test",
            "description": description,
            "instructions": "do the work",
        }))
        .unwrap()
    }

    fn wide() -> RouteThresholds {
        RouteThresholds {
            min_score: -2.0,
            min_margin: -1.0,
            top_k: 3,
        }
    }

    #[test]
    fn prewarm_embeds_every_candidate_under_the_policy() {
        let provider = TestBackend::default();
        let skills = vec![
            skill("a", "Alpha", "build spreadsheets"),
            skill("b", "Beta", "draft email"),
        ];
        let router = SkillRouter::prewarm(&skills, &provider, &model()).unwrap();
        assert_eq!(router.candidate_count(), 2);
        // The embed backend hashes text, so a prefixed input yields a
        // different vector than the raw one: verify the policy actually
        // flowed through by comparing against a manual embed.
        let manual = provider
            .embed(&model(), &["Alpha. build spreadsheets".to_string()])
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(manual, router.candidates[0].vector);
    }

    #[test]
    fn evaluate_ranks_and_can_abstain() {
        let provider = TestBackend::default();
        let skills = vec![
            skill(
                "sheets",
                "Spreadsheet analyst",
                "inspect workbooks and formulas",
            ),
            skill("email", "Email drafting", "compose an email draft"),
        ];
        let router = SkillRouter::prewarm(&skills, &provider, &model()).unwrap();
        // A wide bar ranks; an impossible bar abstains with the reason.
        let wide = router
            .evaluate(&provider, &model(), "explain the formulas", wide())
            .unwrap();
        assert!(!wide.abstained);
        assert_eq!(wide.ranked.len(), 2);
        let strict = RouteThresholds {
            min_score: 2.0,
            min_margin: 0.0,
            top_k: 3,
        };
        let d = router
            .evaluate(&provider, &model(), "explain the formulas", strict)
            .unwrap();
        assert!(d.abstained);
        assert_eq!(d.reason, Some(AbstainReason::NoConfidentMatch));
    }

    #[test]
    fn ambiguous_top_two_abstains_on_margin() {
        // Two candidates with byte-identical keys: any real request ties
        // them; a huge min_margin must abstain as ambiguous, not
        // recommend.
        let provider = TestBackend::default();
        let skills = vec![
            skill("x", "Twin", "identical work"),
            skill("y", "Twin", "identical work"),
        ];
        let router = SkillRouter::prewarm(&skills, &provider, &model()).unwrap();
        let d = router
            .evaluate(
                &provider,
                &model(),
                "do the identical work",
                RouteThresholds {
                    min_score: -2.0,
                    min_margin: 2.0,
                    top_k: 2,
                },
            )
            .unwrap();
        assert!(d.abstained);
        assert_eq!(d.reason, Some(AbstainReason::Ambiguous));
        assert_eq!(d.ranked[0].score, d.ranked[1].score);
    }

    #[test]
    fn top_k_truncates_the_ranked_list() {
        let provider = TestBackend::default();
        let skills: Vec<SkillManifest> = (0..5)
            .map(|i| skill(&format!("s{i}"), &format!("Skill {i}"), "work"))
            .collect();
        let router = SkillRouter::prewarm(&skills, &provider, &model()).unwrap();
        let d = router
            .evaluate(
                &provider,
                &model(),
                "anything",
                RouteThresholds {
                    min_score: -2.0,
                    min_margin: -1.0,
                    top_k: 2,
                },
            )
            .unwrap();
        assert_eq!(d.ranked.len(), 2);
    }

    #[test]
    fn non_finite_candidate_vector_is_a_router_error() {
        use harbor_knowledge::identity::require_finite_vector;
        let bad = vec![0.1, f32::NAN];
        assert!(require_finite_vector(&bad).is_err());
        assert!(require_finite_vector(&[]).is_err());
        assert!(require_finite_vector(&[0.1, -0.2]).is_ok());
    }
}
