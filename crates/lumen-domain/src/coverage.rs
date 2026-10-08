//! What a scan could and could not observe.
//!
//! A directory the scanner could not read is **unknown**, never empty, and an
//! inventory source that was unavailable or truncated cannot prove that something
//! is absent (ADR-0008). Blind spots must never turn into "orphan" findings or
//! zero-size totals, so every observation carries an [`AccessState`] and every scan
//! produces a [`CoverageReport`].

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::RawPath;
use crate::name::is_valid_name;

/// Why a location could not be read.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DenialReason {
    /// macOS privacy protection (TCC), e.g. Full Disk Access not granted.
    Tcc,
    /// macOS System Integrity Protection or the sealed system volume.
    Sip,
    /// POSIX permissions.
    Posix,
    /// Windows ACL or other access-control list.
    Acl,
    /// Outside the scope the platform grants the app (sandbox, Android storage tier).
    Scope,
    /// Denied for a reason the platform did not report.
    Unknown,
}

/// Why reading a location failed for a reason other than access control.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FailureKind {
    /// The location disappeared while being scanned.
    Vanished,
    /// Reading did not finish within its time budget.
    TimedOut,
    /// Any other I/O failure.
    Io,
}

/// Whether a location's contents were observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
pub enum AccessState {
    /// Fully read.
    Readable,
    /// Access was refused.
    Denied(DenialReason),
    /// Reading failed.
    Failed(FailureKind),
    /// Deliberately not visited (excluded, cancelled, not yet reached).
    NotScanned,
}

impl AccessState {
    /// Whether the contents were observed, so sizes and absence are meaningful.
    /// Every other state means "unknown".
    pub const fn is_observed(self) -> bool {
        matches!(self, Self::Readable)
    }
}

/// A named inventory source, e.g. `macos.launchd-plists` or
/// `android.package-manager`: 1–64 bytes of `[a-z0-9.-]`, starting with a letter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceName(String);

/// Error returned for an invalid [`SourceName`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("source name must be 1-64 bytes of [a-z0-9.-] starting with a letter")]
pub struct SourceNameError;

impl SourceName {
    /// Validates and wraps a source name.
    ///
    /// # Errors
    ///
    /// Returns [`SourceNameError`] if the name is invalid.
    pub fn new(name: impl Into<String>) -> Result<Self, SourceNameError> {
        let name = name.into();
        if is_valid_name(&name) {
            Ok(Self(name))
        } else {
            Err(SourceNameError)
        }
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SourceName {
    type Err = SourceNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for SourceName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SourceName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(serde::de::Error::custom)
    }
}

/// How completely an inventory source was observed.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    /// Every item the source exposes was read.
    Complete,
    /// Some items could not be read.
    Partial,
    /// The result looks implausibly small (e.g. an Android package list without
    /// the `android` package), so it must not be trusted for absence.
    SuspectedTruncated,
    /// The source could not be queried at all (missing permission, not supported).
    Unavailable,
}

/// Coverage of one scan root.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RootCoverage {
    /// Directories fully read.
    pub readable: u64,
    /// Directories refused, by reason.
    pub denied: BTreeMap<DenialReason, u64>,
    /// Directories whose read failed, by kind.
    pub failed: BTreeMap<FailureKind, u64>,
    /// Directories deliberately not visited.
    pub not_scanned: u64,
}

impl RootCoverage {
    /// Counts one directory observation.
    pub fn record(&mut self, state: AccessState) {
        match state {
            AccessState::Readable => self.readable = self.readable.saturating_add(1),
            AccessState::Denied(reason) => bump(self.denied.entry(reason).or_default()),
            AccessState::Failed(kind) => bump(self.failed.entry(kind).or_default()),
            AccessState::NotScanned => self.not_scanned = self.not_scanned.saturating_add(1),
        }
    }

    /// Whether every directory under the root was read.
    pub fn is_complete(&self) -> bool {
        self.not_scanned == 0
            && self.denied.values().all(|&n| n == 0)
            && self.failed.values().all(|&n| n == 0)
    }
}

fn bump(counter: &mut u64) {
    *counter = counter.saturating_add(1);
}

/// Coverage of one scan root, together with the root it describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RootEntry {
    /// The scan root.
    pub root: RawPath,
    /// What was observed under it.
    pub coverage: RootCoverage,
}

/// What one scan observed: per-root directory coverage and per-source status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CoverageReport {
    /// Coverage per scan root. A list rather than a map because paths are not
    /// valid JSON object keys.
    pub roots: Vec<RootEntry>,
    /// Status per inventory source.
    pub sources: BTreeMap<SourceName, SourceStatus>,
}

impl CoverageReport {
    /// Whether every root and every source was observed completely.
    pub fn is_complete(&self) -> bool {
        self.roots.iter().all(|entry| entry.coverage.is_complete())
            && self.sources.values().all(|s| *s == SourceStatus::Complete)
    }

    /// Whether "not found" in `required` sources may be treated as evidence of
    /// absence: each required source must have been queried and be
    /// [`SourceStatus::Complete`]. A source missing from the report counts as
    /// unobserved. An empty `required` set proves nothing.
    pub fn absence_is_provable(&self, required: &BTreeSet<SourceName>) -> bool {
        !required.is_empty()
            && required
                .iter()
                .all(|name| self.sources.get(name) == Some(&SourceStatus::Complete))
    }
}

manual_schema!(SourceName, "SourceName", { "type": "string", "pattern": concat!("^", name_regex!(), "$") });

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn source(name: &str) -> SourceName {
        SourceName::new(name).unwrap_or_else(|_| unreachable!("valid test name"))
    }

    #[test]
    fn only_readable_is_observed() {
        assert!(AccessState::Readable.is_observed());
        assert!(!AccessState::Denied(DenialReason::Tcc).is_observed());
        assert!(!AccessState::Failed(FailureKind::Vanished).is_observed());
        assert!(!AccessState::NotScanned.is_observed());
    }

    #[test]
    fn access_state_json_shape() -> serde_json::Result<()> {
        assert_eq!(
            serde_json::to_string(&AccessState::Readable)?,
            r#"{"state":"readable"}"#
        );
        assert_eq!(
            serde_json::to_string(&AccessState::Denied(DenialReason::Sip))?,
            r#"{"state":"denied","reason":"sip"}"#
        );
        Ok(())
    }

    #[test]
    fn root_coverage_counts_and_completeness() {
        let mut root = RootCoverage::default();
        root.record(AccessState::Readable);
        root.record(AccessState::Readable);
        assert!(root.is_complete());
        root.record(AccessState::Denied(DenialReason::Tcc));
        root.record(AccessState::Denied(DenialReason::Tcc));
        assert!(!root.is_complete());
        assert_eq!(root.readable, 2);
        assert_eq!(root.denied.get(&DenialReason::Tcc), Some(&2));
    }

    #[test]
    fn absence_requires_every_required_source_complete() {
        let mut report = CoverageReport::default();
        report
            .sources
            .insert(source("macos.applications"), SourceStatus::Complete);
        report
            .sources
            .insert(source("macos.spotlight"), SourceStatus::Partial);

        let apps_only = BTreeSet::from([source("macos.applications")]);
        let with_spotlight =
            BTreeSet::from([source("macos.applications"), source("macos.spotlight")]);
        let with_missing = BTreeSet::from([source("macos.applications"), source("macos.receipts")]);

        assert!(report.absence_is_provable(&apps_only));
        assert!(
            !report.absence_is_provable(&with_spotlight),
            "partial source"
        );
        assert!(
            !report.absence_is_provable(&with_missing),
            "unqueried source"
        );
        assert!(
            !report.absence_is_provable(&BTreeSet::new()),
            "nothing required proves nothing"
        );
    }

    #[test]
    fn truncated_source_blocks_absence_and_completeness() {
        let mut report = CoverageReport::default();
        report.sources.insert(
            source("android.package-manager"),
            SourceStatus::SuspectedTruncated,
        );
        assert!(!report.absence_is_provable(&BTreeSet::from([source("android.package-manager")])));
        assert!(!report.is_complete());
    }

    #[test]
    fn full_report_round_trips_through_json() -> Result<(), Box<dyn std::error::Error>> {
        let mut coverage = RootCoverage::default();
        coverage.record(AccessState::Readable);
        coverage.record(AccessState::Denied(DenialReason::Tcc));
        coverage.record(AccessState::Failed(FailureKind::TimedOut));
        let mut report = CoverageReport::default();
        report.roots.push(RootEntry {
            root: RawPath::from_unix_bytes(b"/Users/ana".to_vec())?,
            coverage,
        });
        report
            .sources
            .insert(source("macos.launchd-plists"), SourceStatus::Partial);
        let json = serde_json::to_string(&report)?;
        assert_eq!(serde_json::from_str::<CoverageReport>(&json)?, report);
        Ok(())
    }

    #[test]
    fn source_name_validation() {
        assert!(SourceName::new("windows.uninstall-registry").is_ok());
        for bad in [
            "",
            "Windows.registry",
            "9lives",
            "a b",
            "trailing.",
            "x".repeat(65).as_str(),
        ] {
            assert!(SourceName::new(bad).is_err(), "{bad:?}");
        }
        assert!(serde_json::from_str::<SourceName>("\"Bad Name\"").is_err());
    }

    fn arb_state() -> impl Strategy<Value = AccessState> {
        prop_oneof![
            Just(AccessState::Readable),
            Just(AccessState::NotScanned),
            Just(AccessState::Denied(DenialReason::Tcc)),
            Just(AccessState::Denied(DenialReason::Acl)),
            Just(AccessState::Failed(FailureKind::Io)),
        ]
    }

    proptest! {
        #[test]
        fn root_is_complete_iff_every_record_was_readable(states in prop::collection::vec(arb_state(), 0..50)) {
            let mut root = RootCoverage::default();
            for s in &states {
                root.record(*s);
            }
            prop_assert_eq!(root.is_complete(), states.iter().all(|s| s.is_observed()));
        }

        #[test]
        fn absence_never_provable_with_a_non_complete_required_source(
            statuses in prop::collection::vec(
                prop_oneof![
                    Just(SourceStatus::Complete),
                    Just(SourceStatus::Partial),
                    Just(SourceStatus::SuspectedTruncated),
                    Just(SourceStatus::Unavailable),
                ],
                1..6,
            )
        ) {
            let mut report = CoverageReport::default();
            let mut required = BTreeSet::new();
            for (i, status) in statuses.iter().enumerate() {
                let name = source(&format!("src{i}"));
                report.sources.insert(name.clone(), *status);
                required.insert(name);
            }
            let all_complete = statuses.iter().all(|s| *s == SourceStatus::Complete);
            prop_assert_eq!(report.absence_is_provable(&required), all_complete);
        }
    }
}
