//! In-app model acquisition: download Hugging Face files through the
//! Egress Broker and commit them through the staged installer.
//!
//! HF LFS files redirect to CDN origins (cdn-lfs.huggingface.co /
//! cdn-lfs.hf.co). Every hop is brokered: the acquirer holds explicit
//! weight-transfer sessions per allowed origin; a redirect to an origin
//! without a session is a hard stop (logged as redirect_blocked). Nothing
//! is installed unless every file's hash matches the expected value.

use std::collections::BTreeMap;

use harbor_net::AuditSink;
use harbor_net::broker::{
    BrokerError, DispatchOutcome, EgressBroker, EgressClass, Transport, TransportRequest,
};
use harbor_net::broker::PrivacyMode;
use chrono::{DateTime, Utc};

use crate::install::{PackageFile, PackageInstaller, PackageManifest, RuntimeBinding};

/// HF weight files redirect through several storage origins depending on
/// repo age and storage backend (LFS classic vs Xet). An acquisition flow
/// opens weight-transfer sessions for the origins the catalog declares.
pub const HF_CDN_ORIGINS: [&str; 4] = [
    "https://cdn-lfs.huggingface.co",
    "https://cdn-lfs.hf.co",
    "https://us.aws.cdn.hf.co",
    "https://cas-bridge.xethub.hf.co",
];

pub struct HfAcquirer<'a> {
    pub broker: &'a EgressBroker,
    pub transport: &'a dyn Transport,
    pub installer: &'a PackageInstaller,
    /// Origin -> session id, pre-opened by the caller (each open is an
    /// explicit, logged acquisition action).
    pub sessions: BTreeMap<String, harbor_net::broker::EgressSession>,
}

#[derive(Debug, thiserror::Error)]
pub enum AcquireError {
    #[error("no session for origin {0} (redirect blocked)")]
    NoSession(String),
    #[error("broker: {0}")]
    Broker(String),
    #[error("http {0} for {1}")]
    Http(u16, String),
    #[error("hash mismatch for {0}")]
    HashMismatch(String),
    #[error("install: {0}")]
    Install(String),
}

impl HfAcquirer<'_> {
    fn session_for(&self, origin: &str) -> Option<&harbor_net::broker::EgressSession> {
        self.sessions.get(origin)
    }

    /// GET a URL through the broker, following hops only while each origin
    /// has a session. Returns the final body.
    pub fn fetch(
        &self,
        url: &str,
        run_id: Option<&str>,
    ) -> Result<Vec<u8>, AcquireError> {
        let mut current_url: url::Url =
            url.parse().map_err(|e| AcquireError::Broker(format!("url: {e}")))?;
        let mut hops = 0;
        loop {
            let origin = origin_of(&current_url)
                .ok_or_else(|| AcquireError::Broker("no origin".into()))?;
            let session = self
                .session_for(&origin)
                .ok_or_else(|| AcquireError::NoSession(origin.clone()))?;
            let result = self.broker.dispatch(
                session,
                TransportRequest {
                    method: "GET".into(),
                    url: current_url.to_string(),
                    headers: vec![("accept".into(), "*/*".into())],
                    body: Vec::new(),
                },
                self.transport,
                run_id,
                Utc::now(),
            );
            match result {
                Ok(resp) => {
                    if !(200..300).contains(&resp.status) {
                        return Err(AcquireError::Http(resp.status, current_url.to_string()));
                    }
                    return Ok(resp.body);
                }
                Err(BrokerError::RedirectDenied(next)) => {
                    // Was the redirect denied because we lack a session for
                    // the next origin? Then stop honestly.
                    let next_url: url::Url =
                        next.parse().map_err(|e| AcquireError::Broker(format!("url: {e}")))?;
                    let next_origin = origin_of(&next_url)
                        .ok_or_else(|| AcquireError::Broker("no origin".into()))?;
                    if self.session_for(&next_origin).is_none() {
                        return Err(AcquireError::NoSession(next_origin));
                    }
                    current_url = next_url;
                    hops += 1;
                    if hops > 10 {
                        return Err(AcquireError::Broker("too many hops".into()));
                    }
                }
                Err(BrokerError::PolicyDenied) => {
                    return Err(AcquireError::NoSession(origin));
                }
                Err(e) => return Err(AcquireError::Broker(e.to_string())),
            }
        }
    }

    /// Acquire one model package: download each declared file from HF,
    /// verify hashes, and commit through the staged installer.
    #[allow(clippy::too_many_arguments)]
    pub fn acquire(
        &self,
        package_id: &str,
        repo_id: &str,
        revision: &str,
        files: &[(String, String, String)], // (path, role, sha256)
        now: DateTime<Utc>,
    ) -> Result<serde_json::Value, AcquireError> {
        let mut staged = self
            .installer
            .begin(package_id)
            .map_err(|e| AcquireError::Install(e.to_string()))?;
        let mut manifest_files = Vec::new();
        for (path, role, sha256) in files {
            let url = format!(
                "https://huggingface.co/{repo_id}/resolve/{revision}/{path}"
            );
            let bytes = self.fetch(&url, None)?;
            let got = harbor_canonical::sha256_hex(&bytes);
            if !sha256.is_empty() && got != *sha256 {
                return Err(AcquireError::HashMismatch(path.clone()));
            }
            // With no pinned hash (search-driven acquisition), the hash of
            // the downloaded bytes becomes the package identity and is
            // surfaced to the user.
            let effective_sha = if sha256.is_empty() { got } else { sha256.clone() };
            let pf = PackageFile {
                role: role.clone(),
                path: path.clone(),
                sha256: effective_sha,
                size_bytes: bytes.len() as u64,
            };
            self.installer
                .ingest_file(&mut staged, &pf, &bytes)
                .map_err(|e| AcquireError::Install(e.to_string()))?;
            manifest_files.push(pf);
        }
        let manifest = PackageManifest {
            schema: "harbor.model/v3".into(),
            id: package_id.to_string(),
            reference_type: "installed_package".into(),
            files: manifest_files,
            runtime: RuntimeBinding {
                kind: "gguf/llama.cpp".into(),
                min_revision: "0.1.156".into(),
                targets: vec![std::env::consts::ARCH.to_string()],
            },
        };
        let report = self
            .installer
            .validate(&staged, &manifest)
            .map_err(|e| AcquireError::Install(e.to_string()))?;
        if !report.ok {
            return Err(AcquireError::Install(format!("{:?}", report.problems)));
        }
        let dir = self
            .installer
            .commit(&mut staged, &manifest, now)
            .map_err(|e| AcquireError::Install(e.to_string()))?;
        Ok(serde_json::json!({
            "installed": package_id,
            "dir": dir.to_string_lossy(),
            "files": files.len(),
        }))
    }
}

pub fn origin_of(u: &url::Url) -> Option<String> {
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

// Re-exports keep the acquisition API self-contained for callers.
pub use harbor_net::broker::DispatchOutcome as _DispatchOutcomeMarker;

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_net::audit::SqliteAuditSink;
    use harbor_net::broker::EgressBroker;
    use harbor_net::transport::UreqTransport;
    use harbor_security::policy::PrivacyMode;
    use tempfile::TempDir;

    /// Real-network proof: acquire the tiny stories260K model from HF
    /// through the broker (including the CDN redirect hop) and confirm the
    /// installed bytes match the recorded fixture hash.
    #[test]
    fn acquire_real_model_through_broker_end_to_end() {
        let dir = TempDir::new().unwrap();
        let sink = std::sync::Arc::new(SqliteAuditSink::open_in_memory().unwrap());
        let broker = EgressBroker::new(Box::new(sink.clone()));
        let transport = UreqTransport::new();
        let installer = PackageInstaller::new(dir.path().join("models"));

        let mut sessions = BTreeMap::new();
        for origin in ["https://huggingface.co", HF_CDN_ORIGINS[0], HF_CDN_ORIGINS[1], HF_CDN_ORIGINS[2], HF_CDN_ORIGINS[3]] {
            let s = broker
                .open_session(
                    EgressClass::WeightTransfer,
                    origin,
                    chrono::Duration::minutes(10),
                    PrivacyMode::LocalOnly,
                )
                .unwrap();
            sessions.insert(origin.to_string(), s);
        }

        let acquirer = HfAcquirer {
            broker: &broker,
            transport: &transport,
            installer: &installer,
            sessions,
        };
        let sha = "270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d";
        let result = acquirer
            .acquire(
                "stories260k",
                "ggml-org/models",
                "main",
                &[(
                    "tinyllamas/stories260K.gguf".to_string(),
                    "weights".to_string(),
                    sha.to_string(),
                )],
                Utc::now(),
            )
            .unwrap();
        assert_eq!(result["installed"], "stories260k");
        assert_eq!(
            installer.installed_packages().unwrap(),
            vec!["stories260k".to_string()]
        );
        // The audit log proves the traffic crossed the broker: blocked
        // attempts / dispatched / completed entries exist for the HF and
        // CDN origins.
        let entries = sink.entries();
        assert!(
            entries
                .iter()
                .any(|e| e.kind == harbor_net::NetworkEventKind::Dispatched),
            "dispatched entries must exist"
        );
        assert!(
            entries
                .iter()
                .any(|e| e.kind == harbor_net::NetworkEventKind::Completed),
            "completed entries must exist"
        );
        assert!(
            entries
                .iter()
                .any(|e| e.origin.starts_with("https://huggingface.co")),
            "the hf origin hop must be logged"
        );
    }

    #[test]
    fn no_cdn_session_blocks_the_download() {
        let dir = TempDir::new().unwrap();
        let sink = SqliteAuditSink::open_in_memory().unwrap();
        let broker = EgressBroker::new(Box::new(sink));
        let transport = UreqTransport::new();
        let installer = PackageInstaller::new(dir.path().join("models"));
        let mut sessions = BTreeMap::new();
        let s = broker
            .open_session(
                EgressClass::WeightTransfer,
                "https://huggingface.co",
                chrono::Duration::minutes(5),
                PrivacyMode::LocalOnly,
            )
            .unwrap();
        sessions.insert("https://huggingface.co".to_string(), s);
        let acquirer = HfAcquirer {
            broker: &broker,
            transport: &transport,
            installer: &installer,
            sessions,
        };
        // Without a CDN session the redirect hop must be refused.
        let result = acquirer.acquire(
            "m",
            "ggml-org/models",
            "main",
            &[(
                "tinyllamas/stories260K.gguf".to_string(),
                "weights".to_string(),
                "0".repeat(64),
            )],
            Utc::now(),
        );
        // Whatever the error, it must NOT be a successful install and the
        // failure must be the missing CDN session (not a hash error: no
        // bytes may be trusted without brokered hops).
        match result {
            Err(AcquireError::NoSession(origin)) => {
                assert!(origin != "https://huggingface.co");
            }
            other => panic!("expected NoSession, got {other:?}"),
        }
    }
}
