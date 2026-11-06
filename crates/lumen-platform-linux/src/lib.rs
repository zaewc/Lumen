//! Lumen's Linux platform adapter. Directories are listed by
//! `lumen_scan::StdFsEnumerator`; this crate adds what that portable enumerator
//! must not do: reading file contents, for duplicate analysis of folders the
//! user chose ([`DuplicateReader`], ADR-0019).

#[cfg(target_os = "linux")]
mod duplicates;

#[cfg(target_os = "linux")]
pub use duplicates::DuplicateReader;
