//! Independent network-capture qualification: the wire capture
//! (capture::CaptureTransport) must agree with the broker's hash-chained
//! audit log 1:1 — no unlogged egress, no logged-but-unexecuted hop —
//! across redirect chains, blocked requests and strict Local Only
//! operation (release gap: independent traffic capture vs broker log).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use harbor_net::audit::{AuditSink, NetworkEventKind, SqliteAuditSink};
use harbor_net::broker::{
    BrokerError, EgressBroker, Transport, TransportRequest, TransportResponse,
};
use harbor_net::capture::{compare_capture_to_audit, CaptureTransport};
use harbor_security::policy::{EgressClass, PrivacyMode};

fn sink() -> std::sync::Arc<SqliteAuditSink> {
    std::sync::Arc::new(SqliteAuditSink::open_in_memory().unwrap())
}

fn req(method: &str, url: &str) -> TransportRequest {
    TransportRequest {
        method: method.into(),
        url: url.into(),
        headers: Vec::new(),
        body: Vec::new(),
    }
}

/// Scripted chain transport: each call returns the next scripted response.
struct Scripted {
    responses: Vec<TransportResponse>,
    calls: AtomicUsize,
}

impl Scripted {
    fn new(responses: Vec<TransportResponse>) -> Self {
        Scripted {
            responses,
            calls: AtomicUsize::new(0),
        }
    }
}

impl Transport for Scripted {
    fn execute(&self, _req: &TransportRequest, _t: Duration) -> std::io::Result<TransportResponse> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.responses[n.min(self.responses.len() - 1)].clone())
    }
}

fn redirect_to(location: &str) -> TransportResponse {
    TransportResponse {
        status: 302,
        headers: vec![("location".into(), location.into())],
        body: Vec::new(),
        final_url: String::new(),
    }
}

fn ok(body: &[u8]) -> TransportResponse {
    TransportResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
        final_url: String::new(),
    }
}

fn session_for(
    broker: &EgressBroker,
    origin: &str,
    class: EgressClass,
) -> harbor_net::broker::EgressSession {
    broker
        .open_session(
            class,
            origin,
            chrono::Duration::minutes(5),
            PrivacyMode::LocalOnly,
        )
        .expect("session must open under LocalOnly for an allowed class")
}

/// Dispatch a request and follow redirect hops exactly the way the
/// production HfAcquirer does: on RedirectDenied, re-dispatch the target
/// under the pre-opened session for that origin (or stop honestly).
fn dispatch_chain(
    broker: &EgressBroker,
    sessions: &std::collections::BTreeMap<String, harbor_net::broker::EgressSession>,
    wire: &CaptureTransport,
    url: &str,
) -> Result<TransportResponse, BrokerError> {
    let mut current_url = url.to_string();
    let mut hops = 0usize;
    loop {
        let parsed: url::Url = current_url
            .parse()
            .map_err(|_| BrokerError::InvalidRequest("bad url"))?;
        let origin = format!(
            "{}://{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or_default()
        );
        let session = sessions.get(&origin).ok_or(BrokerError::PolicyDenied)?;
        match broker.dispatch(session, req("GET", &current_url), wire, None, Utc::now()) {
            Ok(resp) => return Ok(resp),
            Err(BrokerError::RedirectDenied(next)) => {
                let next: url::Url = next
                    .parse()
                    .map_err(|_| BrokerError::InvalidRequest("bad redirect"))?;
                let next_origin = format!(
                    "{}://{}",
                    next.scheme(),
                    next.host_str().unwrap_or_default()
                );
                if !sessions.contains_key(&next_origin) {
                    return Err(BrokerError::PolicyDenied);
                }
                current_url = next.to_string();
                hops += 1;
                if hops > 10 {
                    return Err(BrokerError::TooManyRedirects);
                }
            }
            Err(e) => return Err(e),
        }
    }
}

#[test]
fn redirect_chain_capture_matches_audit_one_to_one() {
    let audit = sink();
    let broker = EgressBroker::new(Box::new(audit.clone()));
    let wire = CaptureTransport::new(Box::new(Scripted::new(vec![
        redirect_to("https://cas-bridge.test/file"),
        redirect_to("https://us.aws.cdn.test/chunk"),
        ok(b"weights"),
    ])));
    // Explicit, logged sessions for every origin of the chain (the
    // acquisition contract: each open is a user-visible action).
    let mut sessions = std::collections::BTreeMap::new();
    for origin in [
        "https://hub.test",
        "https://cas-bridge.test",
        "https://us.aws.cdn.test",
    ] {
        let s = session_for(&broker, origin, EgressClass::WeightTransfer);
        sessions.insert(origin.to_string(), s);
    }

    dispatch_chain(&broker, &sessions, &wire, "https://hub.test/models/m")
        .expect("chain completes");

    // 1:1 across the three hops.
    let violations = compare_capture_to_audit(&wire.records(), &audit.entries());
    assert!(violations.is_empty(), "violations: {violations:?}");
    // The chain is fully visible in BOTH paths.
    assert_eq!(wire.record_count(), 3);
    let records = wire.records();
    let origins: Vec<&str> = records.iter().map(|r| r.origin.as_str()).collect();
    assert_eq!(
        origins,
        [
            "https://hub.test",
            "https://cas-bridge.test",
            "https://us.aws.cdn.test"
        ]
    );
    // Hop paths are logged per hop (not the original path three times).
    let dispatched: Vec<String> = audit
        .entries()
        .iter()
        .filter(|e| e.kind == NetworkEventKind::Dispatched)
        .map(|e| e.path.clone())
        .collect();
    assert_eq!(dispatched, vec!["/models/m", "/file", "/chunk"]);
    // Completed entries match wire statuses.
    let completed: Vec<Option<u16>> = audit
        .entries()
        .iter()
        .filter(|e| e.kind == NetworkEventKind::Completed)
        .map(|e| e.status)
        .collect();
    assert_eq!(completed, vec![Some(200)]);
    // The audit chain itself is intact.
    assert!(harbor_net::audit::verify_chain(&audit.entries()));
}

#[test]
fn blocked_request_never_reaches_the_wire() {
    let audit = sink();
    let broker = EgressBroker::new(Box::new(audit.clone()));
    // A transport that FAILS the test if ever contacted for the blocked
    // attempt: capture must stay empty.
    struct Boom;
    impl Transport for Boom {
        fn execute(&self, _: &TransportRequest, _: Duration) -> std::io::Result<TransportResponse> {
            panic!("blocked request reached the wire");
        }
    }
    let wire = CaptureTransport::new(Box::new(Boom));
    // Session for origin A only.
    let s = session_for(&broker, "https://hub.test", EgressClass::WeightTransfer);
    let err = broker
        .dispatch(
            &s,
            req("GET", "https://other.test/exfil"),
            &wire,
            None,
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(err, BrokerError::PolicyDenied));
    assert_eq!(wire.record_count(), 0, "nothing may hit the wire");
    let all = audit.entries();
    let blocked: Vec<&harbor_net::audit::NetworkAuditEntry> = all
        .iter()
        .filter(|e| e.kind == NetworkEventKind::Blocked)
        .collect();
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].origin, "https://other.test");
    assert!(harbor_net::audit::verify_chain(&audit.entries()));
}

#[test]
fn redirect_to_unauthorized_origin_blocked_and_unexecuted() {
    let audit = sink();
    let broker = EgressBroker::new(Box::new(audit.clone()));
    let wire = CaptureTransport::new(Box::new(Scripted::new(vec![
        redirect_to("https://unauthorized.test/next"),
        ok(b"never"),
    ])));
    let s = session_for(&broker, "https://hub.test", EgressClass::WeightTransfer);
    let err = broker
        .dispatch(
            &s,
            req("GET", "https://hub.test/models/m"),
            &wire,
            None,
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(err, BrokerError::RedirectDenied(url) if url.contains("unauthorized.test")));
    // Only the FIRST hop was on the wire; the redirect target never was.
    assert_eq!(wire.record_count(), 1);
    assert_eq!(wire.records()[0].origin, "https://hub.test");
    // Audit log: dispatched for hop 0, redirect_blocked for the target.
    let entries = audit.entries();
    assert!(entries
        .iter()
        .any(|e| e.kind == NetworkEventKind::RedirectBlocked));
    assert!(!entries
        .iter()
        .any(|e| e.kind == NetworkEventKind::Dispatched && e.origin.contains("unauthorized.test")));
    let violations = compare_capture_to_audit(&wire.records(), &entries);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn strict_local_only_denies_sessions_and_keeps_wire_silent() {
    let audit = sink();
    let broker = EgressBroker::new(Box::new(audit.clone()));
    struct Boom;
    impl Transport for Boom {
        fn execute(&self, _: &TransportRequest, _: Duration) -> std::io::Result<TransportResponse> {
            panic!("no traffic may leave under LocalOnly without a session");
        }
    }
    let wire = CaptureTransport::new(Box::new(Boom));
    // Remote inference is categorically denied under LocalOnly.
    let err = broker
        .open_session(
            EgressClass::RemoteInference,
            "https://api.remote.test",
            chrono::Duration::minutes(5),
            PrivacyMode::LocalOnly,
        )
        .unwrap_err();
    assert!(matches!(err, BrokerError::PolicyDenied));
    // And a request attempted without any session is denied unlogged on
    // the wire but recorded as blocked in the audit chain.
    let s = session_for(
        &broker,
        "https://hub.test",
        EgressClass::AcquisitionMetadata,
    );
    // Close the session: now nothing is valid.
    broker.close_session(&s.session_id);
    let err = broker
        .dispatch(
            &s,
            req("GET", "https://hub.test/models"),
            &wire,
            None,
            Utc::now() + chrono::Duration::minutes(10), // past close/expiry
        )
        .unwrap_err();
    assert!(matches!(err, BrokerError::PolicyDenied));
    assert_eq!(wire.record_count(), 0);
    assert!(audit
        .entries()
        .iter()
        .any(|e| e.kind == NetworkEventKind::Blocked));
}

#[test]
fn streaming_path_capture_matches_audit_one_to_one() {
    let audit = sink();
    let broker = EgressBroker::new(Box::new(audit.clone()));
    let wire = CaptureTransport::new(Box::new(Scripted::new(vec![
        redirect_to("https://cdn.test/big"),
        ok(b"chunked-payload"),
    ])));
    let mut sessions = std::collections::BTreeMap::new();
    for origin in ["https://hub.test", "https://cdn.test"] {
        let s = session_for(&broker, origin, EgressClass::WeightTransfer);
        sessions.insert(origin.to_string(), s);
    }
    // Drive the chain manually with dispatch_streaming per hop (mirrors
    // fetch_streaming_to's sanctioned RedirectDenied loop).
    let mut chunks = Vec::new();
    {
        let s0 = sessions.get("https://hub.test").unwrap();
        let err = broker
            .dispatch_streaming(
                s0,
                req("GET", "https://hub.test/blob"),
                &wire,
                None,
                Utc::now(),
                &mut |c| {
                    chunks.extend_from_slice(c);
                    Ok(())
                },
            )
            .unwrap_err();
        let BrokerError::RedirectDenied(next) = err else {
            panic!("expected redirect denial, got {err:?}");
        };
        let s1 = sessions.get("https://cdn.test").unwrap();
        broker
            .dispatch_streaming(s1, req("GET", &next), &wire, None, Utc::now(), &mut |c| {
                chunks.extend_from_slice(c);
                Ok(())
            })
            .expect("final hop completes");
    }
    assert_eq!(chunks, b"chunked-payload");
    let violations = compare_capture_to_audit(&wire.records(), &audit.entries());
    assert!(violations.is_empty(), "{violations:?}");
    assert_eq!(wire.record_count(), 2, "both hops captured");
}
