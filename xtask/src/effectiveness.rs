//! `cargo xtask effectiveness [--projects <N>] [--seed <S>] [--out <FILE>]`:
//! cleanup effectiveness on a seeded labeled home (issue #703).
//!
//! Builds `lumen` in release mode, generates
//! `lumen_testkit::labeled::seeded_macos_home` in a temporary directory, and runs
//! `lumen scan` twice (first, then with the file cache warm) and `lumen assess
//! --json` with `HOME` set to it. It reports timings, the peak resident memory of
//! those runs, and the score against the labels, as JSON (to `--out`, or stdout)
//! and as a short summary on stderr. It fails if any recommendation is unsafe.
//!
//! The first scan is only as cold as the operating system's cache allows; the
//! tool never purges it. Nothing here touches a user directory.

use std::error::Error;
use std::ffi::OsStr;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lumen_testkit::labeled::{Candidate, Expect, LabeledHome, Score, score, seeded_macos_home};
use lumen_testkit::{Fixture, Node};
use serde_json::{Value, json};

/// The command's options.
struct Options<'a> {
    projects: usize,
    seed: u64,
    out: Option<&'a str>,
}

fn options<'a>(args: &[&'a str]) -> Result<Options<'a>, Box<dyn Error>> {
    let mut options = Options {
        projects: 200,
        seed: 1,
        out: None,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = || rest.next().copied().ok_or(format!("{arg} needs a value"));
        match *arg {
            "--projects" => options.projects = value()?.parse()?,
            "--seed" => options.seed = value()?.parse()?,
            "--out" => options.out = Some(value()?),
            other => return Err(format!("unknown option: {other}").into()),
        }
    }
    Ok(options)
}

/// One `lumen` run: how long it took, its peak resident memory in bytes (where
/// the platform reports it), and its stdout.
struct Step {
    elapsed: Duration,
    peak_rss: Option<u64>,
    stdout: Vec<u8>,
}

/// What the measured runs produced, in the order they ran.
struct Measured {
    first_scan: Step,
    second_scan: Step,
    assess: Step,
    candidates: Vec<Candidate>,
}

/// Runs `lumen` with `HOME` set to `home`.
fn timed(lumen: &Path, home: &Path, args: &[&OsStr]) -> Result<Step, Box<dyn Error>> {
    let started = Instant::now();
    let mut child = Command::new(lumen)
        .args(args)
        .env("HOME", home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let errors = std::thread::spawn(move || {
        let mut text = Vec::new();
        stderr.read_to_end(&mut text).map(|_| text)
    });
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .ok_or("no stdout")?
        .read_to_end(&mut stdout)?;
    let (success, peak_rss) = reap(&mut child)?;
    let elapsed = started.elapsed();
    let errors = errors.join().map_err(|_| "reading stderr failed")??;
    if !success {
        return Err(format!(
            "lumen {args:?} failed: {}",
            String::from_utf8_lossy(&errors)
        )
        .into());
    }
    Ok(Step {
        elapsed,
        peak_rss,
        stdout,
    })
}

fn measure(lumen: &Path, home: &Path, db: &Path) -> Result<Measured, Box<dyn Error>> {
    let scan = [
        "scan".as_ref(),
        home.as_os_str(),
        "--db".as_ref(),
        db.as_os_str(),
    ];
    let first_scan = timed(lumen, home, &scan)?;
    let second_scan = timed(lumen, home, &scan)?;
    let assess = timed(
        lumen,
        home,
        &[
            "assess".as_ref(),
            "--db".as_ref(),
            db.as_os_str(),
            "--json".as_ref(),
        ],
    )?;
    let report: Value = serde_json::from_slice(&assess.stdout)?;
    let candidates = report["candidates"]
        .as_array()
        .ok_or("assess printed no candidates")?
        .iter()
        .map(|c| Candidate {
            path: c["path"].as_str().unwrap_or_default().to_owned(),
            verdict: c["verdict"].as_str().unwrap_or_default().to_owned(),
            reclaimable_bytes: c["reclaimable_bytes"].as_u64().unwrap_or_default(),
        })
        .collect();
    Ok(Measured {
        first_scan,
        second_scan,
        assess,
        candidates,
    })
}

fn release_lumen() -> Result<PathBuf, Box<dyn Error>> {
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--locked", "-p", "lumen-cli"])
        .status()?;
    if !status.success() {
        return Err("building lumen failed".into());
    }
    Ok(Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/release/lumen"))
}

fn report(options: &Options<'_>, labeled: &LabeledHome, run: &Measured, result: &Score) -> Value {
    let count = |expect| labeled.labels.iter().filter(|l| l.expect == expect).count();
    let files = labeled
        .nodes
        .iter()
        .filter(|node| matches!(node, Node::File { .. }))
        .count();
    json!({
        "environment": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "filesystem": "the system temporary directory",
            "build": "release",
        },
        "fixture": { "seed": options.seed, "projects": options.projects, "files": files },
        "timings_ms": {
            "first_scan": run.first_scan.elapsed.as_millis(),
            "second_scan": run.second_scan.elapsed.as_millis(),
            "assess": run.assess.elapsed.as_millis(),
        },
        "peak_rss_bytes": {
            "first_scan": run.first_scan.peak_rss,
            "second_scan": run.second_scan.peak_rss,
            "assess": run.assess.peak_rss,
        },
        "score": {
            "unsafe_recommendations": result.unsafe_recommendations,
            "proposed_found": result.proposed_found,
            "proposed_total": count(Expect::Proposed),
            "proposed_missed": result.proposed_missed,
            "review_found": result.review_found,
            "review_total": count(Expect::Review),
            "review_missed": result.review_missed,
            "proposed_bytes": result.proposed_bytes,
        },
    })
}

pub(crate) fn effectiveness(args: &[&str]) -> Result<usize, Box<dyn Error>> {
    let options = options(args)?;
    if !cfg!(target_os = "macos") {
        return Err("the labeled home describes macOS paths; run this on macOS".into());
    }
    let lumen = release_lumen()?;
    let labeled = seeded_macos_home(options.seed, options.projects);
    let fx = Fixture::build(&labeled.nodes)?;
    let home = std::fs::canonicalize(fx.path("home")?)?;
    // Held open for the whole run, as an editor or build would.
    let _open = labeled
        .labels
        .iter()
        .filter(|label| label.expect == Expect::InUse)
        .map(|label| std::fs::File::open(home.join(&label.path)))
        .collect::<Result<Vec<_>, _>>()?;
    let run = measure(&lumen, &home, &fx.path("index.db")?)?;
    let home_text = home.to_str().ok_or("the temporary home is not UTF-8")?;
    let result = score(home_text, &labeled.labels, &run.candidates);
    let summary = report(&options, &labeled, &run, &result);
    let text = serde_json::to_string_pretty(&summary)? + "\n";
    match options.out {
        Some(path) => std::fs::write(path, &text)?,
        None => print!("{text}"),
    }
    eprintln!(
        "{} files: scan {} ms, then {} ms warm; assess {} ms; peak memory {} (scan) and {} (assess) bytes",
        summary["fixture"]["files"],
        summary["timings_ms"]["first_scan"],
        summary["timings_ms"]["second_scan"],
        summary["timings_ms"]["assess"],
        summary["peak_rss_bytes"]["first_scan"],
        summary["peak_rss_bytes"]["assess"],
    );
    eprintln!(
        "proposed {}/{} artifacts ({} bytes); review {}/{}; unsafe recommendations: {}",
        result.proposed_found,
        summary["score"]["proposed_total"],
        result.proposed_bytes,
        result.review_found,
        summary["score"]["review_total"],
        result.unsafe_recommendations.len(),
    );
    Ok(result.unsafe_recommendations.len())
}

/// Waits for `child` and returns whether it succeeded and its own peak resident
/// set in bytes. `wait4` reports that child alone, unlike
/// `getrusage(RUSAGE_CHILDREN)`, which would include the release build.
#[cfg(unix)]
#[allow(unsafe_code)]
fn reap(child: &mut std::process::Child) -> Result<(bool, Option<u64>), Box<dyn Error>> {
    let pid = libc::pid_t::try_from(child.id())?;
    let mut status: libc::c_int = 0;
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: invariant: `pid` is this process's unreaped child, and `status` and
    // `usage` point to writable memory of their types. Why unsafe: FFI call.
    // Why it holds: `child` was spawned here and nothing else waits for it (std
    // never waits for a dropped `Child`); both pointers are to locals.
    let reaped = unsafe { libc::wait4(pid, &raw mut status, 0, usage.as_mut_ptr()) };
    if reaped != pid {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: invariant: `wait4` filled `usage`. Why unsafe: `assume_init`. Why it
    // holds: it returned the child's pid, which means success.
    let max = unsafe { usage.assume_init() }.ru_maxrss;
    let success = libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0;
    // macOS reports bytes; Linux and the BSDs report KiB.
    let peak = u64::try_from(max).ok().map(|max| {
        if cfg!(target_os = "macos") {
            max
        } else {
            max * 1024
        }
    });
    Ok((success, peak))
}

#[cfg(not(unix))]
fn reap(child: &mut std::process::Child) -> Result<(bool, Option<u64>), Box<dyn Error>> {
    Ok((child.wait()?.success(), None))
}
