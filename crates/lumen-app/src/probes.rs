//! Platform probes shared by assessment and cleanup.

use lumen_application::assess::Probes;
use lumen_application::ports::{InUseProbe, OwnershipProbe};
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
use lumen_domain::RawPath;
use lumen_domain::SourceName;

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

/// The platform's probes and their source names.
///
/// # Errors
///
/// [`AssessError::SourceName`] if a probe's name is invalid (never in
/// practice: the names are constants).
pub fn with_probes<T>(run: impl FnOnce(&Probes<'_>) -> T) -> Result<T, AssessError> {
    with(false, run)
}

/// [`with_probes`] for one assessment's candidates. On macOS the process table
/// is read once and every candidate is answered from it, by the same rules
/// (#766): asking the kernel again per candidate made assessing grow faster
/// than the number of candidates. The answers are as of the start of the
/// assessment, which the evidence already says; moving and removing use
/// [`with_probes`] and ask live, item by item.
pub(crate) fn with_assessment_probes<T>(
    run: impl FnOnce(&Probes<'_>) -> T,
) -> Result<T, AssessError> {
    with(true, run)
}

fn with<T>(snapshot: bool, run: impl FnOnce(&Probes<'_>) -> T) -> Result<T, AssessError> {
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
    let in_use_source = SourceName::new(in_use_name)?;
    let ownership_source = SourceName::new(ownership_name)?;
    Ok(run(&Probes {
        in_use: in_use.as_ref(),
        in_use_source: &in_use_source,
        ownership: ownership.as_ref(),
        ownership_source: &ownership_source,
    }))
}
