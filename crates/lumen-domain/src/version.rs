//! Version types recorded with every policy decision and AI judgment.
//!
//! A decision is reproducible only if we know exactly which policy, schemas and
//! prompt produced it (ADR-0011, ADR-0014, ADR-0020). These types give those
//! versions a strict, comparable text form:
//!
//! - [`PolicyVersion`]: semantic version, e.g. `1.4.0`;
//! - [`SchemaVersion`]: `name/major.minor`, e.g. `jev.request/1.2`;
//! - [`PromptVersion`]: `prompt/revision`, e.g. `jev-judge/3`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Error returned when a version string is malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VersionError {
    /// The text does not have the expected shape.
    #[error("{kind} '{text}' is malformed: expected {expected}")]
    Malformed {
        /// Name of the version type.
        kind: &'static str,
        /// The rejected text, truncated to 64 bytes.
        text: String,
        /// Description of the expected form.
        expected: &'static str,
    },
}

fn malformed(kind: &'static str, text: &str, expected: &'static str) -> VersionError {
    let mut end = text.len().min(64);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    VersionError::Malformed {
        kind,
        text: text[..end].to_owned(),
        expected,
    }
}

/// Parses a canonical decimal number: digits only, no sign, no leading zeros.
fn parse_number(text: &str) -> Option<u32> {
    let canonical = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if canonical { text.parse().ok() } else { None }
}

/// Validates a schema or prompt name: 1–64 bytes of `[a-z0-9.-]`, starting with a
/// letter, not ending with `.` or `-`.
fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() <= 64
        && first.is_ascii_lowercase()
        && !matches!(last, b'.' | b'-')
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
}

macro_rules! string_serde {
    ($name:ident) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Version of the deterministic safety policy (rules and rule data).
///
/// Ordered by `(major, minor, patch)`. A change that can alter any verdict bumps at
/// least the minor version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyVersion {
    /// Incompatible change to rule semantics or decision outputs.
    pub major: u32,
    /// Change that can alter verdicts for some inputs.
    pub minor: u32,
    /// Change that cannot alter any verdict (e.g. explanation wording).
    pub patch: u32,
}

impl PolicyVersion {
    /// Creates a policy version.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for PolicyVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for PolicyVersion {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const EXPECTED: &str = "MAJOR.MINOR.PATCH with canonical decimal numbers";
        let mut parts = s.split('.');
        let (Some(major), Some(minor), Some(patch), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(malformed("PolicyVersion", s, EXPECTED));
        };
        match (
            parse_number(major),
            parse_number(minor),
            parse_number(patch),
        ) {
            (Some(major), Some(minor), Some(patch)) => Ok(Self {
                major,
                minor,
                patch,
            }),
            _ => Err(malformed("PolicyVersion", s, EXPECTED)),
        }
    }
}

string_serde!(PolicyVersion);

/// Version of a persisted or transmitted schema, e.g. `jev.request/1.2`.
///
/// Additive changes bump `minor`; anything else bumps `major` (ADR-0011).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    name: String,
    major: u32,
    minor: u32,
}

impl SchemaVersion {
    /// Creates a schema version.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError::Malformed`] if `name` is not 1–64 bytes of
    /// `[a-z0-9.-]` starting with a letter, or if `major` is zero.
    pub fn new(name: impl Into<String>, major: u32, minor: u32) -> Result<Self, VersionError> {
        let name = name.into();
        if !is_valid_name(&name) || major == 0 {
            return Err(malformed("SchemaVersion", &name, SCHEMA_EXPECTED));
        }
        Ok(Self { name, major, minor })
    }

    /// Schema name, e.g. `jev.request`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Major version (at least 1).
    pub const fn major(&self) -> u32 {
        self.major
    }

    /// Minor version.
    pub const fn minor(&self) -> u32 {
        self.minor
    }

    /// Whether a reader that understands `reader` can read data written as `self`:
    /// same schema name and same major version. Minor versions only add optional
    /// fields, so any minor is readable within a major.
    pub fn is_readable_by(&self, reader: &SchemaVersion) -> bool {
        self.name == reader.name && self.major == reader.major
    }
}

const SCHEMA_EXPECTED: &str = "NAME/MAJOR or NAME/MAJOR.MINOR, NAME of [a-z0-9.-], MAJOR >= 1";

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}.{}", self.name, self.major, self.minor)
    }
}

impl FromStr for SchemaVersion {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || malformed("SchemaVersion", s, SCHEMA_EXPECTED);
        let (name, number) = s.split_once('/').ok_or_else(err)?;
        let (major, minor) = match number.split_once('.') {
            Some((major, minor)) => (parse_number(major), parse_number(minor)),
            None => (parse_number(number), Some(0)),
        };
        match (major, minor) {
            (Some(major), Some(minor)) => Self::new(name, major, minor).map_err(|_| err()),
            _ => Err(err()),
        }
    }
}

string_serde!(SchemaVersion);

/// Version of an AI prompt, e.g. `jev-judge/3`. Prompts are immutable once
/// released; any change is a new revision (docs/ai/prompting.md).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PromptVersion {
    prompt: String,
    revision: u32,
}

const PROMPT_EXPECTED: &str = "PROMPT/REVISION, PROMPT of [a-z0-9.-], REVISION >= 1";

impl PromptVersion {
    /// Creates a prompt version.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError::Malformed`] if `prompt` is not a valid name or
    /// `revision` is zero.
    pub fn new(prompt: impl Into<String>, revision: u32) -> Result<Self, VersionError> {
        let prompt = prompt.into();
        if !is_valid_name(&prompt) || revision == 0 {
            return Err(malformed("PromptVersion", &prompt, PROMPT_EXPECTED));
        }
        Ok(Self { prompt, revision })
    }

    /// Prompt identifier, e.g. `jev-judge`.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Revision number (at least 1).
    pub const fn revision(&self) -> u32 {
        self.revision
    }
}

impl fmt::Display for PromptVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.prompt, self.revision)
    }
}

impl FromStr for PromptVersion {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || malformed("PromptVersion", s, PROMPT_EXPECTED);
        let (prompt, revision) = s.split_once('/').ok_or_else(err)?;
        let revision = parse_number(revision).ok_or_else(err)?;
        Self::new(prompt, revision).map_err(|_| err())
    }
}

string_serde!(PromptVersion);

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn policy_version_parses_canonical_semver_only() -> Result<(), VersionError> {
        assert_eq!(
            "1.4.0".parse::<PolicyVersion>()?,
            PolicyVersion::new(1, 4, 0)
        );
        assert_eq!(
            "0.0.0".parse::<PolicyVersion>()?,
            PolicyVersion::new(0, 0, 0)
        );
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.2.-3",
            "+1.2.3",
            "1.2.3-rc1",
            " 1.2.3",
            "v1.2.3",
        ] {
            assert!(bad.parse::<PolicyVersion>().is_err(), "{bad:?}");
        }
        Ok(())
    }

    #[test]
    fn policy_versions_order_numerically() {
        assert!(PolicyVersion::new(1, 10, 0) > PolicyVersion::new(1, 9, 9));
        assert!(PolicyVersion::new(2, 0, 0) > PolicyVersion::new(1, 99, 99));
    }

    #[test]
    fn schema_version_text_forms() -> Result<(), VersionError> {
        let v: SchemaVersion = "jev.request/1".parse()?;
        assert_eq!((v.name(), v.major(), v.minor()), ("jev.request", 1, 0));
        assert_eq!(v.to_string(), "jev.request/1.0");
        assert_eq!(
            "quarantine.manifest/2.3".parse::<SchemaVersion>()?.minor(),
            3
        );
        for bad in [
            "jev.request",
            "/1",
            "jev.request/0",
            "Jev.request/1",
            "jev request/1",
            "jev.request/1.",
            "jev./1",
            "jev.request/01",
            "jev.request/1/2",
        ] {
            assert!(bad.parse::<SchemaVersion>().is_err(), "{bad:?}");
        }
        Ok(())
    }

    #[test]
    fn schema_compatibility_is_same_name_and_major() -> Result<(), VersionError> {
        let reader: SchemaVersion = "jev.judgment/1.0".parse()?;
        assert!(
            "jev.judgment/1.7"
                .parse::<SchemaVersion>()?
                .is_readable_by(&reader)
        );
        assert!(
            !"jev.judgment/2.0"
                .parse::<SchemaVersion>()?
                .is_readable_by(&reader)
        );
        assert!(
            !"jev.request/1.0"
                .parse::<SchemaVersion>()?
                .is_readable_by(&reader)
        );
        Ok(())
    }

    #[test]
    fn prompt_version_text_forms() -> Result<(), VersionError> {
        let v: PromptVersion = "jev-judge/3".parse()?;
        assert_eq!((v.prompt(), v.revision()), ("jev-judge", 3));
        for bad in [
            "jev-judge",
            "jev-judge/0",
            "jev-judge/1.0",
            "JEV/1",
            "/3",
            "jev-/3",
        ] {
            assert!(bad.parse::<PromptVersion>().is_err(), "{bad:?}");
        }
        Ok(())
    }

    #[test]
    fn versions_serialize_as_strings() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            serde_json::to_string(&PolicyVersion::new(1, 2, 3))?,
            "\"1.2.3\""
        );
        assert_eq!(
            serde_json::to_string(&SchemaVersion::new("jev.trace", 1, 0)?)?,
            "\"jev.trace/1.0\""
        );
        assert!(serde_json::from_str::<PromptVersion>("\"jev-judge/0\"").is_err());
        Ok(())
    }

    #[test]
    fn malformed_error_truncates_long_input_on_char_boundary() {
        let long = "é".repeat(100);
        let result = long.parse::<PolicyVersion>();
        assert!(
            matches!(&result, Err(VersionError::Malformed { text, .. }) if text.len() <= 64 && !text.is_empty()),
            "{result:?}"
        );
    }

    fn name() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9.-]{0,30}[a-z0-9]"
    }

    proptest! {
        #[test]
        fn policy_version_round_trips(major: u32, minor: u32, patch: u32) {
            let v = PolicyVersion::new(major, minor, patch);
            prop_assert_eq!(v.to_string().parse::<PolicyVersion>(), Ok(v));
        }

        #[test]
        fn schema_version_round_trips(name in name(), major in 1u32.., minor: u32) {
            let v = SchemaVersion::new(name, major, minor).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(v.to_string().parse::<SchemaVersion>(), Ok(v));
        }

        #[test]
        fn prompt_version_round_trips(prompt in name(), revision in 1u32..) {
            let v = PromptVersion::new(prompt, revision).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(v.to_string().parse::<PromptVersion>(), Ok(v));
        }

        #[test]
        fn parsers_never_panic(s in "\\PC{0,80}") {
            let _policy = s.parse::<PolicyVersion>();
            let _schema = s.parse::<SchemaVersion>();
            let _prompt = s.parse::<PromptVersion>();
        }
    }
}
