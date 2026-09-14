//! Real HTTPS transport over ureq (rustls), wired to the broker contract.
//!
//! Redirects are DISABLED at the client: every hop is returned to the
//! [`crate::broker::EgressBroker`] which re-authorizes each origin change
//! and strips cross-origin credentials — the transport must never follow
//! a redirect on its own (13 policy).

use std::time::Duration;

use crate::broker::{Transport, TransportRequest, TransportResponse};

pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new() -> Self {
        UreqTransport {
            agent: ureq::AgentBuilder::new()
                .redirects(0)
                .user_agent("harbor-modelhub/0.1 (+https://github.com/mohsin/harbor)")
                .build(),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for UreqTransport {
    /// True streaming: response bytes are written to `sink` in 64 KiB
    /// chunks as they arrive off the wire.
    fn execute_streaming(
        &self,
        req: &TransportRequest,
        timeout: Duration,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<TransportResponse> {
        let mut request = self.agent.request(&req.method, &req.url).timeout(timeout);
        for (k, v) in &req.headers {
            request = request.set(k, v);
        }
        let response = if req.body.is_empty() && req.method != "POST" {
            request.call()
        } else {
            request.send(req.body.as_slice())
        };
        let resp = match response {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_code, resp)) => resp,
            Err(ureq::Error::Transport(t)) => {
                return Err(std::io::Error::other(format!("transport: {t}")));
            }
        };
        let status = resp.status();
        let headers: Vec<(String, String)> = resp
            .headers_names()
            .iter()
            .map(|name| {
                (
                    name.to_lowercase(),
                    resp.header(name).unwrap_or_default().to_string(),
                )
            })
            .collect();
        let mut reader = resp.into_reader().take(16 * 1024 * 1024 * 1024);
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            sink(&chunk[..n])?;
        }
        Ok(TransportResponse {
            status,
            headers,
            body: Vec::new(),
            final_url: String::new(),
        })
    }

    fn execute(
        &self,
        req: &TransportRequest,
        timeout: Duration,
    ) -> std::io::Result<TransportResponse> {
        let mut request = self.agent.request(&req.method, &req.url).timeout(timeout);
        for (k, v) in &req.headers {
            request = request.set(k, v);
        }
        let response = if req.body.is_empty() && req.method != "POST" {
            request.call()
        } else {
            request.send(req.body.as_slice())
        };
        match response {
            Ok(resp) => Ok(to_response(resp)),
            Err(ureq::Error::Status(_code, resp)) => Ok(to_response(resp)),
            // Redirects land here when the agent has redirects(0): the
            // synthetic error carries the response.
            Err(ureq::Error::Transport(t)) => {
                // ureq surfaces redirect responses (when not following) as
                // transport errors with an attached response only for some
                // kinds; treat unknown as failure.
                Err(std::io::Error::other(format!("transport: {t}")))
            }
        }
    }
}

fn to_response(resp: ureq::Response) -> TransportResponse {
    let status = resp.status();
    let headers: Vec<(String, String)> = resp
        .headers_names()
        .iter()
        .map(|name| {
            (
                name.to_lowercase(),
                resp.header(name).unwrap_or_default().to_string(),
            )
        })
        .collect();
    let mut body = Vec::new();
    let _ = resp
        .into_reader()
        .take(1024 * 1024 * 1024)
        .read_to_end(&mut body);
    TransportResponse {
        status,
        headers,
        body,
        final_url: String::new(),
    }
}

use std::io::Read as _;
