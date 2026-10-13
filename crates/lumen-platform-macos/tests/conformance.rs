//! The `getattrlistbulk` enumerator against the adversarial conformance suite.

#![cfg(target_os = "macos")]

use lumen_platform_macos::BulkEnumerator;
use lumen_scan::conformance;

#[test]
fn link_loops() -> Result<(), conformance::Failure> {
    conformance::link_loops(&BulkEnumerator::new())
}

#[test]
fn hostile_names() -> Result<(), conformance::Failure> {
    conformance::hostile_names(&BulkEnumerator::new())
}

#[test]
fn flat_directory() -> Result<(), conformance::Failure> {
    conformance::flat_directory(&BulkEnumerator::new(), 20_000)
}

#[test]
fn deep_tree() -> Result<(), conformance::Failure> {
    conformance::deep_tree(&BulkEnumerator::new(), 200)
}

#[test]
fn special_files() -> Result<(), conformance::Failure> {
    conformance::special_files(&BulkEnumerator::new())
}
