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
    pub description: String,
    /// Embedded `{title}. {description}` (title-slot for structured
    /// policies) under the routing policy.
    pub vector: Vec<f32>,
}

/// A router prewarmed for one embedding model.
pub struct SkillRouter {
    policy: InstructionPolicy,
    calibration: Option<CalibrationEntry>,
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

/// Benchmark calibration for one embedding model (versioned data in
/// `router_calibration.json`). Bars belong to the embedder, not to
/// whatever chat model the user runs: switching the generative LLM never
/// touches routing, and switching the embedder switches the bars with it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CalibrationEntry {
    pub id: String,
    pub version: u32,
    matches: Vec<String>,
    /// Matryoshka truncation this calibration was measured at; absent =
    /// the model's native dimension. A truncated index is a different
    /// embedder as far as score bands go, so it needs its own entry.
    #[serde(default)]
    pub dimension: Option<u32>,
    pub min_score: f32,
    pub min_margin: f32,
    pub corpus_cases: u32,
    pub coverage: f64,
    pub precision_when_confident: f64,
    pub top1: f64,
    pub evidence: String,
}

#[derive(serde::Deserialize)]
struct CalibrationFile {
    entries: Vec<CalibrationEntry>,
}

impl CalibrationEntry {
    /// The calibration for an installed embedding package, matched by
    /// model-family substring of its id (the instruction-policy rule). None
    /// = uncalibrated: the router ranks but abstains from recommending.
    pub fn for_package(package_id: &str, truncation: Option<u32>) -> Option<CalibrationEntry> {
        static FILE: std::sync::OnceLock<CalibrationFile> = std::sync::OnceLock::new();
        let file = FILE.get_or_init(|| {
            serde_json::from_str(include_str!("router_calibration.json"))
                .expect("router_calibration.json parses (checked by test)")
        });
        let lower = package_id.to_ascii_lowercase();
        file.entries
            .iter()
            .find(|e| {
                e.dimension == truncation && e.matches.iter().any(|m| lower.contains(m.as_str()))
            })
            .cloned()
    }

    pub fn thresholds(&self) -> RouteThresholds {
        RouteThresholds {
            min_score: self.min_score,
            min_margin: self.min_margin,
            top_k: 3,
        }
    }
}

/// One ranked candidate in an evaluate() result.
pub struct RouteHit {
    pub skill_id: String,
    pub title: String,
    pub description: String,
    pub score: f32,
}

/// Why the router declined to recommend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbstainReason {
    /// Top score under `min_score`.
    NoConfidentMatch,
    /// Top two within `min_margin` — picking would be a coin flip.
    Ambiguous,
    /// This embedding model has no benchmark calibration: ranking is
    /// shown, but no recommendation is made.
    Uncalibrated,
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
        Self::prewarm_truncated(skills, provider, model, None)
    }

    /// Prewarm over an embedder that serves Matryoshka-truncated vectors
    /// (`truncation` = the dimension it truncates to, None = native).
    /// `provider` must be that same truncating embedder; the dimension
    /// only selects which benchmark calibration applies.
    pub fn prewarm_truncated(
        skills: &[SkillManifest],
        provider: &dyn ModelProvider,
        model: &ModelRef,
        truncation: Option<u32>,
    ) -> Result<Self, RouterError> {
        let (policy, calibration) = match model {
            ModelRef::InstalledPackage { package_id } => (
                InstructionPolicy::for_package(package_id),
                CalibrationEntry::for_package(package_id, truncation),
            ),
            _ => (InstructionPolicy::None, None),
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
                description: s.description.clone(),
                vector,
            });
        }
        if candidates.is_empty() {
            return Err(RouterError::NoCandidates);
        }
        Ok(SkillRouter {
            policy,
            calibration,
            candidates,
        })
    }

    pub fn policy(&self) -> InstructionPolicy {
        self.policy
    }

    /// The benchmark calibration in force for this embedder, if any.
    pub fn calibration(&self) -> Option<&CalibrationEntry> {
        self.calibration.as_ref()
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Rank with the embedder's own calibration. An uncalibrated embedder
    /// yields a ranking and an `Uncalibrated` abstention — never a
    /// recommendation scored against another model's bars.
    pub fn evaluate_calibrated(
        &self,
        provider: &dyn ModelProvider,
        model: &ModelRef,
        request: &str,
    ) -> Result<RouteDecision, RouterError> {
        match &self.calibration {
            Some(c) => self.evaluate(provider, model, request, c.thresholds()),
            None => {
                let mut d = self.evaluate(
                    provider,
                    model,
                    request,
                    RouteThresholds {
                        min_score: f32::INFINITY,
                        min_margin: 0.0,
                        top_k: RouteThresholds::default().top_k,
                    },
                )?;
                d.abstained = true;
                d.reason = Some(AbstainReason::Uncalibrated);
                Ok(d)
            }
        }
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
                description: self.candidates[*i].description.clone(),
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

/// The outcome of asking the user's selected chat model to choose between
/// candidates the embedder could not separate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disambiguation {
    /// The model picked one of the offered candidates.
    Chose { skill_id: String },
    /// The model judged none of them a fit.
    NoneFit,
    /// No usable answer: the chat model lacks a verified capability, the
    /// call failed, or the answer was not one of the offered ids.
    Unavailable(String),
}

/// Ask a chat model to break a tie between `candidates`. The chat model
/// is whatever the user currently runs behind the [`ModelProvider`]
/// abstraction; nothing here names a model family.
///
/// Safety shape: (1) it only runs when the provider VERIFIES chat support
/// for that model; (2) when the provider can constrain decoding it does,
/// but the answer is validated either way — anything other than an
/// offered id or "none" is discarded; (3) the result is a recommendation
/// that can reorder a suggestion list and nothing else. It never
/// approves, authorizes or starts anything: execution still goes through
/// the executor's deterministic admission and approval gates, whatever
/// any model — embedding or chat — believes.
pub fn disambiguate(
    chat: &dyn ModelProvider,
    chat_model: &ModelRef,
    request: &str,
    candidates: &[RouteHit],
) -> Disambiguation {
    use harbor_inference::provider::{Capabilities, ChatRequest};
    if candidates.len() < 2 {
        return Disambiguation::Unavailable("nothing to disambiguate".into());
    }
    if !chat.supports(chat_model, &Capabilities::Chat) {
        return Disambiguation::Unavailable("chat model does not support chat".into());
    }
    let ids: Vec<&str> = candidates.iter().map(|c| c.skill_id.as_str()).collect();
    let listing: String = candidates
        .iter()
        .map(|c| format!("- {}: {} — {}\n", c.skill_id, c.title, c.description))
        .collect();
    let prompt = format!(
        "Pick the one skill that best fits the user's request, or \"none\" if \
         none of them fits.\n\nSkills:\n{listing}\nRequest: {request}\n\n\
         Answer with JSON only: {{\"skill_id\": \"<id or none>\"}}"
    );
    let mut options: Vec<&str> = ids.clone();
    options.push("none");
    let structured = chat.supports(chat_model, &Capabilities::StructuredOutput);
    let schema = structured.then(|| {
        harbor_canonical::convert(serde_json::json!({
            "type": "object",
            "properties": { "skill_id": { "enum": options } },
            "required": ["skill_id"],
            "additionalProperties": false,
        }))
        .expect("schema is canonical JSON")
    });
    let message = harbor_canonical::JsonValue::object([
        ("role", harbor_canonical::JsonValue::str("user")),
        ("content", harbor_canonical::JsonValue::str(prompt)),
    ]);
    let resp = match chat.generate(ChatRequest {
        model: chat_model.clone(),
        messages: vec![message],
        max_tokens: 48,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
        response_schema: schema,
        trace_key: Some("router/disambiguate".into()),
    }) {
        Ok(r) => r,
        Err(e) => return Disambiguation::Unavailable(format!("chat call failed: {e}")),
    };
    parse_choice(&resp.content, &ids)
}

/// Strict answer validation: the JSON `skill_id`, or a bare id, must be
/// exactly one offered id or "none".
fn parse_choice(content: &str, ids: &[&str]) -> Disambiguation {
    let trimmed = content.trim();
    let candidate: String = serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|v| {
            v.get("skill_id")
                .and_then(|s| s.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            trimmed
                .trim_matches(|c: char| c == '"' || c == '`' || c.is_whitespace())
                .to_string()
        });
    if candidate.eq_ignore_ascii_case("none") {
        Disambiguation::NoneFit
    } else if ids.contains(&candidate.as_str()) {
        Disambiguation::Chose {
            skill_id: candidate,
        }
    } else {
        Disambiguation::Unavailable("answer was not one of the offered skills".into())
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
    fn calibration_is_per_embedder_versioned_data() {
        // The shipped file parses and every entry carries its evidence.
        let m3 = CalibrationEntry::for_package("bge-m3-q8_0", None).unwrap();
        assert_eq!((m3.id.as_str(), m3.version), ("bge-m3", 1));
        let g = CalibrationEntry::for_package("embeddinggemma2-ab12", None).unwrap();
        assert!(g.min_score < m3.min_score, "gemma band sits lower");
        assert!(g.evidence.starts_with("evidence/skill_routing/"));
        assert!(g.corpus_cases >= 100);
        // An embedder nobody benchmarked has NO bars — and the chat model
        // never enters the lookup.
        assert!(CalibrationEntry::for_package("bge-small-en-v1.5", None).is_none());
        // A truncated index is not the native embedder: no entry for a
        // dimension nobody benchmarked.
        assert!(CalibrationEntry::for_package("embeddinggemma2-ab12", Some(64)).is_none());
        // Measured truncations have their own, different bars; 128-d
        // (below the quality floor) has none.
        let t256 = CalibrationEntry::for_package("embeddinggemma2-ab12", Some(256)).unwrap();
        assert_eq!(t256.dimension, Some(256));
        assert!(t256.min_margin > g.min_margin);
        assert!(CalibrationEntry::for_package("embeddinggemma2-ab12", Some(128)).is_none());
        assert!(CalibrationEntry::for_package("qwen2.5-1.5b-instruct", None).is_none());
    }

    #[test]
    fn uncalibrated_embedder_ranks_but_never_recommends() {
        let provider = TestBackend::default();
        let skills = vec![
            skill("a", "Alpha", "build spreadsheets"),
            skill("b", "Beta", "draft email"),
        ];
        // "embed" matches no calibration entry.
        let router = SkillRouter::prewarm(&skills, &provider, &model()).unwrap();
        assert!(router.calibration().is_none());
        let d = router
            .evaluate_calibrated(&provider, &model(), "build spreadsheets")
            .unwrap();
        assert!(d.abstained);
        assert_eq!(d.reason, Some(AbstainReason::Uncalibrated));
        assert_eq!(d.ranked.len(), 2, "the ranking is still shown");
    }

    #[test]
    fn calibrated_embedder_uses_its_own_bars() {
        let provider = TestBackend::default();
        let skills = vec![skill("a", "Alpha", "x"), skill("b", "Beta", "y")];
        let m = ModelRef::InstalledPackage {
            package_id: "bge-m3-q8_0".into(),
        };
        let router = SkillRouter::prewarm(&skills, &provider, &m).unwrap();
        assert_eq!(router.calibration().unwrap().id, "bge-m3");
        // Hash-embedding scores are nowhere near 0.75, so the calibrated
        // bar must abstain as NoConfidentMatch — not Uncalibrated.
        let d = router.evaluate_calibrated(&provider, &m, "zzz").unwrap();
        assert_ne!(d.reason, Some(AbstainReason::Uncalibrated));
    }

    #[test]
    fn disambiguation_accepts_only_offered_ids() {
        let ids = ["sheets", "email"];
        assert_eq!(
            parse_choice(r#"{"skill_id": "email"}"#, &ids),
            Disambiguation::Chose {
                skill_id: "email".into()
            }
        );
        assert_eq!(
            parse_choice("  `sheets`\n", &ids),
            Disambiguation::Chose {
                skill_id: "sheets".into()
            }
        );
        assert_eq!(
            parse_choice(r#"{"skill_id":"None"}"#, &ids),
            Disambiguation::NoneFit
        );
        // A hallucinated or hostile id is discarded, not trusted.
        for bad in [
            r#"{"skill_id": "delete-everything"}"#,
            "email, sheets",
            "I think email.",
            "",
        ] {
            assert!(
                matches!(parse_choice(bad, &ids), Disambiguation::Unavailable(_)),
                "{bad:?} must be rejected"
            );
        }
    }

    /// A chat model stand-in: any family, scripted answer.
    struct ScriptedChat {
        answer: &'static str,
        chat: bool,
    }
    impl ModelProvider for ScriptedChat {
        fn id(&self) -> &str {
            "scripted.chat"
        }
        fn capabilities(&self) -> &'static [harbor_inference::provider::Capabilities] {
            &[]
        }
        fn supports(&self, _m: &ModelRef, need: &harbor_inference::provider::Capabilities) -> bool {
            self.chat && matches!(need, harbor_inference::provider::Capabilities::Chat)
        }
        fn load(&self, _m: &ModelRef) -> Result<(), harbor_inference::provider::ProviderError> {
            Ok(())
        }
        fn unload(&self, _m: &ModelRef) -> Result<(), harbor_inference::provider::ProviderError> {
            Ok(())
        }
        fn generate(
            &self,
            req: harbor_inference::provider::ChatRequest,
        ) -> Result<
            harbor_inference::provider::ChatResponse,
            harbor_inference::provider::ProviderError,
        > {
            // Unconstrained model: must not have been handed a schema.
            assert!(req.response_schema.is_none());
            Ok(harbor_inference::provider::ChatResponse {
                content: self.answer.into(),
                usage: harbor_inference::provider::Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                },
                executed_on: "scripted".into(),
                execution_location: harbor_security::policy::ExecutionLocation::OnDevice,
            })
        }
        fn execution_location(&self) -> harbor_security::policy::ExecutionLocation {
            harbor_security::policy::ExecutionLocation::OnDevice
        }
    }

    fn hits() -> Vec<RouteHit> {
        ["sheets", "email"]
            .iter()
            .map(|id| RouteHit {
                skill_id: (*id).into(),
                title: id.to_uppercase(),
                description: format!("does {id}"),
                score: 0.8,
            })
            .collect()
    }

    #[test]
    fn disambiguation_delegates_to_any_chat_model_and_validates() {
        let chat_model = ModelRef::InstalledPackage {
            package_id: "some-user-chosen-llm".into(),
        };
        let good = ScriptedChat {
            answer: r#"{"skill_id":"email"}"#,
            chat: true,
        };
        assert_eq!(
            disambiguate(&good, &chat_model, "write to bob", &hits()),
            Disambiguation::Chose {
                skill_id: "email".into()
            }
        );
        // Off-list answer from the same model: discarded.
        let rogue = ScriptedChat {
            answer: r#"{"skill_id":"admin-shell"}"#,
            chat: true,
        };
        assert!(matches!(
            disambiguate(&rogue, &chat_model, "write to bob", &hits()),
            Disambiguation::Unavailable(_)
        ));
        // A model without verified chat support is never asked.
        let incapable = ScriptedChat {
            answer: r#"{"skill_id":"email"}"#,
            chat: false,
        };
        assert!(matches!(
            disambiguate(&incapable, &chat_model, "write to bob", &hits()),
            Disambiguation::Unavailable(_)
        ));
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
