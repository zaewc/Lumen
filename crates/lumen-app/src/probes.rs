//! Platform probes shared by assessment and cleanup.

use lumen_application::assess::Probes;
use lumen_application::ports::{InUseProbe, NoRunningProbe, OwnershipProbe, RunningProbe};
use lumen_domain::{AppId, RawPath, SourceName};

use crate::assess::AssessError;

/// A probe for platforms without one: every answer is unknown, so nothing is
/// ever eligible there.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
struct NoInUseProbe;

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
impl InUseProbe for NoInUseProbe {
    fn probe(
        &self,
        _path: &RawPath,
        _expected: &lumen_domain::FileIdentity,
    ) -> lumen_application::ports::InUse {
        lumen_application::ports::InUse::Unknown {
            reason: "no in-use probe on this platform",
        }
    }
}

/// Where each installed application is, by identifier: what the running
/// probe matches processes against (ADR-0059).
pub(crate) type AppLocations = [(AppId, RawPath)];

/// The platform's probes and their source names, with no running probe: for
/// work that decides nothing (restoring and removing held items).
///
/// # Errors
///
/// [`AssessError::SourceName`] if a probe's name is invalid (never in
/// practice: the names are constants).
pub fn with_probes<T>(run: impl FnOnce(&Probes<'_>) -> T) -> Result<T, AssessError> {
    with(false, &[], run)
}

/// [`with_probes`] for one assessment's candidates. On macOS the process table
/// is read once and every candidate is answered from it, by the same rules
/// (#766): asking the kernel again per candidate made assessing grow faster
/// than the number of candidates. The answers are as of the start of the
/// assessment, which the evidence already says; moving and removing use
/// [`with_decision_probes`] and ask live, item by item.
pub(crate) fn with_assessment_probes<T>(
    apps: &AppLocations,
    run: impl FnOnce(&Probes<'_>) -> T,
) -> Result<T, AssessError> {
    with(true, apps, run)
}

/// [`with_probes`] for deciding items again now (confirming a plan,
/// reassessing): in-use is asked live, and whether the owning applications
/// run is read from the process table as it is now.
pub(crate) fn with_decision_probes<T>(
    apps: &AppLocations,
    run: impl FnOnce(&Probes<'_>) -> T,
) -> Result<T, AssessError> {
    with(false, apps, run)
}

/// Whether an application runs, from one read of the process table: running
/// when an executable inside any of its bundles runs, not running only when
/// every bundle is known not to (ADR-0059).
#[cfg(target_os = "macos")]
struct AppsRunning<'a> {
    processes: lumen_platform_macos::ProcessSnapshot,
    apps: &'a AppLocations,
}

#[cfg(target_os = "macos")]
impl RunningProbe for AppsRunning<'_> {
    fn running(&self, app: &AppId) -> Option<bool> {
        let mut answers = self
            .apps
            .iter()
            .filter(|(id, _)| id == app)
            .map(|(_, bundle)| self.processes.running(bundle))
            .peekable();
        answers.peek()?;
        let mut all_stopped = true;
        for answer in answers {
            match answer {
                Some(true) => return Some(true),
                Some(false) => {}
                None => all_stopped = false,
            }
        }
        all_stopped.then_some(false)
    }
}

fn with<T>(
    snapshot: bool,
    apps: &AppLocations,
    run: impl FnOnce(&Probes<'_>) -> T,
) -> Result<T, AssessError> {
    #[cfg(target_os = "macos")]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) = (
        if snapshot {
            Box::new(lumen_platform_macos::ProcessProbe.snapshot())
        } else {
            Box::new(lumen_platform_macos::ProcessProbe)
        },
        "macos.process-probe",
    );
    // Other platforms' probes are not measured yet; they ask live.
    #[cfg(not(target_os = "macos"))]
    let _ = snapshot;
    #[cfg(windows)]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) = (
        Box::new(lumen_platform_windows::RestartManagerProbe),
        "windows.restart-manager",
    );
    #[cfg(target_os = "linux")]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) =
        (Box::new(lumen_scan::ProcProbe), "linux.proc");
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) = (Box::new(NoInUseProbe), "none");
    #[cfg(unix)]
    let (ownership, ownership_name): (Box<dyn OwnershipProbe>, &str) =
        (Box::new(lumen_scan::UnixOwnershipProbe), "unix.owner");
    #[cfg(windows)]
    let (ownership, ownership_name): (Box<dyn OwnershipProbe>, &str) = (
        Box::new(lumen_platform_windows::HandleEnumerator),
        "windows.owner-sid",
    );
    // Only where there are applications to look for, and only on macOS so far.
    #[cfg(target_os = "macos")]
    let running: Box<dyn RunningProbe + '_> = if apps.is_empty() {
        Box::new(NoRunningProbe)
    } else {
        Box::new(AppsRunning {
            processes: lumen_platform_macos::ProcessProbe.snapshot(),
            apps,
        })
    };
    #[cfg(not(target_os = "macos"))]
    let running: Box<dyn RunningProbe + '_> = {
        let _ = apps;
        Box::new(NoRunningProbe)
    };
    let in_use_source = SourceName::new(in_use_name)?;
    let ownership_source = SourceName::new(ownership_name)?;
    let running_source = SourceName::new("process-table.applications")?;
    Ok(run(&Probes {
        in_use: in_use.as_ref(),
        in_use_source: &in_use_source,
        ownership: ownership.as_ref(),
        ownership_source: &ownership_source,
        running: running.as_ref(),
        running_source: &running_source,
    }))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use lumen_domain::{AppIdScheme, UntrustedText};

    use super::*;

    fn app(id: &str) -> AppId {
        AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new(id),
        }
    }

    #[test]
    fn an_app_runs_when_its_bundle_does_and_is_unknown_without_a_location()
    -> Result<(), Box<dyn std::error::Error>> {
        let exe = std::fs::canonicalize(std::env::current_exe()?)?;
        let folder = crate::raw_path(exe.parent().ok_or("the test has a folder")?)?;
        let apps = [(app("com.example.this-test"), folder)];
        let running = AppsRunning {
            processes: lumen_platform_macos::ProcessProbe.snapshot(),
            apps: &apps,
        };
        assert_eq!(running.running(&app("com.example.this-test")), Some(true));
        assert_eq!(running.running(&app("com.example.elsewhere")), None);
        Ok(())
    }
}
