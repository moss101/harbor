//! Real HTTPS transport over ureq (rustls), wired to the broker contract.
//!
//! Redirects are DISABLED at the client: every hop is returned to the
//! [`crate::broker::EgressBroker`] which re-authorizes each origin change
//! and strips cross-origin credentials — the transport must never follow
//! a redirect on its own (13 policy).

use std::time::Duration;

use crate::broker::{Transport, TransportRequest, TransportResponse};

/// Connect timeout for every request (metadata and streaming).
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Idle (per-read) timeout for streaming bodies: a transfer fails only when
/// no byte arrives for this long, never because the whole body takes long.
pub use crate::broker::DEFAULT_STREAM_IDLE_TIMEOUT;

/// Cap on an error/redirect document read back from a streaming request.
/// Only 2xx bodies are streamed into the caller's sink.
const NON_SUCCESS_BODY_CAP: u64 = 64 * 1024;

pub struct UreqTransport {
    agent: ureq::Agent,
    connect_timeout: Duration,
    idle_timeout: Duration,
}

fn build_agent(connect: Duration, idle: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .user_agent("HarborLM (+https://harbor-lm.pages.dev/privacy)")
        // Agent-level timeouts bound each phase, not the whole request:
        // connect, then every individual socket read/write. Buffered
        // metadata calls additionally get an overall deadline per request
        // (see `execute`); streaming downloads do not (upgrade plan C6).
        .timeout_connect(connect)
        .timeout_read(idle)
        .timeout_write(idle)
        .build()
}

impl UreqTransport {
    pub fn new() -> Self {
        Self::with_timeouts(DEFAULT_CONNECT_TIMEOUT, DEFAULT_STREAM_IDLE_TIMEOUT)
    }

    /// Transport with explicit connect and idle (per-read) timeouts. The
    /// idle timeout is the default for streaming requests; a broker may
    /// ask for a different one per call.
    pub fn with_timeouts(connect: Duration, idle: Duration) -> Self {
        UreqTransport {
            agent: build_agent(connect, idle),
            connect_timeout: connect,
            idle_timeout: idle,
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
    ///
    /// `idle_timeout` bounds each read (and the connect phase is bounded by
    /// the transport's connect timeout); there is NO overall deadline, so a
    /// multi-GB weight on a slow phone link completes as long as bytes keep
    /// arriving. Only a 2xx body reaches `sink`; any other status (redirect,
    /// error document) is read into the returned response body, capped.
    fn execute_streaming(
        &self,
        req: &TransportRequest,
        idle_timeout: Duration,
        sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<TransportResponse> {
        // The pooled agent carries the configured idle timeout; a caller
        // asking for a different one gets a one-off agent (ureq 2 sets read
        // timeouts per agent, not per request).
        let one_off;
        let agent = if idle_timeout == self.idle_timeout {
            &self.agent
        } else {
            one_off = build_agent(self.connect_timeout, idle_timeout);
            &one_off
        };
        let mut request = agent.request(&req.method, &req.url);
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
        if !(200..300).contains(&status) {
            let mut body = Vec::new();
            let _ = resp
                .into_reader()
                .take(NON_SUCCESS_BODY_CAP)
                .read_to_end(&mut body);
            return Ok(TransportResponse {
                status,
                headers,
                body,
                final_url: String::new(),
            });
        }
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

    /// Buffered request with an OVERALL deadline of `timeout` (metadata,
    /// small JSON). Never use this for weights.
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

#[cfg(test)]
mod tests {
    //! Upgrade plan C6: a slow-but-alive body must complete regardless of
    //! total duration; a body that stalls past the idle timeout must fail.
    //! A local plain-HTTP fixture server keeps these fast (seconds).
    use super::*;
    use crate::audit::SqliteAuditSink;
    use crate::broker::{EgressBroker, EgressClass, PrivacyMode};
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::time::Instant;

    /// Serve one request: send headers, then `chunks` pieces of `chunk_len`
    /// bytes, sleeping `gap` before each piece. Returns the base URL.
    fn slow_server(chunks: usize, chunk_len: usize, gap: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            // Drain the request head.
            let mut buf = [0u8; 4096];
            let mut head = Vec::new();
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                match sock.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => head.extend_from_slice(&buf[..n]),
                }
            }
            let total = chunks * chunk_len;
            let _ = write!(
                sock,
                "HTTP/1.1 200 OK\r\ncontent-length: {total}\r\nconnection: close\r\n\r\n"
            );
            let _ = sock.flush();
            for i in 0..chunks {
                std::thread::sleep(gap);
                if sock
                    .write_all(&vec![b'a' + (i % 26) as u8; chunk_len])
                    .is_err()
                {
                    return;
                }
                let _ = sock.flush();
            }
        });
        format!("http://{addr}")
    }

    fn get(url: &str) -> TransportRequest {
        TransportRequest {
            method: "GET".into(),
            url: url.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    #[test]
    fn slow_stream_outlives_metadata_deadline_but_completes() {
        // 8 chunks x 250 ms = ~2 s total, far beyond the 500 ms metadata
        // deadline, but never idle longer than the 1 s read timeout.
        let url = slow_server(8, 1000, Duration::from_millis(250));
        let t = UreqTransport::with_timeouts(Duration::from_secs(2), Duration::from_secs(1));
        let started = Instant::now();
        let mut got = Vec::new();
        let resp = t
            .execute_streaming(&get(&url), Duration::from_secs(1), &mut |c| {
                got.extend_from_slice(c);
                Ok(())
            })
            .expect("slow-but-alive stream completes");
        assert_eq!(resp.status, 200);
        assert_eq!(got.len(), 8000);
        assert!(
            started.elapsed() > Duration::from_millis(1500),
            "fixture must actually outlast the metadata deadline"
        );

        // The same body through the buffered path hits the overall deadline.
        let url = slow_server(8, 1000, Duration::from_millis(250));
        let err = t.execute(&get(&url), Duration::from_millis(500));
        let truncated = match &err {
            Ok(r) => r.body.len() < 8000,
            Err(_) => true,
        };
        assert!(truncated, "metadata path keeps its overall deadline");
    }

    #[test]
    fn stalled_stream_fails_after_idle_timeout() {
        // First chunk arrives quickly, then the server goes silent for 3 s
        // against a 500 ms idle timeout.
        let listener_url = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                let Ok((mut sock, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf);
                let _ = sock.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 100000\r\nconnection: close\r\n\r\n",
                );
                let _ = sock.write_all(&[b'x'; 1000]);
                let _ = sock.flush();
                std::thread::sleep(Duration::from_secs(3));
            });
            format!("http://{addr}")
        };
        let t = UreqTransport::with_timeouts(Duration::from_secs(2), Duration::from_millis(500));
        let started = Instant::now();
        let mut got = 0usize;
        let err = t
            .execute_streaming(&get(&listener_url), Duration::from_millis(500), &mut |c| {
                got += c.len();
                Ok(())
            })
            .expect_err("a stalled body must fail");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "{err}");
        assert_eq!(got, 1000);
        assert!(
            started.elapsed() < Duration::from_millis(2500),
            "failed on the idle timeout, not the server closing"
        );
    }

    #[test]
    fn broker_streams_with_idle_timeout_not_request_deadline() {
        let url = slow_server(6, 500, Duration::from_millis(250));
        let broker = EgressBroker::new(Box::new(SqliteAuditSink::open_in_memory().unwrap()))
            .with_timeouts(Duration::from_millis(400), Duration::from_secs(1));
        let session = broker
            .open_session(
                EgressClass::WeightTransfer,
                &url,
                chrono::Duration::minutes(5),
                PrivacyMode::LocalOnly,
            )
            .unwrap();
        // Transport configured with a different idle value: the broker's
        // per-call idle timeout is honoured via a one-off agent.
        let t = UreqTransport::new();
        let mut got = 0usize;
        broker
            .dispatch_streaming(
                &session,
                get(&format!("{url}/blob")),
                &t,
                None,
                chrono::Utc::now(),
                &mut |c| {
                    got += c.len();
                    Ok(())
                },
            )
            .expect("~1.5 s stream completes under a 400 ms request deadline");
        assert_eq!(got, 3000);
    }

    #[test]
    fn non_success_body_is_not_streamed_into_sink() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(
                b"HTTP/1.1 302 Found\r\nlocation: /elsewhere\r\ncontent-length: 9\r\nconnection: close\r\n\r\nredirect!",
            );
        });
        let t = UreqTransport::with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
        let mut sunk = Vec::new();
        let resp = t
            .execute_streaming(
                &get(&format!("http://{addr}/blob")),
                Duration::from_secs(2),
                &mut |c| {
                    sunk.extend_from_slice(c);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(resp.status, 302);
        assert!(
            sunk.is_empty(),
            "redirect body must not reach the hash sink"
        );
        assert_eq!(resp.body, b"redirect!");
    }
}
