//! The HTTPS transport for the judge: `TypeSafe`'s Jev through `OpenRouter`
//! (roadmap 8.9, ADR-0031), or Jeb on the owner's school AI Hub, which speaks
//! the same Decisions protocol (ADR-0036).
//!
//! [`JevJudge`] posts the Decisions API body built by [`crate::decisions`] over
//! HTTPS, verifying the server against the operating system's trust store, and
//! maps the answer back through the same module. Every failure is a
//! [`JudgeError`], so the decision proceeds without Jev. [`JevJudge::ask`]
//! returns the answer exactly as received, before validation, for recording
//! evaluation cassettes (docs/ai/harness.md). The same judge also describes
//! items left for review ([`Describer`], ADR-0041), with the description
//! questions and their own prompt version.
//!
//! The request runs on its own thread: the HTTP client blocks, and Lumen's ports
//! are runtime-agnostic futures. The API key is the user's own; it is never
//! logged or included in errors.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Poll, Waker};
use std::thread;
use std::time::Duration;

use lumen_application::ports::{Describer, JudgeError, JudgeFuture, JudgeModel, NoteFuture};
use lumen_domain::{
    ItemNote, JevRequest, JudgeDescriptor, Judgment, ModelId, PromptVersion, SourceName,
};
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::decisions::{self, DESCRIBE_PROMPT_VERSION, PROMPT_VERSION, Provider};

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

/// Why no answer could be fetched. It never contains the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct AskError {
    /// The HTTP status the server answered with, if it answered.
    pub status: Option<u16>,
    /// What went wrong, for people.
    pub reason: &'static str,
}

impl AskError {
    const fn new(status: Option<u16>, reason: &'static str) -> Self {
        Self { status, reason }
    }

    /// Whether trying again later may help: no answer, a timeout, a rate
    /// limit or a server error.
    pub const fn is_transient(self) -> bool {
        matches!(self.status, None | Some(408 | 429 | 500..=599))
    }

    /// Whether the server rejected the key: every further request would too.
    pub const fn is_key_rejected(self) -> bool {
        matches!(self.status, Some(401 | 403))
    }
}

impl From<AskError> for JudgeError {
    fn from(error: AskError) -> Self {
        Self::Unavailable(error.reason)
    }
}

/// `TypeSafe`'s Jev model through `OpenRouter`'s Decisions API.
#[derive(Debug)]
pub struct JevJudge {
    key: ApiKey,
    provider: Provider,
    endpoint: String,
    agent: Agent,
    descriptor: JudgeDescriptor,
    describer: JudgeDescriptor,
}

fn descriptor(provider: Provider, prompt: &str) -> Result<JudgeDescriptor, SetupError> {
    let (name, revision) = prompt.split_once('/').ok_or(SetupError::Descriptor)?;
    Ok(JudgeDescriptor {
        provider: SourceName::new(provider.source()).map_err(|_| SetupError::Descriptor)?,
        model: ModelId::new(provider.model()).map_err(|_| SetupError::Descriptor)?,
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
        Self::with_endpoint(
            key,
            Provider::OpenRouter,
            Provider::OpenRouter.endpoint().to_owned(),
        )
    }

    /// A judge using `key`, talking to Jeb on the school AI Hub over HTTPS
    /// (ADR-0036).
    ///
    /// # Errors
    ///
    /// A [`SetupError`] if TLS or the descriptor cannot be set up.
    pub fn jeb(key: ApiKey) -> Result<Self, SetupError> {
        Self::with_endpoint(key, Provider::Jeb, Provider::Jeb.endpoint().to_owned())
    }

    fn with_endpoint(
        key: ApiKey,
        provider: Provider,
        endpoint: String,
    ) -> Result<Self, SetupError> {
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
            provider,
            endpoint,
            agent,
            descriptor: descriptor(provider, PROMPT_VERSION)?,
            describer: descriptor(provider, DESCRIBE_PROMPT_VERSION)?,
        })
    }

    /// The service this judge asks.
    pub const fn provider(&self) -> Provider {
        self.provider
    }

    /// Sends the Decisions request for `request` once and returns the answer
    /// body exactly as the server sent it, before any validation;
    /// [`JudgeModel::judge`] validates it with
    /// [`decisions::judgment_for`]. Blocks the calling thread for up to 30 s.
    ///
    /// # Errors
    ///
    /// An [`AskError`] if no answer arrived (unreachable, timed out, or an
    /// HTTP error status).
    pub fn ask(&self, request: &JevRequest) -> Result<String, AskError> {
        let body = decisions::request_body_for(request, self.provider)
            .map_err(|_| AskError::new(None, "the request could not be built"))?
            .to_string();
        self.post(&body)
    }

    /// The answer body for `body`, or why there is none.
    fn post(&self, body: &str) -> Result<String, AskError> {
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
                .map_err(|_| AskError::new(None, "the answer could not be read")),
            Err(ureq::Error::StatusCode(code)) => Err(AskError::new(
                Some(code),
                match (self.provider, code) {
                    (Provider::OpenRouter, 401 | 403) => "OpenRouter rejected the API key",
                    (Provider::OpenRouter, 402) => "the OpenRouter account has no credits",
                    (Provider::OpenRouter, 404) => {
                        "no zero-retention endpoint serves the pinned model"
                    }
                    (Provider::OpenRouter, 408 | 524) => "OpenRouter timed out",
                    (Provider::OpenRouter, 429) => "OpenRouter rate limit reached",
                    (Provider::OpenRouter, _) => "OpenRouter returned an error",
                    (Provider::Jeb, 401 | 403) => "the Jeb server rejected the API key",
                    (Provider::Jeb, 429) => {
                        "the Jeb server allows one request at a time, or the quota is used up"
                    }
                    (Provider::Jeb, 502..=504) => {
                        "the Jeb server is busy or the request was too long"
                    }
                    (Provider::Jeb, _) => "the Jeb server returned an error",
                },
            )),
            Err(ureq::Error::Timeout(_)) => Err(AskError::new(None, "the request timed out")),
            Err(_) => Err(AskError::new(
                None,
                match self.provider {
                    Provider::OpenRouter => "OpenRouter could not be reached",
                    Provider::Jeb => "the Jeb server could not be reached",
                },
            )),
        }
    }

    fn judge_blocking(&self, request: &JevRequest) -> Result<Judgment, JudgeError> {
        let answer = self.ask(request)?;
        decisions::judgment_for(request, &answer, self.provider)
            .map(|(judgment, _snapshot)| judgment)
    }

    fn describe_blocking(&self, request: &JevRequest) -> Result<ItemNote, JudgeError> {
        let body = decisions::describe_body_for(request, self.provider)
            .map_err(|_| AskError::new(None, "the request could not be built"))?
            .to_string();
        let answer = self.post(&body)?;
        decisions::note_for(&answer, self.provider).map(|(note, _snapshot)| note)
    }

    /// Runs `work` with a copy of this judge on its own thread, as a future.
    fn on_thread<'a, T: Send + 'static>(
        &self,
        request: &JevRequest,
        work: fn(&Self, &JevRequest) -> Result<T, JudgeError>,
    ) -> Pin<Box<dyn Future<Output = Result<T, JudgeError>> + Send + 'a>> {
        let slot = Arc::new(Mutex::new(Slot {
            value: None,
            waker: None,
        }));
        let worker = {
            let slot = Arc::clone(&slot);
            let judge = Self {
                key: self.key.clone(),
                provider: self.provider,
                endpoint: self.endpoint.clone(),
                agent: self.agent.clone(),
                descriptor: self.descriptor.clone(),
                describer: self.describer.clone(),
            };
            let request = request.clone();
            move || {
                let answer = work(&judge, &request);
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
        self.on_thread(request, Self::judge_blocking)
    }
}

impl Describer for JevJudge {
    fn descriptor(&self) -> Option<JudgeDescriptor> {
        Some(self.describer.clone())
    }

    fn describe<'a>(&'a self, request: &'a JevRequest) -> NoteFuture<'a> {
        self.on_thread(request, Self::describe_blocking)
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
    use crate::decisions::MODEL;

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
        JevJudge::with_endpoint(key, Provider::OpenRouter, endpoint)
            .unwrap_or_else(|e| unreachable!("{e}"))
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
    fn ask_returns_the_answer_exactly_as_received() {
        // Spacing and key order kept: a recording is what the validator saw.
        let raw = format!(" {}\n", answer().replace(':', ": "));
        let (endpoint, _received) = serve(200, raw.clone());
        let judge = judge_with(endpoint);
        assert_eq!(judge.provider(), Provider::OpenRouter);
        assert_eq!(judge.ask(&request()), Ok(raw));
        for (status, transient, key_rejected) in [
            (401, false, true),
            (400, false, false),
            (429, true, false),
            (503, true, false),
        ] {
            let (endpoint, _received) = serve(status, "{}".to_owned());
            let error = judge_with(endpoint).ask(&request());
            let Err(error) = error else {
                unreachable!("{status}: {error:?}");
            };
            assert_eq!(error.status, Some(status));
            assert_eq!(error.is_transient(), transient, "{status}");
            assert_eq!(error.is_key_rejected(), key_rejected, "{status}");
            assert!(!format!("{error} {error:?}").contains("sk-or-test-key"));
        }
        let unreachable = judge_with("http://127.0.0.1:9/".to_owned()).ask(&request());
        assert!(unreachable.is_err_and(AskError::is_transient));
    }

    #[test]
    fn the_descriptor_pins_the_model_and_prompt() {
        let judge = judge_with("http://127.0.0.1:9/".to_owned());
        let descriptor = JudgeModel::descriptor(&judge).unwrap_or_else(|| unreachable!());
        assert_eq!(descriptor.model.as_str(), MODEL);
        assert_eq!(descriptor.prompt_version.to_string(), "jev-decisions/1");
        let describer = Describer::descriptor(&judge).unwrap_or_else(|| unreachable!());
        assert_eq!(describer.model.as_str(), MODEL);
        assert_eq!(describer.prompt_version.to_string(), "jev-describe/1");
    }

    #[test]
    fn a_description_round_trips_over_http() {
        let reply = json!({
            "answers": {
                "kind": {"type": "choice", "choice": "downloads_or_installers",
                    "probabilities": {"downloads_or_installers": 0.91, "unknown": 0.09}},
                "injection": {"type": "noul", "noul": 0.02},
            },
            "model": "typesafe/jev-1.13",
            "usage": {"input_tokens": 400, "output_tokens": 0},
        })
        .to_string();
        let (endpoint, received) = serve(200, reply);
        let note = block(judge_with(endpoint).describe(&request()));
        assert_eq!(
            note.map(|n| (n.kind, n.confidence)),
            Ok((
                lumen_domain::ItemKind::DownloadsOrInstallers,
                lumen_domain::JudgmentConfidence::High
            ))
        );
        let seen = received
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let body: serde_json::Value =
            serde_json::from_str(&seen.body).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(body["questions"]["kind"]["type"], "choice");
        assert_eq!(body["provider"]["zdr"], true);
        assert_eq!(body["state"]["item_ref"], "c_1");

        let (endpoint, _received) = serve(429, "{}".to_owned());
        assert_eq!(
            block(judge_with(endpoint).describe(&request())),
            Err(JudgeError::Unavailable("OpenRouter rate limit reached"))
        );
    }
}
