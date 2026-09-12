//! In-app model acquisition: download Hugging Face files through the
//! Egress Broker and commit them through the staged installer.
//!
//! HF LFS files redirect to CDN origins (cdn-lfs.huggingface.co /
//! cdn-lfs.hf.co). Every hop is brokered: the acquirer holds explicit
//! weight-transfer sessions per allowed origin; a redirect to an origin
//! without a session is a hard stop (logged as redirect_blocked). Nothing
//! is installed unless every file's hash matches the expected value.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use harbor_net::AuditSink;
use harbor_net::broker::{
    BrokerError, DispatchOutcome, EgressBroker, EgressClass, Transport, TransportRequest,
};
use harbor_net::broker::PrivacyMode;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use harbor_canonical::JsonValue;

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
    /// Optional HF bearer token for gated/private repos. Attached ONLY to
    /// huggingface.co requests; the broker strips it on any cross-origin
    /// redirect hop (policy 13).
    pub auth_token: Option<String>,
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
        let mut retries = 0u32;
        loop {
            let origin = origin_of(&current_url)
                .ok_or_else(|| AcquireError::Broker("no origin".into()))?;
            let session = self
                .session_for(&origin)
                .ok_or_else(|| AcquireError::NoSession(origin.clone()))?;
            // The bearer token rides only on huggingface.co requests.
            let mut headers = vec![("accept".into(), "*/*".into())];
            if origin == "https://huggingface.co" {
                if let Some(token) = &self.auth_token {
                    headers.push(("authorization".into(), format!("Bearer {token}")));
                }
            }
            let result = self.broker.dispatch(
                session,
                TransportRequest {
                    method: "GET".into(),
                    url: current_url.to_string(),
                    headers,
                    body: Vec::new(),
                },
                self.transport,
                run_id,
                Utc::now(),
            );
            match result {
                Ok(resp) => {
                    if !(200..300).contains(&resp.status) {
                        // 429/503 are transient server-side refusals (no
                        // bytes dispatched to us): bounded retry with
                        // backoff, then an honest failure.
                        if (resp.status == 429 || resp.status == 503) && retries < 3 {
                            std::thread::sleep(
                                std::time::Duration::from_secs(2 * (retries as u64 + 1)),
                            );
                            retries += 1;
                            continue;
                        }
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

    /// Stream a URL into `out` through the broker, hashing incrementally.
    /// Memory use is O(chunk), not O(file): safe for multi-GB weights.
    pub fn fetch_streaming_to(
        &self,
        url: &str,
        out: &Path,
        run_id: Option<&str>,
    ) -> Result<(String, u64), AcquireError> {
        let mut current_url: url::Url =
            url.parse().map_err(|e| AcquireError::Broker(format!("url: {e}")))?;
        let mut hops = 0;
        let mut retries = 0u32;
        loop {
            let origin = origin_of(&current_url)
                .ok_or_else(|| AcquireError::Broker("no origin".into()))?;
            let session = self
                .session_for(&origin)
                .ok_or_else(|| AcquireError::NoSession(origin.clone()))?;
            let mut file = std::fs::File::create(out)
                .map_err(|e| AcquireError::Install(e.to_string()))?;
            let mut hasher = Sha256::new();
            let mut size = 0u64;
            {
                let mut sink = |chunk: &[u8]| -> std::io::Result<()> {
                    hasher.update(chunk);
                    size += chunk.len() as u64;
                    file.write_all(chunk)
                };
                let result = self.broker.dispatch_streaming(
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
                    &mut sink,
                );
                file.flush().ok();
                match result {
                    Ok(()) => {
                        let hash = hex::encode(hasher.finalize());
                        return Ok((hash, size));
                    }
                    Err(BrokerError::RedirectDenied(next)) => {
                        let next_url: url::Url = next
                            .parse()
                            .map_err(|e| AcquireError::Broker(format!("url: {e}")))?;
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
                        // Retry the hop with a fresh file.
                        continue;
                    }
                    Err(e) => return Err(AcquireError::Broker(e.to_string())),
                }
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
            let out = staged.staging_dir.join(path);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AcquireError::Install(e.to_string()))?;
            }
            let (got, size) = self.fetch_streaming_to(&url, &out, None)?;
            if !sha256.is_empty() && got != *sha256 {
                let _ = std::fs::remove_file(&out);
                return Err(AcquireError::HashMismatch(path.clone()));
            }
            // With no pinned hash (search-driven acquisition), the hash of
            // the downloaded bytes becomes the package identity and is
            // surfaced to the user.
            let effective_sha = if sha256.is_empty() { got } else { sha256.clone() };
            let pf = PackageFile {
                role: role.clone(),
                path: path.clone(),
                sha256: effective_sha.clone(),
                size_bytes: size,
            };
            staged.verified_files.insert(path.clone(), size);
            let _ = effective_sha;
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
static HF_NETWORK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let _guard = HF_NETWORK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            auth_token: None,
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
        let _guard = HF_NETWORK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            auth_token: None,
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

/// A package entry inside a signed catalog document.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogPackage {
    pub id: String,
    pub repo_id: String,
    pub revision: String,
    /// (path, role, pinned sha256)
    pub files: Vec<(String, String, String)>,
    pub quantization: String,
    pub context_tokens: u64,
}

/// Parse a signed catalog document (the canonical `entries` JSON of a
/// `SignedCatalog`). Every file must carry a pinned sha256: entries without
/// one are rejected — acquisition without an accountable hash never runs.
pub fn parse_catalog_document(entries: &JsonValue) -> Result<Vec<CatalogPackage>, AcquireError> {
    let Some(packages) = entries.get("packages").and_then(|v| v.as_array()) else {
        return Err(AcquireError::Malformed("missing packages array".into()));
    };
    let mut out = Vec::new();
    for p in packages {
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AcquireError::Malformed("package missing id".into()))?
            .to_string();
        let repo_id = p
            .get("repo_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AcquireError::Malformed("package missing repo_id".into()))?
            .to_string();
        let revision = p
            .get("revision")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();
        let quantization = p
            .get("quantization")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let context_tokens = p
            .get("context_tokens")
            .and_then(|v| v.as_int())
            .map(|i| i.max(0) as u64)
            .unwrap_or(2048);
        let mut files = Vec::new();
        for f in p
            .get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AcquireError::Malformed("package missing files".into()))?
        {
            let path = f
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcquireError::Malformed("file missing path".into()))?
                .to_string();
            let role = f
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("weights")
                .to_string();
            let sha = f
                .get("sha256")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if sha.len() != 64 {
                return Err(AcquireError::Malformed(format!(
                    "file {path} lacks a pinned 64-char sha256"
                )));
            }
            files.push((path, role, sha));
        }
        out.push(CatalogPackage {
            id,
            repo_id,
            revision,
            files,
            quantization,
            context_tokens,
        });
    }
    Ok(out)
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
    #[error("malformed catalog: {0}")]
    Malformed(String),
    #[error("catalog: {0}")]
    Catalog(String),
    #[error("package {0} not in catalog")]
    PackageNotInCatalog(String),
}

/// Verify a signed catalog and acquire one of its packages. The pinned
/// per-file hashes come ONLY from the signed document — a signature over
/// the whole document makes the hash set tamper-evident, and the epoch rule
/// prevents rolling the catalog back to older (weaker) contents.
#[allow(clippy::too_many_arguments)]
pub fn acquire_signed(
    verifier: &mut crate::catalog_signing::CatalogVerifier,
    signed: &crate::catalog_signing::SignedCatalog,
    package_id: &str,
    installer: &PackageInstaller,
    broker: &EgressBroker,
    transport: &dyn Transport,
    sessions: &BTreeMap<String, harbor_net::broker::EgressSession>,
    now: DateTime<Utc>,
) -> Result<serde_json::Value, AcquireError> {
    verifier
        .verify(signed)
        .map_err(|e| AcquireError::Catalog(e.to_string()))?;
    let packages = parse_catalog_document(&signed.entries)?;
    let package = packages
        .iter()
        .find(|p| p.id == package_id)
        .ok_or_else(|| AcquireError::PackageNotInCatalog(package_id.to_string()))?;
    let acquirer = HfAcquirer {
        broker,
        transport,
        installer,
        sessions: sessions.clone(),
        auth_token: None,
    };
    acquirer.acquire(
        &package.id,
        &package.repo_id,
        &package.revision,
        &package.files
            .iter()
            .map(|(p, r, s)| (p.clone(), r.clone(), s.clone()))
            .collect::<Vec<_>>(),
        now,
    )
}

#[cfg(test)]
mod signed_tests {
    use super::*;
    use super::HF_NETWORK_LOCK;
    use crate::catalog_signing::{sign_catalog, CatalogSigningKey, CatalogVerifier};
    use harbor_net::audit::SqliteAuditSink;
    use harbor_net::broker::EgressBroker;
    use harbor_net::transport::UreqTransport;
    use tempfile::TempDir;

    fn sessions_for(broker: &EgressBroker) -> BTreeMap<String, harbor_net::broker::EgressSession> {
        let mut sessions = BTreeMap::new();
        for origin in ["https://huggingface.co", HF_CDN_ORIGINS[0], HF_CDN_ORIGINS[1], HF_CDN_ORIGINS[2], HF_CDN_ORIGINS[3]] {
            let s = broker
                .open_session(
                    EgressClass::WeightTransfer,
                    origin,
                    chrono::Duration::minutes(10),
                    harbor_security::policy::PrivacyMode::LocalOnly,
                )
                .unwrap();
            sessions.insert(origin.to_string(), s);
        }
        sessions
    }

    fn signed_catalog(sha: &str) -> crate::catalog_signing::SignedCatalog {
        let key = CatalogSigningKey::from_secret_bytes(&[7u8; 32]);
        let entries = harbor_canonical::parse(&format!(
            r#"{{"packages":[{{"context_tokens":2048,"files":[{{"path":"tinyllamas/stories260K.gguf","role":"weights","sha256":"{sha}"}}],"id":"stories260k","quantization":"Q8_0","repo_id":"ggml-org/models","revision":"main"}}]}}"#
        ))
        .unwrap();
        sign_catalog(&key, 1, "2026-09-12T00:00:00Z", entries).unwrap()
    }

    #[test]
    fn signed_catalog_acquisition_verifies_against_pinned_hash() {
        let _guard = HF_NETWORK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new().unwrap();
        let sink = std::sync::Arc::new(SqliteAuditSink::open_in_memory().unwrap());
        let broker = EgressBroker::new(Box::new(sink.clone()));
        let transport = UreqTransport::new();
        let installer = PackageInstaller::new(dir.path().join("models"));
        let key = CatalogSigningKey::from_secret_bytes(&[7u8; 32]);
        let mut verifier = CatalogVerifier::new(&hex::encode(&key.public_bytes())).unwrap();
        let sessions = sessions_for(&broker);

        // The pinned hash is the REAL model hash, signed into the catalog.
        let catalog = signed_catalog(
            "270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d",
        );
        let result = acquire_signed(
            &mut verifier,
            &catalog,
            "stories260k",
            &installer,
            &broker,
            &transport,
            &sessions,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(result["installed"], "stories260k");
        assert_eq!(
            installer.installed_packages().unwrap(),
            vec!["stories260k".to_string()]
        );
        // Brokered evidence: the stream was dispatched and completed.
        assert!(sink.entries().iter().any(|e| e.kind == harbor_net::NetworkEventKind::Completed));
    }

    #[test]
    fn signed_catalog_with_wrong_pinned_hash_blocks_install() {
        let _guard = HF_NETWORK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new().unwrap();
        let sink = std::sync::Arc::new(SqliteAuditSink::open_in_memory().unwrap());
        let broker = EgressBroker::new(Box::new(sink.clone()));
        let transport = UreqTransport::new();
        let installer = PackageInstaller::new(dir.path().join("models"));
        let key = CatalogSigningKey::from_secret_bytes(&[7u8; 32]);
        let mut verifier = CatalogVerifier::new(&hex::encode(&key.public_bytes())).unwrap();
        let sessions = sessions_for(&broker);

        // A catalog signed with a WRONG hash (attacker or stale metadata):
        // the download happens, the pinned-hash check must refuse install.
        let catalog = signed_catalog(&"0".repeat(64));
        let result = acquire_signed(
            &mut verifier,
            &catalog,
            "stories260k",
            &installer,
            &broker,
            &transport,
            &sessions,
            Utc::now(),
        );
        assert!(matches!(result, Err(AcquireError::HashMismatch(_))));
        // Nothing installed.
        assert!(installer.installed_packages().unwrap().is_empty());
        assert!(matches!(
            verifier.verify(&signed_catalog(&"0".repeat(64))),
            Err(crate::catalog_signing::CatalogSignError::StaleEpoch { .. })
        ));
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;
    use tempfile::TempDir;
    use harbor_net::audit::SqliteAuditSink;
    use harbor_net::broker::{EgressBroker, Transport, TransportResponse};
    use harbor_security::policy::PrivacyMode;
    use std::sync::Mutex;

    /// Records the Authorization header seen per origin.
    struct RecordingTransport {
        seen: Mutex<Vec<(String, Option<String>)>>,
    }

    impl Transport for RecordingTransport {
        fn execute(
            &self,
            req: &TransportRequest,
            _t: std::time::Duration,
        ) -> std::io::Result<TransportResponse> {
            let origin = req
                .url
                .split('/')
                .take(3)
                .last()
                .unwrap_or("")
                .to_string();
            let host = req
                .url
                .split('/')
                .nth(2)
                .unwrap_or("")
                .to_string();
            let auth = req
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                .map(|(_, v)| v.clone());
            self.seen.lock().unwrap().push((host, auth));
            let _ = origin;
            if req.url.contains("huggingface.co") {
                // Redirect to the CDN (cross-origin hop).
                Ok(TransportResponse {
                    status: 302,
                    headers: vec![(
                        "location".to_string(),
                        "https://cdn-lfs.hf.co/file.bin".into(),
                    )],
                    body: Vec::new(),
                    final_url: String::new(),
                })
            } else {
                Ok(TransportResponse {
                    status: 200,
                    headers: Vec::new(),
                    body: b"data".to_vec(),
                    final_url: String::new(),
                })
            }
        }
    }

    #[test]
    fn token_rides_hf_hops_and_is_stripped_cross_origin() {
        let dir = TempDir::new().unwrap();
        let sink = SqliteAuditSink::open_in_memory().unwrap();
        let broker = EgressBroker::new(Box::new(sink));
        let transport = RecordingTransport { seen: Mutex::new(Vec::new()) };
        let installer = PackageInstaller::new(dir.path().join("models"));
        let mut sessions = BTreeMap::new();
        for origin in ["https://huggingface.co", "https://cdn-lfs.hf.co"] {
            let s = broker
                .open_session(
                    EgressClass::WeightTransfer,
                    origin,
                    chrono::Duration::minutes(5),
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
            auth_token: Some("hf_secret_token".to_string()),
        };
        let bytes = acquirer
            .fetch("https://huggingface.co/repo/resolve/main/file.bin", None)
            .unwrap();
        assert_eq!(bytes, b"data");
        let seen = transport.seen.lock().unwrap();
        let (hf_host, hf_auth) = &seen[0];
        assert!(hf_host.contains("huggingface.co"));
        assert_eq!(hf_auth.as_deref(), Some("Bearer hf_secret_token"));
        let (cdn_host, cdn_auth) = &seen[1];
        assert!(cdn_host.contains("cdn-lfs"));
        assert!(cdn_auth.is_none(), "credentials must be stripped cross-origin");
    }

    #[test]
    fn no_token_means_no_auth_header_anywhere() {
        let dir = TempDir::new().unwrap();
        let sink = SqliteAuditSink::open_in_memory().unwrap();
        let broker = EgressBroker::new(Box::new(sink));
        let transport = RecordingTransport { seen: Mutex::new(Vec::new()) };
        let installer = PackageInstaller::new(dir.path().join("models"));
        let mut sessions = BTreeMap::new();
        for origin in ["https://huggingface.co", "https://cdn-lfs.hf.co"] {
            let s = broker
                .open_session(
                    EgressClass::WeightTransfer,
                    origin,
                    chrono::Duration::minutes(5),
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
            auth_token: None,
        };
        acquirer
            .fetch("https://huggingface.co/repo/resolve/main/file.bin", None)
            .unwrap();
        for (host, auth) in transport.seen.lock().unwrap().iter() {
            assert!(auth.is_none(), "unexpected auth header for {host}");
        }
    }
}
