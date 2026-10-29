//! Running a developer tool's own cleanup command when the user asks
//! (ADR-0049).
//!
//! Only a knowledge-base entry's `official_action` runs: a program name and
//! literal arguments, never a string from the page, the user or the AI. The
//! program is found as the user's terminal would find it (the login shell's
//! `PATH`, then the usual install locations), and runs as the user without a
//! shell, with input closed, in the home folder, for at most
//! [`TIME_LIMIT`]. What it removes cannot be restored; the caller has
//! confirmed that with the user.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use lumen_devkb::{KnowledgeBase, SafetyClass};

/// The longest a command may run before it is stopped.
pub const TIME_LIMIT: Duration = Duration::from_mins(15);

/// The most output kept, from its end.
pub const OUTPUT_LIMIT: usize = 8 * 1024;

/// A knowledge-base command, ready to look up and run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCommand {
    /// The entry it comes from.
    pub entry: String,
    /// The tool's display name.
    pub tool: String,
    /// The program's bare name, such as `pnpm`.
    pub program: String,
    /// Its arguments, exactly as the knowledge base spells them.
    pub args: Vec<String>,
    /// The item cannot be regenerated, so the command removes data, not
    /// just caches (Docker's disk image).
    pub removes_data: bool,
}

/// The command of knowledge-base entry `entry`, if it has one.
pub fn command(kb: &KnowledgeBase, entry: &str) -> Option<ToolCommand> {
    let found = kb.get(entry)?;
    let action = found.official_action.as_ref()?;
    Some(ToolCommand {
        entry: found.id.clone(),
        tool: found.tool.clone(),
        program: action.program.clone(),
        args: action.args.clone(),
        removes_data: found.safety_class == SafetyClass::NotRegenerable,
    })
}

/// The command of entry `entry` in Lumen's built-in knowledge base.
pub fn builtin(entry: &str) -> Option<ToolCommand> {
    command(crate::assess::builtin_knowledge_base()?, entry)
}

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRun {
    /// It exited with status 0.
    pub success: bool,
    /// Its exit code, if it exited on its own.
    pub exit_code: Option<i32>,
    /// It ran past the time limit and was stopped.
    pub timed_out: bool,
    /// The end of what it printed (standard output and error, interleaved
    /// as they arrived), at most [`OUTPUT_LIMIT`] bytes.
    pub output: String,
    /// All matched volumes observed around the run, or unknown coverage.
    pub space_changes: Option<Vec<crate::space_change::SpaceChange>>,
}

/// Why a command did not run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolError {
    /// The tool is not installed where the user's terminal would find it.
    #[error("{0} is not installed, or Lumen cannot find it")]
    NotFound(String),
    /// The resolved directories cannot form a process search path.
    #[error("the tool's search path is invalid: {0}")]
    SearchPath(#[from] std::env::JoinPathsError),
    /// The home folder is unknown.
    #[error("the home folder is unknown")]
    NoHome,
    /// It could not be started or watched.
    #[error("{program} could not be run: {source}")]
    Spawn {
        /// The program, by path.
        program: String,
        /// The error.
        source: std::io::Error,
    },
}

/// Directories the user's terminal searches: the login shell's `PATH` on
/// Unix (an app started from the Dock or Finder has a much shorter one),
/// then this process's own `PATH`, then the usual install locations.
fn search_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(unix)]
    if let Some(path) = login_shell_path() {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    #[cfg(unix)]
    {
        dirs.extend(
            ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
                .into_iter()
                .map(PathBuf::from),
        );
        dirs.extend(
            [
                ".local/bin",
                ".cargo/bin",
                "Library/pnpm",
                ".local/share/pnpm",
                "go/bin",
            ]
            .into_iter()
            .map(|dir| home.join(dir)),
        );
    }
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("pnpm"));
    }
    let _ = home;
    dirs.retain(|dir| dir.is_absolute());
    dirs
}

/// The `PATH` the user's login shell sets up, from running it with a fixed
/// script and no input; `None` if it does not answer within 5 s.
#[cfg(unix)]
fn login_shell_path() -> Option<std::ffi::OsString> {
    use std::io::Read as _;
    use std::os::unix::ffi::OsStringExt as _;

    let shell = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|shell| shell.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"));
    let mut child = Command::new(shell)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if started.elapsed() > Duration::from_secs(5) => {
                let _killed = child.kill();
                let _reaped = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut bytes = Vec::new();
    child.stdout.take()?.read_to_end(&mut bytes).ok()?;
    Some(std::ffi::OsString::from_vec(bytes))
}

/// Whether `path` is a file this user may run.
fn runnable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// The program named `name` in the first of `dirs` that has it, by absolute
/// path. On Windows, with the executable extensions it may carry.
pub fn find_in(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        [".exe", ".cmd", ".bat"]
            .into_iter()
            .map(|ext| format!("{name}{ext}"))
            .collect()
    } else {
        vec![name.to_owned()]
    };
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|candidate| runnable(candidate))
}

/// The end of a stream, at most [`OUTPUT_LIMIT`] bytes.
type Tail = Arc<Mutex<VecDeque<u8>>>;

/// Copies `from` into `tail` until it ends, keeping only the last bytes.
fn drain(mut from: impl std::io::Read + Send + 'static, tail: Tail) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while let Ok(n) = from.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let mut kept = tail.lock().unwrap_or_else(PoisonError::into_inner);
            kept.extend(buffer.iter().take(n));
            let excess = kept.len().saturating_sub(OUTPUT_LIMIT);
            kept.drain(..excess);
        }
    })
}

/// Runs `command` (found with [`find_in`] in the user's search directories)
/// in `home`, stopping it after `limit`.
///
/// # Errors
///
/// [`ToolError::NotFound`] if the program is not installed, or
/// [`ToolError::Spawn`] if it cannot be started or watched.
pub fn run(command: &ToolCommand, home: &Path, limit: Duration) -> Result<ToolRun, ToolError> {
    run_with_dirs(command, home, limit, &search_dirs(home))
}

/// Run a confirmed command and observe its matched volumes. Read failures do
/// not fail cleanup; tool failures still report any observed space change.
///
/// # Errors
/// The same errors as [`run`].
pub fn run_measured(
    command: &ToolCommand,
    home: &Path,
    limit: Duration,
    targets: &[(lumen_domain::RawPath, lumen_domain::VolumeId)],
) -> Result<ToolRun, ToolError> {
    observe_run(targets, || run(command, home, limit))
}

fn observe_run(
    targets: &[(lumen_domain::RawPath, lumen_domain::VolumeId)],
    run: impl FnOnce() -> Result<ToolRun, ToolError>,
) -> Result<ToolRun, ToolError> {
    let watches = crate::space_change::watch_targets(targets);
    let mut result = run()?;
    result.space_changes = watches.and_then(|watches| {
        watches
            .into_iter()
            .map(crate::space_change::SpaceWatch::finish)
            .collect()
    });
    Ok(result)
}

fn run_with_dirs(
    command: &ToolCommand,
    home: &Path,
    limit: Duration,
    dirs: &[PathBuf],
) -> Result<ToolRun, ToolError> {
    let program = find_in(&command.program, dirs)
        .ok_or_else(|| ToolError::NotFound(command.program.clone()))?;
    let mut process = process(&program, &command.args, home);
    // A script's interpreter and child tools need the same PATH that found it.
    process.env("PATH", std::env::join_paths(dirs)?);
    let run = run_command(process, &program, limit)?;
    // The entry and the outcome only: never the output, which may name paths.
    tracing::info!(
        entry = %command.entry,
        success = run.success,
        timed_out = run.timed_out,
        "tool command finished"
    );
    Ok(run)
}

/// Stops `child` and, on Unix, every process in its group.
fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Some(group) = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _signalled = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    let _killed = child.kill();
    let _reaped = child.wait();
}

/// Runs `program` with `args` in `dir`, as [`run`] does.
///
/// # Errors
///
/// [`ToolError::Spawn`] if it cannot be started or watched.
pub fn run_program(
    program: &Path,
    args: &[String],
    dir: &Path,
    limit: Duration,
) -> Result<ToolRun, ToolError> {
    run_command(process(program, args, dir), program, limit)
}

fn process(program: &Path, args: &[String], dir: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Its own process group, so stopping it stops what it started too.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command
}

fn run_command(
    mut command: Command,
    program: &Path,
    limit: Duration,
) -> Result<ToolRun, ToolError> {
    let spawn_error = |source| ToolError::Spawn {
        program: program.display().to_string(),
        source,
    };
    let mut child = command.spawn().map_err(spawn_error)?;
    let tail: Tail = Arc::new(Mutex::new(VecDeque::new()));
    let readers: Vec<_> = [
        child.stdout.take().map(|out| drain(out, Arc::clone(&tail))),
        child.stderr.take().map(|err| drain(err, Arc::clone(&tail))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let started = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().map_err(spawn_error)? {
            break (Some(status), false);
        }
        if started.elapsed() > limit {
            stop(&mut child);
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    // Something it left running may hold its output open: wait a little for
    // the rest of the output, then report without it.
    let deadline = Instant::now() + Duration::from_secs(2);
    while readers.iter().any(|r| !r.is_finished()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    for reader in readers
        .into_iter()
        .filter(std::thread::JoinHandle::is_finished)
    {
        let _joined = reader.join();
    }
    let bytes: Vec<u8> = tail
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .copied()
        .collect();
    Ok(ToolRun {
        success: status.is_some_and(|s| s.success()),
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        output: String::from_utf8_lossy(&bytes).into_owned(),
        space_changes: None,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use lumen_testkit::{Fixture, Node};

    use super::*;

    /// A new executable script in the fixture (created, never overwritten).
    fn script(fx: &Fixture, name: &str, body: &str) -> PathBuf {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;

        let path = fx.path(name).unwrap_or_else(|e| unreachable!("{e}"));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&path)
            .unwrap_or_else(|e| unreachable!("{e}"));
        file.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
            .unwrap_or_else(|e| unreachable!("{e}"));
        path
    }

    fn fixture() -> Fixture {
        Fixture::build(&[Node::File {
            path: "bin/plain".into(),
            contents: b"x".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn programs_are_found_only_as_runnable_files_in_order() {
        let fx = fixture();
        let bin = fx.path("bin").unwrap_or_else(|e| unreachable!("{e}"));
        let first = script(&fx, "bin/tool", "true");
        let dirs = vec![PathBuf::from("/nonexistent"), bin.clone()];
        assert_eq!(find_in("tool", &dirs), Some(first));
        assert_eq!(find_in("missing", &dirs), None);
        assert_eq!(find_in("plain", &dirs), None, "not executable");
        assert_eq!(
            find_in("bin", &[fx.root().to_path_buf()]),
            None,
            "a directory"
        );
    }

    #[test]
    fn failed_commands_still_report_the_observed_volume() -> Result<(), Box<dyn std::error::Error>>
    {
        use lumen_application::ports::DirEnumerator as _;
        let fx = fixture();
        let program = script(&fx, "bin/tool", "exit 3");
        let path = crate::raw_path(&fx.path("bin/plain")?)?;
        let volume = crate::scan::enumerator()
            .stat(&path)?
            .identity()
            .volume
            .clone();
        let run = observe_run(&[(path, volume.clone())], || {
            run_program(&program, &[], fx.root(), Duration::from_secs(10))
        })?;
        assert!(!run.success);
        assert_eq!(run.exit_code, Some(3));
        let changes = run.space_changes.ok_or("measurement missing")?;
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].volume, volume);
        Ok(())
    }

    #[test]
    fn a_command_runs_without_a_shell_with_input_closed() -> Result<(), ToolError> {
        let fx = fixture();
        let program = script(
            &fx,
            "bin/tool",
            r#"printf 'args:%s|%s\n' "$1" "$2"; read line || echo "input closed"; echo oops >&2; exit 3"#,
        );
        let args = vec!["a b".to_owned(), "$HOME;rm".to_owned()];
        let run = run_program(&program, &args, fx.root(), Duration::from_secs(10))?;
        assert!(!run.success);
        assert_eq!(run.exit_code, Some(3));
        assert!(!run.timed_out);
        assert!(run.output.contains("args:a b|$HOME;rm"), "{}", run.output);
        assert!(run.output.contains("input closed"), "{}", run.output);
        assert!(run.output.contains("oops"), "{}", run.output);
        Ok(())
    }

    #[test]
    fn a_tool_and_its_children_use_the_resolved_search_path()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = fixture();
        script(&fx, "bin/helper", "printf 'helper available'");
        script(&fx, "bin/tool", "helper");
        let command = ToolCommand {
            entry: "fixture".into(),
            tool: "Fixture".into(),
            program: "tool".into(),
            args: Vec::new(),
            removes_data: false,
        };
        let run = run_with_dirs(
            &command,
            fx.root(),
            Duration::from_secs(10),
            &[fx.path("bin")?],
        )?;
        assert!(run.success, "{}", run.output);
        assert_eq!(run.output, "helper available");
        Ok(())
    }

    #[test]
    fn a_command_past_the_limit_is_stopped() -> Result<(), ToolError> {
        let fx = fixture();
        let program = script(&fx, "bin/slow", "echo started; sleep 30");
        let started = Instant::now();
        let run = run_program(&program, &[], fx.root(), Duration::from_secs(3))?;
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "its sleep was stopped too"
        );
        assert!(run.timed_out);
        assert!(!run.success);
        assert!(run.output.contains("started"));
        Ok(())
    }

    #[test]
    fn only_the_end_of_long_output_is_kept() -> Result<(), ToolError> {
        let fx = fixture();
        let program = script(
            &fx,
            "bin/loud",
            "i=0; while [ $i -lt 2000 ]; do echo line-$i; i=$((i+1)); done",
        );
        let run = run_program(&program, &[], fx.root(), Duration::from_secs(10))?;
        assert!(run.success);
        assert!(run.output.len() <= OUTPUT_LIMIT);
        assert!(run.output.contains("line-1999"));
        assert!(!run.output.contains("line-0\n"));
        Ok(())
    }

    #[test]
    fn knowledge_base_commands_carry_their_entry_and_data_warning() {
        let kb = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let docker = command(&kb, "docker-desktop-disk-image").unwrap_or_else(|| unreachable!());
        assert_eq!(docker.program, "docker");
        assert_eq!(
            docker.args,
            ["system", "prune", "--force"],
            "never waits for input"
        );
        assert!(docker.removes_data);
        let pnpm = command(&kb, "pnpm-store").unwrap_or_else(|| unreachable!());
        assert!(!pnpm.removes_data);
        assert_eq!(command(&kb, "no-such-entry"), None);
    }
}
