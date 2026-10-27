//! Recording Apple's on-device model through the `fm-judge` helper
//! (ADR-0044, `apps/fm-judge`), for evaluation only.
//!
//! The helper is started by absolute path, without a shell, and kept running:
//! each request is one `jev.request/1` line on its standard input, exactly the
//! JSON a cloud judge gets as its `state` (metadata only, never file
//! contents), and each answer is one line on its standard output. An
//! `fm.error/1` line is never an answer: its closed error code says whether
//! to retry, skip the case, or stop because the model is not available. Any
//! other line is the answer body, kept as received and checked by
//! `lumen_jev::foundation` like every recorded answer. Nothing leaves the
//! Mac.

use std::cell::RefCell;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use lumen_domain::{
    JevRequest, JudgeDescriptor, ModelId, PromptVersion, SourceName, UntrustedText,
};
use lumen_jev::foundation;
use serde::Deserialize;

use crate::record::{LiveError, LiveJudge, Pacing};

/// The longest line read from the helper.
const MAX_LINE: u64 = 64 * 1024;

/// How long one answer may take (the first includes loading the model).
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);

impl Pacing {
    /// The on-device model: no pause between requests (nothing is shared with
    /// anyone), and a rate limit or a busy model retried twice, after 2 and 4 s.
    pub const ON_DEVICE: Self = Self {
        pause: Duration::ZERO,
        attempts: 3,
        backoff: Duration::from_secs(2),
    };
}

/// Why the helper cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum FmError {
    /// The helper must be named by an absolute path.
    #[error("the helper path must be absolute: {0}")]
    RelativePath(PathBuf),
    /// It could not be run.
    #[error("the helper could not be run ({path}): {source}")]
    Run {
        /// The helper.
        path: PathBuf,
        /// What failed.
        source: std::io::Error,
    },
    /// It did not answer `--check` as an `fm-judge` of this prompt does.
    #[error("the helper's status is not understood: {0}")]
    Protocol(&'static str),
    /// The on-device model is not available.
    #[error("the on-device model is not available: {0}")]
    Unavailable(&'static str),
}

/// `fm-judge --check`'s line (`fm.status/1`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Status {
    schema: String,
    available: bool,
    #[serde(default)]
    error: Option<String>,
    model: String,
    model_version: String,
    prompt_version: String,
    context_size: u32,
}

/// An `fm.error/1` line.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code, reason = "accepted fields that Lumen does not use")]
struct ErrorLine {
    schema: String,
    #[serde(default)]
    item_ref: Option<String>,
    error: String,
}

/// What a helper error code means for the recording.
pub fn live_error(code: &str) -> LiveError {
    match code {
        "apple_intelligence_not_enabled" => LiveError::Stopped(
            "Apple Intelligence is not enabled (System Settings > Apple Intelligence & Siri)",
        ),
        "model_not_ready" => {
            LiveError::Stopped("the on-device model is not ready yet (it may still be downloading)")
        }
        "device_not_eligible" => {
            LiveError::Stopped("this Mac is not eligible for Apple Intelligence")
        }
        "unavailable" | "assets_unavailable" => {
            LiveError::Stopped("the on-device model is not available")
        }
        "rate_limited" => LiveError::Transient("the on-device model is rate limited"),
        "concurrent_requests" => LiveError::Transient("the on-device model is busy"),
        "context_exceeded" => {
            LiveError::Failed("the item does not fit the on-device model's context")
        }
        "guardrail" => LiveError::Failed("the on-device model's guardrails stopped the answer"),
        "refusal" => LiveError::Failed("the on-device model refused to answer"),
        "decoding_failure" => {
            LiveError::Failed("the on-device model's answer could not be decoded")
        }
        "unsupported_guide" => {
            LiveError::Failed("the on-device model does not support the answer's schema")
        }
        "unsupported_language" => {
            LiveError::Failed("the on-device model took the item for an unsupported language")
        }
        "bad_request" => LiveError::Failed("the helper did not understand the request"),
        _ => LiveError::Failed("the on-device model failed"),
    }
}

/// The error an answer line reports, if it is an `fm.error/1` line.
fn reported_error(line: &str) -> Option<LiveError> {
    let error: ErrorLine = serde_json::from_str(line).ok()?;
    (error.schema == "fm.error/1").then(|| live_error(&error.error))
}

/// Ends the helper and reaps it. Killing fails only when it has already
/// exited, and its exit status says nothing about the answers recorded, so
/// neither result matters.
fn end(child: &mut Child) {
    child.kill().ok();
    child.wait().ok();
}

/// A running helper.
struct Running {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Result<String, ()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        end(&mut self.child);
    }
}

/// Apple's on-device model behind the `fm-judge` helper.
pub struct FmHelper {
    path: PathBuf,
    timeout: Duration,
    descriptor: JudgeDescriptor,
    running: RefCell<Option<Running>>,
}

impl std::fmt::Debug for FmHelper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FmHelper")
            .field("path", &self.path)
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl FmHelper {
    /// Checks that the helper at `path` (absolute) answers with this prompt
    /// and that its model is available, and returns the judge to record,
    /// whose descriptor carries the model version (OS build).
    ///
    /// # Errors
    ///
    /// [`FmError::Unavailable`] when the model is not available (Apple
    /// Intelligence off, model not ready, Mac not eligible), and another
    /// [`FmError`] when the helper cannot be run or is not understood.
    pub fn start(path: &Path, timeout: Duration) -> Result<Self, FmError> {
        if !path.is_absolute() {
            return Err(FmError::RelativePath(path.to_owned()));
        }
        let run = |source| FmError::Run {
            path: path.to_owned(),
            source,
        };
        let output = Command::new(path)
            .arg("--check")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(run)?;
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text
            .lines()
            .next()
            .ok_or(FmError::Protocol("no status line"))?;
        let status: Status =
            serde_json::from_str(line).map_err(|_| FmError::Protocol("not an fm.status/1 line"))?;
        if status.schema != "fm.status/1"
            || status.model != foundation::MODEL
            || status.prompt_version != foundation::PROMPT_VERSION
        {
            return Err(FmError::Protocol(
                "another model or prompt: rebuild the helper with `just fm-judge`",
            ));
        }
        if !foundation::is_model_version(&status.model_version) || status.context_size == 0 {
            return Err(FmError::Protocol("invalid model version or context size"));
        }
        if !status.available || !output.status.success() {
            let reason = match live_error(status.error.as_deref().unwrap_or("unavailable")) {
                LiveError::Stopped(reason) => reason,
                _ => "the on-device model is not available",
            };
            return Err(FmError::Unavailable(reason));
        }
        let descriptor = JudgeDescriptor {
            provider: SourceName::new(foundation::PROVIDER)
                .map_err(|_| FmError::Protocol("invalid provider name"))?,
            model: ModelId::new(foundation::MODEL)
                .map_err(|_| FmError::Protocol("invalid model name"))?,
            model_version: Some(UntrustedText::new(status.model_version)),
            prompt_version: foundation::PROMPT_VERSION
                .split_once('/')
                .and_then(|(name, revision)| PromptVersion::new(name, revision.parse().ok()?).ok())
                .ok_or(FmError::Protocol("invalid prompt version"))?,
        };
        Ok(Self {
            path: path.to_owned(),
            timeout,
            descriptor,
            running: RefCell::new(None),
        })
    }

    fn spawn(&self) -> Result<Running, LiveError> {
        let failed = LiveError::Stopped("the helper could not be started");
        let mut child = Command::new(&self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| failed)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            end(&mut child);
            return Err(failed);
        };
        let (send, lines) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let line = match reader
                    .by_ref()
                    .take(MAX_LINE + 1)
                    .read_until(b'\n', &mut line)
                {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() as u64 > MAX_LINE || !line.ends_with(b"\n") => Err(()),
                    Ok(_) => String::from_utf8(line)
                        .map(|l| l.trim_end().to_owned())
                        .map_err(|_| ()),
                };
                let stop = line.is_err();
                if send.send(line).is_err() || stop {
                    break;
                }
            }
        });
        Ok(Running {
            child,
            stdin,
            lines,
        })
    }
}

impl LiveJudge for FmHelper {
    fn descriptor(&self) -> JudgeDescriptor {
        self.descriptor.clone()
    }

    fn ask(&self, request: &JevRequest) -> Result<String, LiveError> {
        let line = serde_json::to_string(request)
            .map_err(|_| LiveError::Failed("the request could not be serialized"))?;
        let mut slot = self.running.borrow_mut();
        let running = match slot.take() {
            Some(running) => slot.insert(running),
            None => slot.insert(self.spawn()?),
        };
        let exited = LiveError::Stopped("the helper exited");
        if writeln!(running.stdin, "{line}")
            .and_then(|()| running.stdin.flush())
            .is_err()
        {
            *slot = None;
            return Err(exited);
        }
        match running.lines.recv_timeout(self.timeout) {
            Ok(Ok(answer)) => reported_error(&answer).map_or(Ok(answer), Err),
            Ok(Err(())) | Err(RecvTimeoutError::Disconnected) => {
                *slot = None;
                Err(exited)
            }
            Err(RecvTimeoutError::Timeout) => {
                // A late answer would be read as the next one: start afresh.
                *slot = None;
                Err(LiveError::Failed(
                    "the on-device model did not answer in time",
                ))
            }
        }
    }

    fn wait(&self, pause: Duration) {
        std::thread::sleep(pause);
    }
}

/// The protocol, against fake helpers: shell scripts, so Unix only (the real
/// helper is macOS only).
#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use lumen_application::ports::{JudgeError, JudgeModel as _};
    use lumen_domain::Assessment;
    use lumen_jev::decisions::{self, Prompt, Provider};

    use super::*;
    use crate::{Cassette, CassetteError, RecordOutcome, ReplayJudge, block_on, load, record};

    const VERSION: &str = "macOS 26.6.2 (25G83)";

    fn status(available: bool, error: &str) -> String {
        format!(
            r#"{{"available":{available},"context_size":4096,{error}"model":"apple.foundation-models","model_version":"{VERSION}","prompt_version":"fm-decisions/1","schema":"fm.status/1"}}"#
        )
    }

    /// The answer the fake gives, in shell, for the request in `$line` whose
    /// item is `$item`.
    const ANSWER: &str = r#"printf '{"answers":{"assessment":"uncertain","injection_suspected":false,"reason":"insufficient_evidence"},"elapsed_ms":5,"item_ref":"%s","model":"apple.foundation-models","model_version":"macOS 26.6.2 (25G83)","prompt_version":"fm-decisions/1","schema":"fm.answer/1"}\n' "$item""#;

    /// A fake helper in `dir`: `--check` prints `check` and exits with
    /// `code`; otherwise it logs every request line to `requests.log` and
    /// runs `body` for it (with `$line`, `$item` and `$n`, the request's
    /// number from 1).
    fn helper(dir: &Path, check: &str, code: i32, body: &str) -> PathBuf {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let log = dir.join("requests.log");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = --check ]; then printf '%s\\n' '{check}'; exit {code}; fi\nn=0\nwhile IFS= read -r line; do\n  n=$((n+1))\n  printf '%s\\n' \"$line\" >> '{}'\n  item=$(printf '%s' \"$line\" | sed 's/.*\"item_ref\":\"\\([^\"]*\\)\".*/\\1/')\n  {body}\ndone\n",
            log.display()
        );
        // A new executable file each time, so a helper still running keeps
        // its own script; nothing is replaced.
        let path = dir.join(format!(
            "fm-judge-{}",
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&path)
            .and_then(|mut f| f.write_all(script.as_bytes()))
            .unwrap_or_else(|e| unreachable!("{e}"));
        path
    }

    fn available(dir: &Path, body: &str) -> FmHelper {
        let path = helper(dir, &status(true, ""), 0, body);
        FmHelper::start(&path, Duration::from_secs(10)).unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn dataset() -> crate::Dataset {
        let ai = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ai");
        load(&ai, "2026.10.0").unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn the_check_names_the_model_version_and_unavailability() {
        let dir = tempdir();
        let judge = available(dir.path(), ANSWER);
        let descriptor = judge.descriptor();
        assert_eq!(descriptor.provider.as_str(), "apple.fm");
        assert_eq!(descriptor.model.as_str(), "apple.foundation-models");
        assert_eq!(
            descriptor.model_version.as_ref().map(UntrustedText::raw),
            Some(VERSION)
        );
        assert_eq!(descriptor.prompt_version.to_string(), "fm-decisions/1");

        for (code, exit, message) in [
            (
                "apple_intelligence_not_enabled",
                3,
                "Apple Intelligence is not enabled",
            ),
            ("model_not_ready", 4, "not ready"),
            ("device_not_eligible", 5, "not eligible"),
        ] {
            let error = format!(r#""error":"{code}","#);
            let path = helper(dir.path(), &status(false, &error), exit, ANSWER);
            let result = FmHelper::start(&path, ANSWER_TIMEOUT);
            assert!(
                matches!(&result, Err(FmError::Unavailable(m)) if m.contains(message)),
                "{code}: {result:?}"
            );
        }
        let other = status(true, "").replace("fm-decisions/1", "fm-decisions/2");
        let path = helper(dir.path(), &other, 0, ANSWER);
        assert!(matches!(
            FmHelper::start(&path, ANSWER_TIMEOUT),
            Err(FmError::Protocol(_))
        ));
        assert!(matches!(
            FmHelper::start(Path::new("fm-judge"), ANSWER_TIMEOUT),
            Err(FmError::RelativePath(_))
        ));
        assert!(matches!(
            FmHelper::start(&dir.path().join("missing"), ANSWER_TIMEOUT),
            Err(FmError::Run { .. })
        ));
    }

    #[test]
    fn a_recording_replays_and_the_helper_gets_only_the_request() -> Result<(), CassetteError> {
        let dir = tempdir();
        let dataset = dataset();
        // One case's answer is not an answer at all, and one names an option
        // the helper never offers: both are recorded and refused, like Jeb's.
        let refused = dataset.golden[1].request.item_ref.as_str().to_owned();
        let odd = dataset.golden[2].request.item_ref.as_str().to_owned();
        let body = format!(
            "case \"$item\" in {refused}) echo 'not json' ;; {odd}) {} ;; *) {ANSWER} ;; esac",
            ANSWER.replace("insufficient_evidence", "because_i_said_so")
        );
        let judge = available(dir.path(), &body);
        let recording = record(&judge, &dataset, Pacing::ON_DEVICE, None, &mut |_, _| {})?;
        drop(judge);
        assert!(recording.complete());
        let refusals = recording
            .outcomes
            .iter()
            .filter(|(_, o)| matches!(o, RecordOutcome::Refused(JudgeError::Malformed)))
            .count();
        assert_eq!(refusals, 2);

        // The helper received, line by line, exactly the request a cloud
        // judge gets as its state: metadata only, never file contents.
        let log = std::fs::read_to_string(dir.path().join("requests.log"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let sent: Vec<&str> = log.lines().collect();
        assert_eq!(sent.len(), dataset.golden.len() + dataset.adversarial.len());
        for (line, case) in sent.iter().zip(dataset.cases()) {
            let request: JevRequest = serde_json::from_str(line)?;
            assert_eq!(&request, &case.request);
            let state = decisions::request_body_for(&case.request, Provider::Jeb, Prompt::V2)?;
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(line)?,
                state["state"]
            );
        }

        // The cassette replays to the same judgments the recorder validated.
        let text = serde_json::to_string_pretty(&recording.cassette)?;
        let replay = ReplayJudge::new(Cassette::parse(&text)?);
        for case in dataset.cases() {
            let answer = block_on(replay.judge(&case.request));
            if case.request.item_ref.as_str() == refused || case.request.item_ref.as_str() == odd {
                assert_eq!(answer, Err(JudgeError::Malformed));
            } else {
                assert_eq!(answer.map(|j| j.assessment), Ok(Assessment::Uncertain));
            }
        }
        Ok(())
    }

    #[test]
    fn unavailability_stops_the_recording_and_is_never_recorded() -> Result<(), CassetteError> {
        let dir = tempdir();
        let dataset = dataset();
        let body = format!(
            r#"if [ "$n" -ge 3 ]; then printf '{{"error":"apple_intelligence_not_enabled","item_ref":"%s","schema":"fm.error/1"}}\n' "$item"; else {ANSWER}; fi"#
        );
        let judge = available(dir.path(), &body);
        let recording = record(&judge, &dataset, Pacing::ON_DEVICE, None, &mut |_, _| {})?;
        assert_eq!(recording.cassette.entries.len(), 2);
        assert_eq!(
            recording.outcomes.get(2).map(|(_, o)| o),
            Some(&RecordOutcome::Failed(LiveError::Stopped(
                "Apple Intelligence is not enabled (System Settings > Apple Intelligence & Siri)"
            )))
        );
        assert!(
            recording
                .outcomes
                .iter()
                .skip(3)
                .all(|(_, o)| *o == RecordOutcome::NotSent)
        );
        Ok(())
    }

    #[test]
    fn errors_retry_skip_or_stop() {
        let dir = tempdir();
        let request = &dataset().golden[0].request;
        let error = |code: &str| {
            let body = format!(
                r#"printf '{{"error":"{code}","item_ref":"%s","schema":"fm.error/1"}}\n' "$item""#
            );
            available(dir.path(), &body).ask(request)
        };
        assert!(matches!(
            error("model_not_ready"),
            Err(LiveError::Stopped(_))
        ));
        assert!(matches!(
            error("rate_limited"),
            Err(LiveError::Transient(_))
        ));
        assert!(matches!(
            error("context_exceeded"),
            Err(LiveError::Failed(_))
        ));
        assert!(matches!(error("guardrail"), Err(LiveError::Failed(_))));
        assert!(matches!(error("something_new"), Err(LiveError::Failed(_))));

        // A helper that exits stops the recording; one that hangs is
        // abandoned and started afresh for the next request.
        let gone = available(dir.path(), "exit 1");
        assert_eq!(
            gone.ask(request),
            Err(LiveError::Stopped("the helper exited"))
        );
        let path = helper(
            dir.path(),
            &status(true, ""),
            0,
            &format!(
                r#"if [ ! -e "{0}/slow" ]; then : > "{0}/slow"; sleep 5; fi; {ANSWER}"#,
                dir.path().display()
            ),
        );
        let slow = FmHelper::start(&path, Duration::from_millis(500))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(matches!(slow.ask(request), Err(LiveError::Failed(_))));
        assert!(slow.ask(request).is_ok(), "a fresh helper answers");
    }
}
