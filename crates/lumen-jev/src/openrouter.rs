//! The `OpenRouter` transport for `TypeSafe`'s Jev model (roadmap 8.9, ADR-0031).
//!
//! [`JevJudge`] posts the Decisions API body built by [`crate::decisions`] over
//! HTTPS, verifying the server against the operating system's trust store, and
//! maps the answer back through the same module. Every failure is a
//! [`JudgeError`], so the decision proceeds without Jev.
//!
//! The request runs on its own thread: the HTTP client blocks, and Lumen's ports
//! are runtime-agnostic futures. The API key is the user's own; it is never
//! logged or included in errors.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Poll, Waker};
use std::thread;
use std::time::Duration;

use lumen_application::ports::{JudgeError, JudgeFuture, JudgeModel};
use lumen_domain::{JevRequest, JudgeDescriptor, Judgment, ModelId, PromptVersion, SourceName};
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::decisions::{self, ENDPOINT, MODEL, PROMPT_VERSION};

/// Longest wait for one answer.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Largest answer read; real answers are a few hundred bytes.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// An `OpenRouter` API key. Its `Debug` output never shows the key.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Wraps a key: 1–256 visible ASCII characters, so it can never smuggle a
    /// header line or whitespace into the request.
    pub fn new(key: impl Into<String>) -> Option<Self> {
        let key = key.into();
        ((1..=256).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_graphic()))
            .then_some(Self(key))
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(redacted)")
    }
}

/// Why the judge could not be set up.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    /// The judge's descriptor constants are invalid (a programming error).
    #[error("invalid judge descriptor")]
    Descriptor,
    /// No TLS crypto provider could be installed.
    #[error("no TLS crypto provider is available")]
    Crypto,
}

/// `TypeSafe`'s Jev model through `OpenRouter`'s Decisions API.
#[derive(Debug)]
pub struct JevJudge {
    key: ApiKey,
    endpoint: String,
    agent: Agent,
    descriptor: JudgeDescriptor,
}

fn descriptor() -> Result<JudgeDescriptor, SetupError> {
    let (name, revision) = PROMPT_VERSION
        .split_once('/')
        .ok_or(SetupError::Descriptor)?;
    Ok(JudgeDescriptor {
        provider: SourceName::new("openrouter.typesafe").map_err(|_| SetupError::Descriptor)?,
        model: ModelId::new(MODEL).map_err(|_| SetupError::Descriptor)?,
        model_version: None,
        prompt_version: PromptVersion::new(
            name,
            revision.parse().map_err(|_| SetupError::Descriptor)?,
        )
        .map_err(|_| SetupError::Descriptor)?,
    })
}

/// Installs the ring crypto provider as the process default, unless another
/// provider already is.
fn ensure_crypto_provider() -> Result<(), SetupError> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    match rustls::crypto::ring::default_provider().install_default() {
        Ok(()) => Ok(()),
        // Another thread installed one first: that one is used.
        Err(_) if rustls::crypto::CryptoProvider::get_default().is_some() => Ok(()),
        Err(_) => Err(SetupError::Crypto),
    }
}

impl JevJudge {
    /// A judge using `key`, talking to `OpenRouter` over HTTPS.
    ///
    /// # Errors
    ///
    /// A [`SetupError`] if TLS or the descriptor cannot be set up.
    pub fn new(key: ApiKey) -> Result<Self, SetupError> {
        Self::with_endpoint(key, ENDPOINT.to_owned())
    }

    fn with_endpoint(key: ApiKey, endpoint: String) -> Result<Self, SetupError> {
        ensure_crypto_provider()?;
        let agent = Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::Rustls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .timeout_global(Some(TIMEOUT))
            .https_only(endpoint.starts_with("https://"))
            .build()
            .new_agent();
        Ok(Self {
            key,
            endpoint,
            agent,
            descriptor: descriptor()?,
        })
    }

    /// The answer body for `body`, or why there is none.
    fn post(&self, body: &str) -> Result<String, JudgeError> {
        let response = self
            .agent
            .post(&self.endpoint)
            .header("Authorization", &format!("Bearer {}", self.key.0))
            .header("Content-Type", "application/json")
            .send(body);
        match response {
            Ok(mut response) => response
                .body_mut()
                .with_config()
                .limit(MAX_RESPONSE_BYTES)
                .read_to_string()
                .map_err(|_| JudgeError::Unavailable("the answer could not be read")),
            Err(ureq::Error::StatusCode(code)) => Err(JudgeError::Unavailable(match code {
                401 | 403 => "OpenRouter rejected the API key",
                402 => "the OpenRouter account has no credits",
                404 => "no zero-retention endpoint serves the pinned model",
                408 | 524 => "OpenRouter timed out",
                429 => "OpenRouter rate limit reached",
                _ => "OpenRouter returned an error",
            })),
            Err(ureq::Error::Timeout(_)) => Err(JudgeError::Unavailable("the request timed out")),
            Err(_) => Err(JudgeError::Unavailable("OpenRouter could not be reached")),
        }
    }

    fn judge_blocking(&self, request: &JevRequest) -> Result<Judgment, JudgeError> {
        let body = decisions::request_body(request)
            .map_err(|_| JudgeError::Unavailable("the request could not be built"))?
            .to_string();
        let answer = self.post(&body)?;
        decisions::judgment(request, &answer).map(|(judgment, _snapshot)| judgment)
    }
}

/// A value produced on another thread.
struct Slot<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

impl JudgeModel for JevJudge {
    fn descriptor(&self) -> Option<JudgeDescriptor> {
        Some(self.descriptor.clone())
    }

    fn judge<'a>(&'a self, request: &'a JevRequest) -> JudgeFuture<'a> {
        let slot = Arc::new(Mutex::new(Slot {
            value: None,
            waker: None,
        }));
        let worker = {
            let slot = Arc::clone(&slot);
            let judge = Self {
                key: self.key.clone(),
                endpoint: self.endpoint.clone(),
                agent: self.agent.clone(),
                descriptor: self.descriptor.clone(),
            };
            let request = request.clone();
            move || {
                let answer = judge.judge_blocking(&request);
                let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
                slot.value = Some(answer);
                if let Some(waker) = slot.waker.take() {
                    waker.wake();
                }
            }
        };
        if thread::Builder::new()
            .name("jev-openrouter".to_owned())
            .spawn(worker)
            .is_err()
        {
            return Box::pin(std::future::ready(Err(JudgeError::Unavailable(
                "no thread for the request",
            ))));
        }
        Box::pin(std::future::poll_fn(move |context| {
            let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(answer) = slot.value.take() {
                Poll::Ready(answer)
            } else {
                slot.waker = Some(context.waker().clone());
                Poll::Pending
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::future::Future;
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::mpsc;

    use lumen_domain::{
        Assessment, ByteCount, CandidateSummary, Category, CoverageSummary, EvidenceId,
        EvidenceKind, ItemRef, RequestEvidence, UntrustedText,
    };
    use serde_json::json;

    use super::*;

    fn request() -> JevRequest {
        JevRequest {
            schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
            item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
            candidate: CandidateSummary {
                category: Category::AppCache,
                path_display: UntrustedText::new("~/Library/Caches/x"),
                size_bytes: ByteCount::ZERO,
                reclaimable_bytes: ByteCount::ZERO,
            },
            evidence: vec![RequestEvidence {
                id: EvidenceId::from_bytes([1; 32]),
                kind: EvidenceKind::OwningApplication,
                value: UntrustedText::new("x"),
            }],
            coverage: CoverageSummary {
                complete: true,
                notes: BTreeSet::new(),
            },
            untrusted_strings: BTreeMap::new(),
        }
    }

    /// What the local server received: request line, headers, body.
    struct Received {
        line: String,
        headers: Vec<String>,
        body: String,
    }

    /// Serves one request on localhost with `status` and `body`.
    fn serve(status: u16, body: String) -> (String, mpsc::Receiver<Received>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| unreachable!("{e}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|e| unreachable!("{e}"));
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                return;
            }
            let mut headers = Vec::new();
            let mut length = 0;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header == "\r\n" {
                    break;
                }
                if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                headers.push(header.trim_end().to_owned());
            }
            let mut request_body = vec![0; length];
            if reader.read_exact(&mut request_body).is_err() {
                return;
            }
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut stream = reader.into_inner();
            if stream.write_all(reply.as_bytes()).is_ok() {
                let _sent = sender.send(Received {
                    line: line.trim_end().to_owned(),
                    headers,
                    body: String::from_utf8_lossy(&request_body).into_owned(),
                });
            }
        });
        (format!("http://{address}/api/alpha/decisions"), receiver)
    }

    fn answer() -> String {
        json!({
            "answers": {
                "assessment": {"type": "choice", "choice": "likely_needed", "confidence": 0.9,
                    "probabilities": {"likely_removable": 0.02, "likely_needed": 0.95, "uncertain": 0.03}},
                "reason": {"type": "choice", "choice": "contains_user_data", "confidence": 0.8,
                    "probabilities": {"contains_user_data": 0.8, "owner_app_present": 0.2}},
                "injection": {"type": "noul", "noul": 0.01},
            },
            "id": "gen-dec-1",
            "model": "typesafe/jev-1.13-20260917",
            "provider": "TypeSafe",
            "usage": {"cost": 0.00002, "input_tokens": 400, "output_tokens": 60},
        })
        .to_string()
    }

    fn judge_with(endpoint: String) -> JevJudge {
        let key = ApiKey::new("sk-or-test-key").unwrap_or_else(|| unreachable!());
        JevJudge::with_endpoint(key, endpoint).unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn block<F: Future>(future: F) -> F::Output {
        struct Unpark(thread::Thread);
        impl std::task::Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Waker::from(Arc::new(Unpark(thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
            thread::park_timeout(Duration::from_millis(50));
        }
    }

    #[test]
    fn keys_are_validated_and_never_printed() {
        let key = ApiKey::new("sk-or-secret").unwrap_or_else(|| unreachable!());
        assert!(!format!("{key:?}").contains("secret"));
        for bad in ["", "has space", "line\nbreak", &"k".repeat(257)] {
            assert_eq!(ApiKey::new(bad), None, "{bad:?}");
        }
        let judge = judge_with("http://127.0.0.1:9/".to_owned());
        assert!(!format!("{judge:?}").contains("sk-or-test-key"));
    }

    #[test]
    fn a_decision_round_trips_over_http() {
        let (endpoint, received) = serve(200, answer());
        let judge = judge_with(endpoint);
        let judgment = block(judge.judge(&request()));
        assert_eq!(judgment.map(|j| j.assessment), Ok(Assessment::LikelyNeeded));
        let seen = received
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(seen.line, "POST /api/alpha/decisions HTTP/1.1");
        assert!(
            seen.headers
                .iter()
                .any(|h| h == "authorization: Bearer sk-or-test-key"
                    || h == "Authorization: Bearer sk-or-test-key")
        );
        let body: serde_json::Value =
            serde_json::from_str(&seen.body).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["provider"]["zdr"], true);
        assert_eq!(body["provider"]["data_collection"], "deny");
        assert_eq!(body["state"]["item_ref"], "c_1");
    }

    #[test]
    fn http_errors_mean_no_jev() {
        for (status, reason) in [
            (401, "OpenRouter rejected the API key"),
            (402, "the OpenRouter account has no credits"),
            (404, "no zero-retention endpoint serves the pinned model"),
            (429, "OpenRouter rate limit reached"),
            (503, "OpenRouter returned an error"),
        ] {
            let (endpoint, _received) = serve(
                status,
                json!({"error": {"code": status, "message": "x"}}).to_string(),
            );
            let answer = block(judge_with(endpoint).judge(&request()));
            assert_eq!(answer, Err(JudgeError::Unavailable(reason)), "{status}");
        }
        let (endpoint, _received) = serve(200, "{\"answers\":{}}".to_owned());
        assert_eq!(
            block(judge_with(endpoint).judge(&request())),
            Err(JudgeError::Malformed)
        );
        let unreachable = block(judge_with("http://127.0.0.1:9/".to_owned()).judge(&request()));
        assert!(matches!(unreachable, Err(JudgeError::Unavailable(_))));
    }

    #[test]
    fn the_descriptor_pins_the_model_and_prompt() {
        let judge = judge_with("http://127.0.0.1:9/".to_owned());
        let descriptor = judge.descriptor().unwrap_or_else(|| unreachable!());
        assert_eq!(descriptor.model.as_str(), MODEL);
        assert_eq!(descriptor.prompt_version.to_string(), "jev-decisions/1");
    }
}
