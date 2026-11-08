//! The installed software list (ADR-0060 step 3): every inventory the platform
//! keeps of installed applications and packages, and of the background items
//! that may belong to them (step 2), read on request. Nothing is
//! changed, nothing is run except the read-only queries the sources document
//! (RPM's `rpm -qa`), and nothing leaves the device from here; AI review's
//! notes on the list are [`crate::software_notes`] (ADR-0061).

use lumen_application::CancelToken;
use lumen_application::ports::InventoryReport;

/// Every software inventory of this platform, each with how completely it was
/// read. A source that is not installed reports itself as unavailable rather
/// than as empty.
#[must_use]
pub fn inventory(cancel: &CancelToken) -> Vec<InventoryReport> {
    collect(cancel)
}

#[cfg(target_os = "macos")]
fn collect(cancel: &CancelToken) -> Vec<InventoryReport> {
    use lumen_application::ports::InventorySource as _;
    use lumen_platform_macos::{
        AppInventory, HomebrewInventory, LaunchdInventory, ReceiptInventory,
    };
    vec![
        AppInventory::new().collect(cancel),
        ReceiptInventory::new().collect(cancel),
        HomebrewInventory::new().collect(cancel),
        LaunchdInventory::new().collect(cancel),
    ]
}

#[cfg(windows)]
fn collect(cancel: &CancelToken) -> Vec<InventoryReport> {
    use lumen_application::ports::InventorySource as _;
    use lumen_platform_windows::msix::MsixInventory;
    use lumen_platform_windows::{InstalledApps, Services, StartupEntries};
    vec![
        InstalledApps.collect(cancel),
        MsixInventory.collect(cancel),
        Services.collect(cancel),
        StartupEntries.collect(cancel),
    ]
}

#[cfg(target_os = "linux")]
fn collect(cancel: &CancelToken) -> Vec<InventoryReport> {
    use lumen_application::ports::InventorySource as _;
    use lumen_platform_linux::packages::{DpkgInventory, PacmanInventory};
    use lumen_platform_linux::rpm::RpmInventory;
    use lumen_platform_linux::{FlatpakInventory, SnapInventory};
    vec![
        DpkgInventory.collect(cancel),
        RpmInventory.collect(cancel),
        PacmanInventory.collect(cancel),
        FlatpakInventory::new().collect(cancel),
        SnapInventory::new().collect(cancel),
    ]
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn collect(_cancel: &CancelToken) -> Vec<InventoryReport> {
    Vec::new()
}
