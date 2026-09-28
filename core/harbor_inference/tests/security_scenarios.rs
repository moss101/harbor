//! Security scenarios SEC-011 and SEC-019 (09_Security_Test_Matrix.csv)
//! as executable controls:
//! - `security.sec_011` — silent cloud fallback: the provider router has
//!   an explicit policy; a missing model is a typed error under ExactOnly,
//!   substitution under AllowSubstitution is a visible record (never
//!   silent), and no registered local provider may answer for a request
//!   it was not asked for.
//! - `security.sec_019` — memory residue: unloading a model drops it from
//!   the provider's loaded set (best-effort clear; nothing answers
//!   afterwards), and a reload re-materializes it cleanly.

use harbor_canonical::JsonValue;
use harbor_inference::provider::{Capabilities, ModelProvider, ModelRef};
use harbor_inference::router::{Router, RouterPolicy};
use harbor_inference::TestBackend;

fn request(pkg: &str) -> harbor_inference::ChatRequest {
    harbor_inference::ChatRequest {
        model: ModelRef::InstalledPackage {
            package_id: pkg.into(),
        },
        messages: vec![JsonValue::object([
            ("role", JsonValue::str("user")),
            ("content", JsonValue::str("offline question")),
        ])],
        max_tokens: 16,
        temperature: 0.0,
        requires: vec![Capabilities::Chat],
        response_schema: None,
        trace_key: None,
    }
}

/// SEC-011 part 1: under the default ExactOnly policy a missing model is
/// a typed ModelNotFound — the runtime NEVER quietly tries another
/// model, local or otherwise.
#[test]
fn sec_011_missing_model_is_a_typed_error_never_a_fallback() {
    let mut router = Router::new(RouterPolicy::ExactOnly);
    router.register(Box::new(TestBackend::default().with_packages(&["pkg-a"])));
    let err = router.chat(request("pkg-not-installed")).unwrap_err();
    assert!(
        matches!(err, harbor_inference::ProviderError::ModelNotFound(_)),
        "ExactOnly must fail loudly, got: {err}"
    );
}

/// SEC-011 part 2: when substitution IS allowed it is a first-class
/// visible record the UI must show — never a silent swap.
#[test]
fn sec_011_substitution_is_visible_never_silent() {
    let mut router = Router::new(RouterPolicy::AllowSubstitution);
    router.register(Box::new(
        TestBackend::default().with_packages(&["pkg-b-alt"]),
    ));
    let (resp, sub) = router.chat(request("pkg-b")).unwrap();
    let sub = sub.expect("substitution must be surfaced as a record");
    assert_eq!(
        sub.reason, "requested model unavailable; qualified substitute used",
        "the record carries an operator-readable reason"
    );
    assert_eq!(resp.executed_on, "pkg-b-alt", "executed_on states reality");
}

/// SEC-011 part 3: the local answer names the package that produced it —
/// a response can never claim an identity other than its executor's.
#[test]
fn sec_011_executed_identity_matches_the_provider() {
    let mut router = Router::new(RouterPolicy::ExactOnly);
    router.register(Box::new(TestBackend::default().with_packages(&["pkg-a"])));
    let (resp, sub) = router.chat(request("pkg-a")).unwrap();
    assert!(sub.is_none());
    assert!(resp.executed_on.starts_with("pkg-a"));
    assert_eq!(
        resp.execution_location,
        harbor_security::policy::ExecutionLocation::OnDevice,
        "local providers answer on-device; remote execution is the \
         RemoteEndpoint path behind the egress broker, never a fallback"
    );
}

/// SEC-019: unload drops the loaded model — nothing answers for it
/// afterwards (the runtime does not keep a resident copy), and a reload
/// re-materializes a working context.
#[test]
fn sec_019_unload_clears_resident_model_state() {
    let provider = TestBackend::default().with_packages(&["pkg-a"]);
    let model = ModelRef::InstalledPackage {
        package_id: "pkg-a".into(),
    };
    provider.load(&model).expect("load");
    let resp = provider.generate(request("pkg-a")).unwrap();
    assert!(resp.executed_on == "pkg-a");
    provider.unload(&model).expect("unload");
    // After unload the model is gone from the resident set: a generate
    // against it is a typed error, not a stale-context answer.
    let err = provider
        .generate(request("pkg-a"))
        .expect_err("unloaded model must not answer");
    assert!(matches!(
        err,
        harbor_inference::ProviderError::ModelNotFound(_)
    ));
    // Best-effort residue clearing must not break a subsequent load.
    provider.load(&model).expect("reload after unload");
    provider
        .generate(request("pkg-a"))
        .expect("answers again after reload");
}
