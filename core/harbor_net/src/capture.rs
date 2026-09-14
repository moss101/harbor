//! Independent wire capture: a transport wrapper that records every
//! request that actually leaves the process, independently of the broker's
//! audit log. Qualification compares the two paths 1:1 — any request on
//! the wire without a Dispatched entry (or vice versa) is an egress
//! policy violation.

use std::sync::Mutex;
use std::time::Duration;

use crate::audit::NetworkAuditEntry;
use crate::broker::{Transport, TransportRequest, TransportResponse};

/// One wire-level request/response observation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WireRecord {
    pub method: String,
    pub url: String,
    pub origin: String,
    pub path: String,
    pub status: Option<u16>,
    pub bytes_in: usize,
    pub error: Option<String>,
}

/// Wrap any transport and record every execution. The capture is written
/// BEFORE the inner transport runs, so a hung or crashing inner call still
/// leaves the request on record.
pub struct CaptureTransport {
    inner: Box<dyn Transport>,
    records: Mutex<Vec<WireRecord>>,
}

impl CaptureTransport {
    pub fn new(inner: Box<dyn Transport>) -> Self {
        CaptureTransport {
            inner,
            records: Mutex::new(Vec::new()),
        }
    }

    pub fn records(&self) -> Vec<WireRecord> {
        self.records.lock().unwrap().clone()
    }

    pub fn record_count(&self) -> usize {
        self.records.lock().unwrap().len()
    }
}

/// Same origin normalization as the broker (default ports omitted).
fn origin_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            let host = u.host_str()?.to_string();
            let default_port = match u.scheme() {
                "https" => 443,
                "http" => 80,
                _ => return None,
            };
            Some(match u.port() {
                Some(p) if p != default_port => format!("{}://{}:{}", u.scheme(), host, p),
                _ => format!("{}://{}", u.scheme(), host),
            })
        })
        .unwrap_or_default()
}

fn path_of(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| {
            let p = u.path().to_string();
            match u.query() {
                Some(q) => format!("{p}?{q}"),
                None => p,
            }
        })
        .unwrap_or_default()
}

impl Transport for CaptureTransport {
    fn execute(
        &self,
        req: &TransportRequest,
        timeout: Duration,
    ) -> std::io::Result<TransportResponse> {
        self.records.lock().unwrap().push(WireRecord {
            method: req.method.clone(),
            url: req.url.clone(),
            origin: origin_of(&req.url),
            path: path_of(&req.url),
            status: None,
            bytes_in: 0,
            error: None,
        });
        let result = self.inner.execute(req, timeout);
        let mut records = self.records.lock().unwrap();
        let last = records.last_mut().expect("record was just pushed");
        match &result {
            Ok(resp) => {
                last.status = Some(resp.status);
                last.bytes_in = resp.body.len();
            }
            Err(e) => last.error = Some(e.to_string()),
        }
        result
    }

    fn execute_streaming(
        &self,
        req: &TransportRequest,
        timeout: Duration,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<TransportResponse> {
        self.records.lock().unwrap().push(WireRecord {
            method: req.method.clone(),
            url: req.url.clone(),
            origin: origin_of(&req.url),
            path: path_of(&req.url),
            status: None,
            bytes_in: 0,
            error: None,
        });
        let result = self.inner.execute_streaming(req, timeout, sink);
        let mut records = self.records.lock().unwrap();
        let last = records.last_mut().expect("record was just pushed");
        match &result {
            Ok(resp) => {
                last.status = Some(resp.status);
                last.bytes_in = resp.body.len();
            }
            Err(e) => last.error = Some(e.to_string()),
        }
        result
    }
}

/// Compare the independent wire capture against the broker audit log.
/// Returns a list of violations; empty means the two paths agree 1:1:
/// every Dispatched entry has exactly one wire request, in order, and no
/// unlogged request reached the wire.
pub fn compare_capture_to_audit(
    capture: &[WireRecord],
    audit: &[crate::audit::NetworkAuditEntry],
) -> Vec<String> {
    let dispatched: Vec<&NetworkAuditEntry> = audit
        .iter()
        .filter(|e| e.kind == crate::audit::NetworkEventKind::Dispatched)
        .collect();
    let mut violations = Vec::new();
    if capture.len() != dispatched.len() {
        violations.push(format!(
            "wire requests ({}) != broker Dispatched entries ({})",
            capture.len(),
            dispatched.len()
        ));
    }
    for (i, (wire, log)) in capture.iter().zip(dispatched.iter()).enumerate() {
        if wire.method != log.method {
            violations.push(format!(
                "hop {i}: wire method {} != logged {}",
                wire.method, log.method
            ));
        }
        if wire.origin != log.origin {
            violations.push(format!(
                "hop {i}: wire origin {} != logged {}",
                wire.origin, log.origin
            ));
        }
        if wire.path != log.path {
            violations.push(format!(
                "hop {i}: wire path {} != logged {}",
                wire.path, log.path
            ));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{NetworkAuditEntry, NetworkEventKind};

    fn entry(kind: NetworkEventKind, method: &str, origin: &str, path: &str) -> NetworkAuditEntry {
        NetworkAuditEntry {
            seq: 0,
            kind,
            egress_class: "test".into(),
            origin: origin.into(),
            method: method.into(),
            path: path.into(),
            status: Some(200),
            session_id: None,
            run_id: None,
            detail: String::new(),
            at_rfc3339: String::new(),
            prev_entry_hash: None,
            entry_hash: String::new(),
        }
    }

    #[test]
    fn compare_detects_no_violation_when_matching() {
        let capture = vec![WireRecord {
            method: "GET".into(),
            url: "https://a.example/x".into(),
            origin: "https://a.example".into(),
            path: "/x".into(),
            status: Some(200),
            bytes_in: 1,
            error: None,
        }];
        let audit = vec![entry(
            NetworkEventKind::Dispatched,
            "GET",
            "https://a.example",
            "/x",
        )];
        assert!(compare_capture_to_audit(&capture, &audit).is_empty());
    }

    #[test]
    fn compare_detects_unlogged_wire_request_and_path_drift() {
        let capture = vec![WireRecord {
            method: "GET".into(),
            url: "https://evil.example/x".into(),
            origin: "https://evil.example".into(),
            path: "/x".into(),
            status: Some(200),
            bytes_in: 1,
            error: None,
        }];
        let audit = vec![entry(
            NetworkEventKind::Dispatched,
            "GET",
            "https://a.example",
            "/x",
        )];
        let violations = compare_capture_to_audit(&capture, &audit);
        assert!(
            violations.iter().any(|v| v.contains("origin")),
            "origin drift must be flagged: {violations:?}"
        );
    }
}
