//! Lumen's Linux platform adapter. Directories are listed by
//! `lumen_scan::StdFsEnumerator`; this crate adds what that portable enumerator
//! must not do: reading file contents, for duplicate analysis of folders the
//! user chose ([`DuplicateReader`], ADR-0019), and the installed packages that
//! dpkg and pacman record ([`packages`], #806), RPM reports ([`rpm`]), and
//! Flatpak applications and snaps (`sandboxed`, Linux only), with what each
//! package's record says it needs (`relations`, ADR-0064).

#[cfg(target_os = "linux")]
mod duplicates;
pub mod packages;
mod relations;
pub mod rpm;
#[cfg(target_os = "linux")]
mod sandboxed;

#[cfg(target_os = "linux")]
pub use duplicates::DuplicateReader;
#[cfg(target_os = "linux")]
pub use sandboxed::{FlatpakInventory, SnapInventory, flatpak_app_id, snap_meta};
