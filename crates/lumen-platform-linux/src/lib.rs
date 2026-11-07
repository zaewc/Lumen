//! Lumen's Linux platform adapter. Directories are listed by
//! `lumen_scan::StdFsEnumerator`; this crate adds what that portable enumerator
//! must not do: reading file contents, for duplicate analysis of folders the
//! user chose ([`DuplicateReader`], ADR-0019), and the installed packages that
//! dpkg and pacman record ([`packages`], #806), and Flatpak applications and
//! snaps (`sandboxed`, Linux only).

#[cfg(target_os = "linux")]
mod duplicates;
pub mod packages;
#[cfg(target_os = "linux")]
mod sandboxed;

#[cfg(target_os = "linux")]
pub use duplicates::DuplicateReader;
#[cfg(target_os = "linux")]
pub use sandboxed::{FlatpakInventory, SnapInventory, flatpak_app_id, snap_meta};
