//! Local, redacted telemetry for Lumen (ADR-0022, roadmap 12.3).
//!
//! Lumen instruments its work with `tracing` spans and events. This crate
//! turns them into one JSON line per event, written to two local sinks only:
//!
//! - a daily rolling log file (`lumen.log.<date>`, UTC date) in a folder the
//!   app owns, of which only the latest [`KEEP_DAILY_LOGS`] files are kept;
//! - an in-memory ring buffer of the latest events, which the person can
//!   review and export for support ([`Telemetry::recent`]).
//!
//! Nothing is ever sent anywhere. At `info` and above, any field that may
//! carry a path, a name or a content hash ([`is_sensitive`]) is written as
//! `"[redacted]"`; `debug` and `trace` keep raw values for local diagnosis
//! only and are off by default. Event messages must never interpolate paths
//! or names; put them in fields so this layer sees them.

use std::collections::VecDeque;
use std::fmt;
use std::io::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt as _};
use tracing_subscriber::registry::LookupSpan;

/// The value written instead of a sensitive field.
pub const REDACTED: &str = "[redacted]";

/// The file name prefix of the daily log: files are named `lumen.log.<date>`.
const LOG_PREFIX: &str = "lumen.log";

/// How many daily log files are kept; older ones are removed.
///
/// The removal is done by `tracing-appender` itself (Lumen writes no deletion
/// code outside `lumen-fs-exec`). It prunes when telemetry starts and at each
/// daily rollover: if the folder holds this many or more regular files whose
/// names start with `lumen.log`, the oldest (by creation time, else by the
/// date in the name) are removed until one fewer remain, and then the current
/// day's file is opened, so at most this many remain. Directories, symbolic
/// links and files with other names are never touched, which is why the log
/// folder must hold Lumen's logs only.
pub const KEEP_DAILY_LOGS: usize = 14;

/// Why telemetry could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TelemetryError {
    /// The log folder could not be created.
    #[error("the log folder could not be created: {0}")]
    LogDir(std::io::Error),
    /// The rolling log file could not be opened.
    #[error("the log file could not be opened: {0}")]
    LogFile(InitError),
    /// A global subscriber was already installed.
    #[error("telemetry was already started")]
    AlreadyStarted,
}

/// Whether a field may carry a path, a name or a content hash, so it is
/// redacted at `info` and above.
pub fn is_sensitive(field: &str) -> bool {
    const EXACT: [&str; 7] = [
        "path",
        "name",
        "file",
        "root",
        "hash",
        "original",
        "message_path",
    ];
    const SUFFIXES: [&str; 3] = ["_path", "_name", "_hash"];
    EXACT.contains(&field) || SUFFIXES.iter().any(|s| field.ends_with(s))
}

/// The latest events, oldest first, bounded.
#[derive(Debug, Clone)]
pub struct Ring {
    lines: Arc<Mutex<VecDeque<String>>>,
    capacity: usize,
}

impl Ring {
    /// A ring holding at most `capacity` events.
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    fn push(&self, line: String) {
        if self.capacity == 0 {
            return;
        }
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        if lines.len() == self.capacity {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// The events held, oldest first, as JSON lines.
    pub fn lines(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

/// Collects an event's fields, redacting sensitive ones when `redact`.
struct Fields {
    map: Map<String, Value>,
    redact: bool,
}

impl Fields {
    fn insert(&mut self, field: &Field, value: Value) {
        let name = field.name();
        let value = if self.redact && is_sensitive(name) {
            Value::String(REDACTED.to_owned())
        } else {
            value
        };
        self.map.insert(name.to_owned(), value);
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field, Value::String(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::String(value.to_owned()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        // Decimal strings, like every other 64-bit count Lumen exports (ADR-0011).
        self.insert(field, Value::String(value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, Value::Bool(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, Value::from(value));
    }
}

/// The layer that writes every event as a redacted JSON line.
pub struct JsonLayer {
    file: Option<Mutex<NonBlocking>>,
    ring: Ring,
}

impl fmt::Debug for JsonLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonLayer")
            .field("file", &self.file.is_some())
            .field("ring", &self.ring)
            .finish()
    }
}

/// One event as a JSON line.
fn line<S>(event: &Event<'_>, ctx: &Context<'_, S>) -> String
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let level = *event.metadata().level();
    let mut fields = Fields {
        map: Map::new(),
        redact: level <= Level::INFO,
    };
    event.record(&mut fields);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let mut record = Map::new();
    record.insert("ts_ms".into(), Value::String(millis.to_string()));
    record.insert("level".into(), Value::String(level.to_string()));
    record.insert(
        "target".into(),
        Value::String(event.metadata().target().to_owned()),
    );
    if let Some(span) = ctx.event_span(event) {
        let names: Vec<Value> = span
            .scope()
            .from_root()
            .map(|s| Value::String(s.name().to_owned()))
            .collect();
        record.insert("spans".into(), Value::Array(names));
    }
    record.insert("fields".into(), Value::Object(fields.map));
    Value::Object(record).to_string()
}

impl<S> Layer<S> for JsonLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let text = line(event, &ctx);
        if let Some(file) = &self.file {
            let mut writer = file.lock().unwrap_or_else(PoisonError::into_inner);
            // A lost log line must never disturb the work being logged.
            if writeln!(writer, "{text}").is_err() {
                self.ring.push(
                    r#"{"level":"WARN","fields":{"message":"a log line could not be written"}}"#
                        .to_owned(),
                );
            }
        }
        self.ring.push(text);
    }
}

/// What to record and where.
#[derive(Debug, Clone)]
pub struct Config<'a> {
    /// The folder for the rolling log file; `None` keeps the ring buffer only.
    pub log_dir: Option<&'a Path>,
    /// The most detailed level recorded.
    pub level: Level,
    /// How many recent events the ring buffer keeps.
    pub ring: usize,
}

impl Default for Config<'_> {
    fn default() -> Self {
        Self {
            log_dir: None,
            level: Level::INFO,
            ring: 1000,
        }
    }
}

/// Running telemetry: keep it alive for as long as events should be written.
#[derive(Debug)]
pub struct Telemetry {
    ring: Ring,
    _guard: Option<WorkerGuard>,
}

impl Telemetry {
    /// The latest events, oldest first, as redacted JSON lines, for the person
    /// to review and export.
    pub fn recent(&self) -> Vec<String> {
        self.ring.lines()
    }
}

/// The layer, its ring buffer and the file writer's guard, without installing
/// anything globally (used by [`start`] and by tests).
///
/// # Errors
///
/// [`TelemetryError::LogDir`] if the log folder cannot be created, or
/// [`TelemetryError::LogFile`] if the day's log file cannot be opened.
pub fn layer(
    config: &Config<'_>,
) -> Result<(JsonLayer, Ring, Option<WorkerGuard>), TelemetryError> {
    let ring = Ring::new(config.ring);
    let (file, guard) = match config.log_dir {
        Some(dir) => {
            std::fs::create_dir_all(dir).map_err(TelemetryError::LogDir)?;
            let appender = RollingFileAppender::builder()
                .rotation(Rotation::DAILY)
                .filename_prefix(LOG_PREFIX)
                .max_log_files(KEEP_DAILY_LOGS)
                .build(dir)
                .map_err(TelemetryError::LogFile)?;
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (Some(Mutex::new(writer)), Some(guard))
        }
        None => (None, None),
    };
    Ok((
        JsonLayer {
            file,
            ring: ring.clone(),
        },
        ring,
        guard,
    ))
}

/// Installs Lumen's telemetry as the global subscriber.
///
/// # Errors
///
/// A [`TelemetryError`] if the log folder or file cannot be created or
/// telemetry is already running.
pub fn start(config: &Config<'_>) -> Result<Telemetry, TelemetryError> {
    let (json, ring, guard) = layer(config)?;
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            config.level,
        ))
        .with(json);
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|_| TelemetryError::AlreadyStarted)?;
    Ok(Telemetry {
        ring,
        _guard: guard,
    })
}

#[cfg(test)]
mod tests {
    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn record(level: Level, ring: usize, events: impl FnOnce()) -> Vec<Value> {
        let config = Config {
            log_dir: None,
            level,
            ring,
        };
        let (json, ring, _guard) = layer(&config).unwrap_or_else(|e| unreachable!("{e}"));
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::filter::LevelFilter::from_level(level))
            .with(json);
        tracing::subscriber::with_default(subscriber, events);
        ring.lines()
            .iter()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| unreachable!("{e}")))
            .collect()
    }

    #[test]
    fn sensitive_fields_are_redacted_at_info_and_above() {
        let events = record(Level::DEBUG, 10, || {
            let span = tracing::info_span!("quarantine", op_id = "op-1");
            let _entered = span.enter();
            tracing::info!(path = "/Users/ana/secret", files = 3_u64, "moved");
            tracing::warn!(original_path = "/Users/ana/x", "kept");
            tracing::debug!(path = "/Users/ana/debug", "diagnosis");
        });
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["fields"]["path"], REDACTED);
        assert_eq!(events[0]["fields"]["files"], "3", "u64 as a decimal string");
        assert_eq!(events[0]["fields"]["message"], "moved");
        assert_eq!(events[0]["spans"][0], "quarantine");
        assert_eq!(events[1]["fields"]["original_path"], REDACTED);
        assert_eq!(
            events[2]["fields"]["path"], "/Users/ana/debug",
            "debug is local diagnosis"
        );
    }

    #[test]
    fn the_ring_keeps_the_latest_events_and_the_level_filters() {
        let events = record(Level::INFO, 2, || {
            for i in 0..5_u64 {
                tracing::info!(i, "tick");
            }
            tracing::debug!("not recorded at info");
        });
        let kept: Vec<&Value> = events.iter().map(|e| &e["fields"]["i"]).collect();
        assert_eq!(kept, [&Value::from("3"), &Value::from("4")]);
    }

    #[test]
    fn the_log_file_receives_redacted_lines() {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let dir = fx.path("logs").unwrap_or_else(|e| unreachable!("{e}"));
        {
            let config = Config {
                log_dir: Some(&dir),
                level: Level::INFO,
                ring: 10,
            };
            let (json, _ring, guard) = layer(&config).unwrap_or_else(|e| unreachable!("{e}"));
            let subscriber = tracing_subscriber::registry().with(json);
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(name = "Secret.app", "found");
            });
            drop(guard);
        }
        let text: String = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| unreachable!("{e}"))
            .filter_map(Result::ok)
            .map(|entry| std::fs::read_to_string(entry.path()).unwrap_or_default())
            .collect();
        assert!(text.contains(REDACTED), "{text}");
        assert!(!text.contains("Secret.app"), "{text}");
    }

    #[test]
    fn only_the_latest_daily_logs_are_kept() {
        // Twenty old daily logs (dates long past, so today's file is new) and
        // one file that is not a log.
        let mut nodes: Vec<Node> = (1..=20_u8)
            .map(|day| Node::File {
                path: format!("logs/lumen.log.2020-01-{day:02}"),
                contents: b"old\n".to_vec(),
            })
            .collect();
        nodes.push(Node::File {
            path: "logs/notes.txt".to_owned(),
            contents: b"keep me".to_vec(),
        });
        let fx = Fixture::build(&nodes).unwrap_or_else(|e| unreachable!("{e}"));
        let dir = fx.path("logs").unwrap_or_else(|e| unreachable!("{e}"));
        {
            let config = Config {
                log_dir: Some(&dir),
                level: Level::INFO,
                ring: 10,
            };
            // `tracing-appender` prunes when the appender is built (and again
            // at each daily rollover), before opening today's file: it removes
            // the oldest logs until `KEEP_DAILY_LOGS - 1` remain, then today's
            // file makes `KEEP_DAILY_LOGS`. No rollover is needed to see it.
            let (json, _ring, guard) = layer(&config).unwrap_or_else(|e| unreachable!("{e}"));
            let subscriber = tracing_subscriber::registry().with(json);
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(files = 1_u64, "today");
            });
            drop(guard);
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| unreachable!("{e}"))
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        let logs: Vec<&String> = names
            .iter()
            .filter(|n| n.starts_with("lumen.log."))
            .collect();
        assert_eq!(logs.len(), KEEP_DAILY_LOGS, "{names:?}");
        let today: String = logs
            .iter()
            .filter(|n| !n.starts_with("lumen.log.2020-"))
            .map(|n| std::fs::read_to_string(dir.join(n)).unwrap_or_default())
            .collect();
        assert!(
            today.contains("\"today\""),
            "today's file has the event: {today}"
        );
        let other = std::fs::read(dir.join("notes.txt")).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(other, b"keep me", "other files are untouched");
    }

    #[test]
    fn sensitive_names_are_recognised() {
        for name in ["path", "root", "file_name", "content_hash", "original"] {
            assert!(is_sensitive(name), "{name}");
        }
        for name in ["files", "bytes", "verdict", "op_id"] {
            assert!(!is_sensitive(name), "{name}");
        }
    }
}
