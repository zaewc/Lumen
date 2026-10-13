//! The portable enumerator against the adversarial conformance suite.

#![cfg(unix)]

use lumen_scan::StdFsEnumerator;
use lumen_scan::conformance;

#[test]
fn link_loops() -> Result<(), conformance::Failure> {
    conformance::link_loops(&StdFsEnumerator)
}

#[test]
fn hostile_names() -> Result<(), conformance::Failure> {
    conformance::hostile_names(&StdFsEnumerator)
}

#[test]
fn flat_directory() -> Result<(), conformance::Failure> {
    conformance::flat_directory(&StdFsEnumerator, 20_000)
}

#[test]
fn deep_tree() -> Result<(), conformance::Failure> {
    conformance::deep_tree(&StdFsEnumerator, 200)
}

#[test]
fn special_files() -> Result<(), conformance::Failure> {
    conformance::special_files(&StdFsEnumerator)
}
