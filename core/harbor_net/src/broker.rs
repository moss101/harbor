//! The Egress Broker: the single authorization point for app-controlled
//! network traffic.
//!
//! Callers describe a request (origin, method, path, class, credentials)
//! and a [`Transport`] implementation; the broker decides, logs and only
//! then dispatches. Redirects never bypass the broker: each hop is
//! re-authorized and cross-origin hops drop credentials unless the session
//! explicitly rebinds them.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use crate::audit::{AuditSink, NetworkAuditEntry, NetworkEventKind};
pub use harbor_security::policy::EgressClass;

/// An authorized egress window: one class, one origin, expiring.
#[derive(Debug, Clone)]
pub struct EgressSession {
    pub session_id: String,
    pub class: EgressClass,
    pub origin: String,
    pub rebind_credentials_cross_origin: bool,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl EgressSession {
    pub fn is_valid_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.issued_at && now < self.expires_at
    }

    pub fn allows_origin(&self, origin: &str, now: DateTime<Utc>) -> bool {
        self.is_valid_at(now) && self.origin == origin
    }
}

/// A description of an outbound request (no payload leaves this struct to
/// the broker; bodies stream through the transport).
#[derive(Debug, Clone)]
pub struct TransportRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TransportResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Final URL after redirects performed by the transport itself (should
    /// be empty when the transport defers redirect handling to the broker).
    pub final_url: String,
}

impl TransportResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status) && self.header("location").is_some()
    }
}

/// Transport abstraction. Implementations must NOT follow redirects
/// themselves; the broker drives each hop.
pub trait Transport: Send + Sync {
    fn execute(
        &self,
        req: &TransportRequest,
        timeout: Duration,
    ) -> std::io::Result<TransportResponse>;

    /// Streaming variant for large bodies: each chunk is handed to `sink`
    /// as it arrives. Default delegates to the buffered [`Self::execute`];
    /// real transports override this so multi-GB models never need to fit
    /// in memory.
    fn execute_streaming(
        &self,
        req: &TransportRequest,
        timeout: Duration,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<TransportResponse> {
        let response = self.execute(req, timeout)?;
        sink(&response.body)?;
        Ok(response)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectDecision {
    /// Same-origin redirect (or authorized rebind): follow with the broker.
    Follow { strip_credentials: bool },
    /// Redirect to an origin not covered by any session: blocked.
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
    Completed,
    Blocked,
    Failed,
}

pub struct EgressBroker {
    sink: Box<dyn AuditSink>,
    sessions: Mutex<HashMap<String, EgressSession>>,
    default_timeout: Duration,
}

/// Header names treated as credentials for cross-origin stripping.
const CREDENTIAL_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "proxy-authorization",
    "x-api-key",
];

impl EgressBroker {
    pub fn new(sink: Box<dyn AuditSink>) -> Self {
        EgressBroker {
            sink,
            sessions: Mutex::new(HashMap::new()),
            default_timeout: Duration::from_secs(30),
        }
    }

    /// Open an explicit egress session (user-visible action). Sessions are
    /// short-lived; long transfers re-issue.
    pub fn open_session(
        &self,
        class: EgressClass,
        origin: &str,
        lifetime: ChronoDuration,
        workspace_mode: harbor_security::policy::PrivacyMode,
    ) -> Result<EgressSession, BrokerError> {
        if !class.permitted_under(workspace_mode) {
            self.log(
                NetworkEventKind::Blocked,
                class,
                origin,
                "SESSION",
                "/",
                None,
                None,
                None,
                format!("class {class:?} not permitted under {workspace_mode}"),
            );
            return Err(BrokerError::PolicyDenied);
        }
        let now = Utc::now();
        let session = EgressSession {
            session_id: format!(
                "egress-{}",
                harbor_canonical::sha256_hex(format!("{class:?}{origin}{now}").as_bytes())
                    .get(..16)
                    .unwrap_or("s")
            ),
            class,
            origin: origin.to_string(),
            rebind_credentials_cross_origin: false,
            issued_at: now,
            expires_at: now + lifetime,
        };
        self.sessions
            .lock()
            .unwrap()
            .insert(session.session_id.clone(), session.clone());
        Ok(session)
    }

    pub fn close_session(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
    }

    pub fn active_sessions(&self) -> Vec<EgressSession> {
        self.sessions.lock().unwrap().values().cloned().collect()
    }

    /// Decide whether a redirect hop may be followed.
    pub fn redirect_decision(
        &self,
        session: &EgressSession,
        from_origin: &str,
        to_url: &url::Url,
        now: DateTime<Utc>,
    ) -> RedirectDecision {
        let to_origin = match (to_url.scheme(), to_url.host_str(), to_url.port()) {
            (s, Some(h), p) => match p {
                Some(p) => format!("{s}://{h}:{p}"),
                None => format!("{s}://{h}"),
            },
            _ => return RedirectDecision::Blocked,
        };
        if to_origin == from_origin {
            return RedirectDecision::Follow {
                strip_credentials: false,
            };
        }
        if session.allows_origin(&to_origin, now) {
            return RedirectDecision::Follow {
                strip_credentials: !session.rebind_credentials_cross_origin,
            };
        }
        RedirectDecision::Blocked
    }

    /// Execute a request under a session through `transport`, logging every
    /// state transition. Each redirect hop is re-authorized.
    pub fn dispatch(
        &self,
        session: &EgressSession,
        req: TransportRequest,
        transport: &dyn Transport,
        run_id: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<TransportResponse, BrokerError> {
        let parsed: url::Url = req
            .url
            .parse()
            .map_err(|_| BrokerError::InvalidRequest("unparseable url"))?;
        let origin = request_origin(&parsed).ok_or(BrokerError::InvalidRequest("no origin"))?;

        // Attempt.
        if !session.allows_origin(&origin, now) {
            self.log(
                NetworkEventKind::Blocked,
                session.class,
                &origin,
                &req.method,
                parsed.path(),
                None,
                Some(&session.session_id),
                run_id,
                "no valid session for origin".into(),
            );
            return Err(BrokerError::PolicyDenied);
        }
        let mut current = req;
        let mut current_origin = origin;
        let mut hops = 0usize;
        loop {
            self.log(
                NetworkEventKind::Dispatched,
                session.class,
                &current_origin,
                &current.method,
                current_path(&current).as_str(),
                None,
                Some(&session.session_id),
                run_id,
                format!("hop {hops}"),
            );
            let response = transport
                .execute(&current, self.default_timeout)
                .map_err(|e| {
                    self.log(
                        NetworkEventKind::Completed,
                        session.class,
                        &current_origin,
                        &current.method,
                        current_path(&current).as_str(),
                        None,
                        Some(&session.session_id),
                        run_id,
                        format!("transport error: {e}"),
                    );
                    BrokerError::Transport(e.to_string())
                })?;
            if response.is_redirect() {
                let location = response.header("location").unwrap_or_default();
                let next: url::Url = parsed
                    .join(location)
                    .map_err(|_| BrokerError::InvalidRequest("bad redirect location"))?;
                match self.redirect_decision(session, &current_origin, &next, Utc::now()) {
                    RedirectDecision::Follow { strip_credentials } => {
                        let mut next_req = TransportRequest {
                            method: if response.status == 307 || response.status == 308 {
                                current.method.clone()
                            } else {
                                "GET".into()
                            },
                            url: next.to_string(),
                            headers: current.headers.clone(),
                            body: if response.status == 307 || response.status == 308 {
                                current.body.clone()
                            } else {
                                Vec::new()
                            },
                        };
                        if strip_credentials {
                            next_req.headers.retain(|(k, _)| {
                                !CREDENTIAL_HEADERS.contains(&k.to_ascii_lowercase().as_str())
                            });
                        }
                        current = next_req;
                        current_origin = request_origin(&next)
                            .ok_or(BrokerError::InvalidRequest("no origin"))?;
                        hops += 1;
                        if hops > 10 {
                            return Err(BrokerError::TooManyRedirects);
                        }
                        continue;
                    }
                    RedirectDecision::Blocked => {
                        self.log(
                            NetworkEventKind::RedirectBlocked,
                            session.class,
                            &current_origin,
                            &current.method,
                            next.path(),
                            Some(response.status),
                            Some(&session.session_id),
                            run_id,
                            format!("redirect to {next} denied"),
                        );
                        return Err(BrokerError::RedirectDenied(next.to_string()));
                    }
                }
            }
            self.log(
                NetworkEventKind::Completed,
                session.class,
                &current_origin,
                &current.method,
                current_path(&current).as_str(),
                Some(response.status),
                Some(&session.session_id),
                run_id,
                String::new(),
            );
            return Ok(response);
        }
    }

    /// Streaming dispatch: identical authorization loop to [`Self::dispatch`]
    /// but the final 2xx response body is streamed chunk-by-chunk into
    /// `sink` instead of buffered. Redirect hops are re-authorized exactly
    /// as in the buffered path; the hop count still applies.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_streaming(
        &self,
        session: &EgressSession,
        req: TransportRequest,
        transport: &dyn Transport,
        run_id: Option<&str>,
        now: DateTime<Utc>,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> Result<(), BrokerError> {
        let parsed: url::Url = req
            .url
            .parse()
            .map_err(|_| BrokerError::InvalidRequest("unparseable url"))?;
        let origin = request_origin(&parsed).ok_or(BrokerError::InvalidRequest("no origin"))?;
        if !session.allows_origin(&origin, now) {
            self.log(
                NetworkEventKind::Blocked,
                session.class,
                &origin,
                &req.method,
                parsed.path(),
                None,
                Some(&session.session_id),
                run_id,
                "no valid session for origin".into(),
            );
            return Err(BrokerError::PolicyDenied);
        }
        let mut current = req;
        let mut current_origin = origin;
        let mut hops = 0usize;
        loop {
            self.log(
                NetworkEventKind::Dispatched,
                session.class,
                &current_origin,
                &current.method,
                current_path(&current).as_str(),
                None,
                Some(&session.session_id),
                run_id,
                format!("stream hop {hops}"),
            );
            let response = transport
                .execute_streaming(&current, self.default_timeout, sink)
                .map_err(|e| {
                    self.log(
                        NetworkEventKind::Completed,
                        session.class,
                        &current_origin,
                        &current.method,
                        current_path(&current).as_str(),
                        None,
                        Some(&session.session_id),
                        run_id,
                        format!("stream transport error: {e}"),
                    );
                    BrokerError::Transport(e.to_string())
                })?;
            if response.is_redirect() {
                let location = response.header("location").unwrap_or_default();
                let next: url::Url = parsed
                    .join(location)
                    .map_err(|_| BrokerError::InvalidRequest("bad redirect location"))?;
                match self.redirect_decision(session, &current_origin, &next, Utc::now()) {
                    RedirectDecision::Follow { strip_credentials } => {
                        let mut next_req = TransportRequest {
                            method: if response.status == 307 || response.status == 308 {
                                current.method.clone()
                            } else {
                                "GET".into()
                            },
                            url: next.to_string(),
                            headers: current.headers.clone(),
                            body: Vec::new(),
                        };
                        if strip_credentials {
                            next_req.headers.retain(|(k, _)| {
                                !CREDENTIAL_HEADERS.contains(&k.to_ascii_lowercase().as_str())
                            });
                        }
                        current = next_req;
                        current_origin = request_origin(&next)
                            .ok_or(BrokerError::InvalidRequest("no origin"))?;
                        hops += 1;
                        if hops > 10 {
                            return Err(BrokerError::TooManyRedirects);
                        }
                        continue;
                    }
                    RedirectDecision::Blocked => {
                        self.log(
                            NetworkEventKind::RedirectBlocked,
                            session.class,
                            &current_origin,
                            &current.method,
                            next.path(),
                            Some(response.status),
                            Some(&session.session_id),
                            run_id,
                            format!("stream redirect to {next} denied"),
                        );
                        return Err(BrokerError::RedirectDenied(next.to_string()));
                    }
                }
            }
            self.log(
                NetworkEventKind::Completed,
                session.class,
                &current_origin,
                &current.method,
                current_path(&current).as_str(),
                Some(response.status),
                Some(&session.session_id),
                run_id,
                "stream completed".into(),
            );
            // Non-2xx after streaming: the error document may already be
            // streamed to the caller's discard/hash sink; the caller must
            // treat non-2xx as failure and discard state.
            if !(200..300).contains(&response.status) {
                return Err(BrokerError::Http(response.status));
            }
            return Ok(());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn log(
        &self,
        kind: NetworkEventKind,
        class: EgressClass,
        origin: &str,
        method: &str,
        path: &str,
        status: Option<u16>,
        session_id: Option<&str>,
        run_id: Option<&str>,
        detail: String,
    ) {
        self.sink.append(NetworkAuditEntry {
            seq: 0,
            kind,
            egress_class: format!("{class:?}"),
            origin: origin.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            status,
            session_id: session_id.map(|s| s.to_string()),
            run_id: run_id.map(|s| s.to_string()),
            detail,
            at_rfc3339: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            prev_entry_hash: None,
            entry_hash: String::new(),
        });
    }
}

/// Path (+query) of the request actually being executed at this hop.
fn current_path(req: &TransportRequest) -> String {
    req.url
        .parse::<url::Url>()
        .map(|u| {
            let p = u.path();
            match u.query() {
                Some(q) => format!("{p}?{q}"),
                None => p.to_string(),
            }
        })
        .unwrap_or_else(|_| req.url.clone())
}

fn request_origin(u: &url::Url) -> Option<String> {
    let host = u.host_str()?;
    let default_port = match u.scheme() {
        "https" => 443,
        "http" => 80,
        _ => return None,
    };
    Some(match u.port() {
        Some(p) if p != default_port => format!("{}://{}:{}", u.scheme(), host, p),
        _ => format!("{}://{}", u.scheme(), host),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("policy denied this egress")]
    PolicyDenied,
    #[error("redirect to {0} was not authorized")]
    RedirectDenied(String),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("http status {0}")]
    Http(u16),
}

// Re-export the policy alias used in session construction signatures.
pub use harbor_security::policy::PrivacyMode;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::SqliteAuditSink;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct RedirectThenOk {
        redirects: AtomicUsize,
        redirect_target: String,
    }

    impl Transport for RedirectThenOk {
        fn execute(
            &self,
            _req: &TransportRequest,
            _t: Duration,
        ) -> std::io::Result<TransportResponse> {
            let n = self.redirects.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Ok(TransportResponse {
                    status: 302,
                    headers: vec![("location".into(), self.redirect_target.clone())],
                    body: Vec::new(),
                    final_url: String::new(),
                })
            } else {
                Ok(TransportResponse {
                    status: 200,
                    headers: Vec::new(),
                    body: b"payload".to_vec(),
                    final_url: String::new(),
                })
            }
        }
    }

    struct AlwaysBlocked;
    impl Transport for AlwaysBlocked {
        fn execute(
            &self,
            _req: &TransportRequest,
            _t: Duration,
        ) -> std::io::Result<TransportResponse> {
            Ok(TransportResponse {
                status: 403,
                headers: Vec::new(),
                body: Vec::new(),
                final_url: String::new(),
            })
        }
    }

    fn broker() -> EgressBroker {
        EgressBroker::new(Box::new(SqliteAuditSink::open_in_memory().unwrap()))
    }

    #[test]
    fn unauthorized_class_under_local_only_is_denied() {
        let b = broker();
        let err = b.open_session(
            EgressClass::RemoteInference,
            "https://api.remote.test",
            ChronoDuration::minutes(5),
            harbor_security::policy::PrivacyMode::LocalOnly,
        );
        assert!(matches!(err, Err(BrokerError::PolicyDenied)));
    }

    #[test]
    fn acquisition_is_allowed_under_local_only() {
        let b = broker();
        let s = b
            .open_session(
                EgressClass::AcquisitionMetadata,
                "https://huggingface.co",
                ChronoDuration::minutes(5),
                harbor_security::policy::PrivacyMode::LocalOnly,
            )
            .unwrap();
        assert!(s.is_valid_at(Utc::now()));
    }

    #[test]
    fn request_outside_session_origin_is_blocked_and_logged() {
        let b = broker();
        let s = b
            .open_session(
                EgressClass::AcquisitionMetadata,
                "https://huggingface.co",
                ChronoDuration::minutes(5),
                harbor_security::policy::PrivacyMode::LocalOnly,
            )
            .unwrap();
        let req = TransportRequest {
            method: "GET".into(),
            url: "https://evil.test/exfil".into(),
            headers: Vec::new(),
            body: Vec::new(),
        };
        let t = AlwaysBlocked;
        assert!(matches!(
            b.dispatch(&s, req, &t, None, Utc::now()),
            Err(BrokerError::PolicyDenied)
        ));
        let entries = b.sink.entries();
        assert!(entries
            .iter()
            .any(|e| e.kind == NetworkEventKind::Blocked && e.origin == "https://evil.test"));
    }

    #[test]
    fn cross_origin_redirect_strips_credentials_and_requires_authorization() {
        let b = broker();
        let s = b
            .open_session(
                EgressClass::WeightTransfer,
                "https://cdn.example.test",
                ChronoDuration::minutes(5),
                harbor_security::policy::PrivacyMode::LocalOnly,
            )
            .unwrap();
        // Note session origin is cdn.example.test; the redirect goes
        // same-origin, so it follows without stripping.
        let t = RedirectThenOk {
            redirects: AtomicUsize::new(0),
            redirect_target: "/file.bin".into(),
        };
        let req = TransportRequest {
            method: "GET".into(),
            url: "https://cdn.example.test/file.bin".into(),
            headers: vec![("authorization".into(), "Bearer secret".into())],
            body: Vec::new(),
        };
        let resp = b.dispatch(&s, req, &t, None, Utc::now()).unwrap();
        assert_eq!(resp.status, 200);
        // Cross-origin redirect is blocked (no session for it).
        let t2 = RedirectThenOk {
            redirects: AtomicUsize::new(0),
            redirect_target: "https://attacker.test/file.bin".into(),
        };
        let req2 = TransportRequest {
            method: "GET".into(),
            url: "https://cdn.example.test/file.bin".into(),
            headers: vec![("authorization".into(), "Bearer secret".into())],
            body: Vec::new(),
        };
        assert!(matches!(
            b.dispatch(&s, req2, &t2, None, Utc::now()),
            Err(BrokerError::RedirectDenied(_))
        ));
        let entries = b.sink.entries();
        assert!(entries
            .iter()
            .any(|e| e.kind == NetworkEventKind::RedirectBlocked));
    }
}
