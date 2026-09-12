//! Hugging Face public discovery. Acquisition metadata only — model
//! search terms travel, workspace content never does (egress class
//! `AcquisitionMetadata`, broker-mediated).
//!
//! The HTTP client is injected as a [`harbor_net::broker::Transport`] so
//! every request crosses the Egress Broker; tests use a fake transport and
//! desktop/mobile builds plug the real TLS client.

use serde::Deserialize;

use harbor_net::broker::{BrokerError, EgressBroker, Transport, TransportRequest};

pub const HF_ORIGIN: &str = "https://huggingface.co";

#[derive(Debug, Clone, Deserialize)]
pub struct HfModel {
    pub id: String,
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub pipeline_tag: Option<String>,
    #[serde(default)]
    pub likes: i64,
    #[serde(default)]
    pub downloads: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HfFile {
    #[serde(rename = "rfilename")]
    pub path: String,
    #[serde(default)]
    pub size: u64,
}

pub struct HfDiscovery<'a> {
    pub broker: &'a EgressBroker,
    pub transport: &'a dyn Transport,
    /// Session origin override for tests/proxies.
    pub origin: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HfError {
    #[error("broker: {0}")]
    Broker(String),
    #[error("api decode: {0}")]
    Decode(String),
}

impl<'a> HfDiscovery<'a> {
    pub fn new(broker: &'a EgressBroker, transport: &'a dyn Transport) -> Self {
        HfDiscovery { broker, transport, origin: HF_ORIGIN.into() }
    }

    fn get_json(
        &self,
        session: &harbor_net::broker::EgressSession,
        path: &str,
    ) -> Result<serde_json::Value, HfError> {
        let url = format!("{}{}", self.origin, path);
        let req = TransportRequest {
            method: "GET".into(),
            url,
            headers: vec![("accept".into(), "application/json".into()), ("user-agent".into(), "Harbor/0.1".into())],
            body: Vec::new(),
        };
        let resp = self
            .broker
            .dispatch(session, req, self.transport, None, chrono::Utc::now())
            .map_err(|e: BrokerError| HfError::Broker(e.to_string()))?;
        if resp.status != 200 {
            return Err(HfError::Decode(format!("HTTP {}", resp.status)));
        }
        serde_json::from_slice(&resp.body).map_err(|e| HfError::Decode(e.to_string()))
    }

    /// Search public models. `query` is acquisition metadata only.
    pub fn search(
        &self,
        session: &harbor_net::broker::EgressSession,
        query: &str,
        limit: usize,
    ) -> Result<Vec<HfModel>, HfError> {
        let q = urlencode(query);
        let v = self.get_json(
            session,
            &format!("/api/models?search={q}&limit={limit}&sort=downloads&direction=-1"),
        )?;
        serde_json::from_value(v).map_err(|e| HfError::Decode(e.to_string()))
    }

    /// List the files of one repo revision, with sizes when available.
    pub fn list_files(
        &self,
        session: &harbor_net::broker::EgressSession,
        repo_id: &str,
        revision: &str,
    ) -> Result<Vec<HfFile>, HfError> {
        let tree = format!("/api/models/{}/{}/tree/main", urlencode(repo_id), urlencode(revision));
        let v = self.get_json(session, &tree)?;
        serde_json::from_value(v).map_err(|e| HfError::Decode(e.to_string()))
    }

    /// Filter repo files down to a compatible GGUF acquisition set:
    /// weights + tokenizer/config; ignore everything else by default.
    pub fn compatible_gguf_files(files: &[HfFile]) -> Vec<HfFile> {
        files
            .iter()
            .filter(|f| {
                let p = f.path.to_ascii_lowercase();
                p.ends_with(".gguf") || p.ends_with("tokenizer.json") || p.ends_with("config.json")
            })
            .cloned()
            .collect()
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_net::audit::SqliteAuditSink;
    use harbor_net::broker::{EgressBroker, Transport, TransportResponse};
    use harbor_security::policy::PrivacyMode;
    use std::sync::Mutex;

    struct FakeHf {
        calls: Mutex<Vec<String>>,
    }

    impl Transport for FakeHf {
        fn execute(
            &self,
            req: &TransportRequest,
            _t: std::time::Duration,
        ) -> std::io::Result<TransportResponse> {
            self.calls.lock().unwrap().push(req.url.clone());
            let body = if req.url.contains("/api/models?") {
                br#"[{"id":"ggml-org/tiny","sha":"abc123","likes":5,"downloads":100,"pipeline_tag":"text-generation"}]"#.to_vec()
            } else {
                br#"[{"type":"file","rfilename":"tiny-Q4_K_M.gguf","size":1234},{"type":"file","rfilename":"tokenizer.json","size":10},{"type":"file","rfilename":"README.md","size":5}]"#.to_vec()
            };
            Ok(TransportResponse { status: 200, headers: Vec::new(), body, final_url: String::new() })
        }
    }

    #[test]
    fn discovery_goes_through_broker_and_filters_compatible_files() {
        let sink = SqliteAuditSink::open_in_memory().unwrap();
        let broker = EgressBroker::new(Box::new(sink));
        let session = broker
            .open_session(
                harbor_net::broker::EgressClass::AcquisitionMetadata,
                HF_ORIGIN,
                chrono::Duration::minutes(5),
                PrivacyMode::LocalOnly,
            )
            .unwrap();
        let transport = FakeHf { calls: Mutex::new(Vec::new()) };
        let hf = HfDiscovery::new(&broker, &transport);
        let models = hf.search(&session, "tiny llama", 5).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "ggml-org/tiny");
        // Every request stayed on the authorized origin.
        for url in transport.calls.lock().unwrap().iter() {
            assert!(url.starts_with(HF_ORIGIN), "egress escaped: {url}");
        }
        let files = hf.list_files(&session, "ggml-org/tiny", "abc123").unwrap();
        let compat = HfDiscovery::compatible_gguf_files(&files);
        let names: Vec<&str> = compat.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names, vec!["tiny-Q4_K_M.gguf", "tokenizer.json"]);
    }

    #[test]
    fn search_outside_session_origin_is_blocked() {
        let sink = SqliteAuditSink::open_in_memory().unwrap();
        let broker = EgressBroker::new(Box::new(sink));
        let session = broker
            .open_session(
                harbor_net::broker::EgressClass::AcquisitionMetadata,
                HF_ORIGIN,
                chrono::Duration::minutes(5),
                PrivacyMode::LocalOnly,
            )
            .unwrap();
        // Origin override to an attacker host: broker must refuse.
        let transport = FakeHf { calls: Mutex::new(Vec::new()) };
        let mut hf = HfDiscovery::new(&broker, &transport);
        hf.origin = "https://evil.test".into();
        assert!(hf.search(&session, "x", 3).is_err());
    }
}
