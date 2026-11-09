//! What Jev sees and says about installed software (ADR-0061):
//! the request `jev.software-request/1` and the note `jev.software-note/2`
//! (docs/ai/model-contract.md).
//!
//! A request describes one installed application or package with identity
//! metadata only: its origin, identifier, name, publisher and version, whether
//! the platform marks it protected or hidden, and the background items linked
//! to it by evidence. It never carries a path, a user or account name, a usage
//! signal, a size, or anything read from a file. Every attacker-controllable
//! string appears only as an [`UntrustedText`] in `untrusted_strings` or as a
//! service `label`, so a prompt can label it as data.
//!
//! A note is display only: it is never an input to the policy, a plan or the
//! executor, and it never says that software is safe to remove.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{JudgmentConfidence, PayloadHash, SchemaVersion, ServiceKind, UntrustedText};

/// Domain separator for software request hashes.
const SOFTWARE_REQUEST_HASH_DOMAIN: &[u8] = b"lumen.jev.software-request/1\0";

/// Schema name of software requests.
pub const JEV_SOFTWARE_REQUEST_SCHEMA: &str = "jev.software-request";

/// Where installed software came from: its installer or package manager.
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
pub enum SoftwareOrigin {
    /// A macOS application bundle.
    MacApp,
    /// A macOS installer package receipt.
    MacInstaller,
    /// A Homebrew formula.
    Homebrew,
    /// A Homebrew cask.
    HomebrewCask,
    /// A Windows program with an uninstall entry.
    WindowsProgram,
    /// A Windows MSIX or Store package.
    Msix,
    /// A Debian package.
    Dpkg,
    /// An RPM package.
    Rpm,
    /// A pacman package.
    Pacman,
    /// A Flatpak application.
    Flatpak,
    /// A snap.
    Snap,
    /// Another inventory.
    Other,
}

/// The labelled untrusted strings of a software request.
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
pub enum SoftwareField {
    /// The native identifier (bundle identifier, package name, product code).
    Id,
    /// The display name.
    Name,
    /// The publisher or vendor.
    Publisher,
    /// The version string.
    Version,
}

/// A background item linked to the software by evidence (#807).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoftwareService {
    /// What kind of background item it is.
    pub kind: ServiceKind,
    /// Its label (launchd label, service name, task name); untrusted.
    pub label: UntrustedText,
    /// Whether the link is verified; `false` for a heuristic or ambiguous one.
    pub verified: bool,
}

impl SoftwareService {
    /// A linked background item from plain values.
    pub fn new(kind: ServiceKind, label: &str, verified: bool) -> Self {
        Self {
            kind,
            label: UntrustedText::new(label),
            verified,
        }
    }
}

/// The plain values a software request is built from. There is no field for a
/// path, a user name, a usage signal or a size, by design (ADR-0061 rule 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwareFacts<'a> {
    /// Its installer or package manager.
    pub origin: SoftwareOrigin,
    /// Its native identifier.
    pub id: &'a str,
    /// Its display name, if known.
    pub name: Option<&'a str>,
    /// Its publisher, if known.
    pub publisher: Option<&'a str>,
    /// Its version, if known.
    pub version: Option<&'a str>,
    /// Whether the platform marks it protected.
    pub protected: bool,
    /// Whether the platform hides it from its own software list.
    pub hidden: bool,
    /// The background items linked to it.
    pub services: Vec<SoftwareService>,
}

/// Everything Jev may see about one installed application or package
/// (`jev.software-request/1`). Built only through [`SoftwareRequest::new`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoftwareRequest {
    /// `jev.software-request/1.x`.
    schema_version: SchemaVersion,
    /// Its installer or package manager.
    origin: SoftwareOrigin,
    /// Whether the platform marks it protected.
    protected: bool,
    /// Whether the platform hides it from its own software list.
    hidden: bool,
    /// Linked background items, verified first, at most
    /// [`SoftwareRequest::MAX_SERVICES`].
    services: Vec<SoftwareService>,
    /// Whether more background items are linked than are listed.
    services_truncated: bool,
    /// Attacker-controllable strings, labelled; empty values are left out.
    untrusted_strings: BTreeMap<SoftwareField, UntrustedText>,
}

impl SoftwareRequest {
    /// The most background items a request lists.
    pub const MAX_SERVICES: usize = 8;

    /// The schema version this code writes.
    ///
    /// # Errors
    ///
    /// Never in practice: the name and version are constants that are valid.
    pub fn current_schema() -> Result<SchemaVersion, crate::VersionError> {
        SchemaVersion::new(JEV_SOFTWARE_REQUEST_SCHEMA, 1, 0)
    }

    /// The request for `facts`. Verified background items are listed before
    /// heuristic ones, and only the first [`Self::MAX_SERVICES`] are kept.
    ///
    /// # Errors
    ///
    /// Never in practice: see [`Self::current_schema`].
    pub fn new(facts: SoftwareFacts<'_>) -> Result<Self, crate::VersionError> {
        let mut services = facts.services;
        services.sort_by_key(|s| !s.verified);
        let services_truncated = services.len() > Self::MAX_SERVICES;
        services.truncate(Self::MAX_SERVICES);
        let untrusted_strings = [
            (SoftwareField::Id, Some(facts.id)),
            (SoftwareField::Name, facts.name),
            (SoftwareField::Publisher, facts.publisher),
            (SoftwareField::Version, facts.version),
        ]
        .into_iter()
        .filter_map(|(field, value)| {
            value
                .filter(|v| !v.is_empty())
                .map(|v| (field, UntrustedText::new(v)))
        })
        .collect();
        Ok(Self {
            schema_version: Self::current_schema()?,
            origin: facts.origin,
            protected: facts.protected,
            hidden: facts.hidden,
            services,
            services_truncated,
            untrusted_strings,
        })
    }

    /// Its schema version.
    pub const fn schema_version(&self) -> &SchemaVersion {
        &self.schema_version
    }

    /// Its origin.
    pub const fn origin(&self) -> SoftwareOrigin {
        self.origin
    }

    /// Whether the platform marks the software protected.
    pub const fn protected(&self) -> bool {
        self.protected
    }

    /// The listed background items.
    pub fn services(&self) -> &[SoftwareService] {
        &self.services
    }

    /// The labelled untrusted strings.
    pub const fn untrusted_strings(&self) -> &BTreeMap<SoftwareField, UntrustedText> {
        &self.untrusted_strings
    }

    /// Domain-separated BLAKE3 of the request's canonical JSON, for traces,
    /// cassettes and the on-device answer cache.
    pub fn hash(&self) -> PayloadHash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SOFTWARE_REQUEST_HASH_DOMAIN);
        hasher.update(
            &serde_json::to_vec(self)
                .unwrap_or_else(|e| unreachable!("request serialization is infallible: {e}")),
        );
        PayloadHash::from_bytes(*hasher.finalize().as_bytes())
    }
}

/// What an AI note says installed software is mainly for (ADR-0061). A closed
/// list: only these values are ever shown.
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
pub enum SoftwarePurpose {
    /// A web browser.
    WebBrowser,
    /// Messaging, email or calls.
    Communication,
    /// Music, video or photos.
    Media,
    /// Documents, notes or other office work.
    OfficeOrProductivity,
    /// A developer tool.
    DevelopmentTool,
    /// Design or other creative work.
    DesignOrCreative,
    /// A game or game launcher.
    Game,
    /// Security, banking or identity.
    SecurityOrBanking,
    /// Part of the system, a driver or hardware support.
    SystemOrDriver,
    /// Cloud storage, sync or backup.
    CloudSyncOrBackup,
    /// A utility for managing the computer.
    Utility,
    /// A runtime or library other software uses.
    RuntimeOrLibrary,
    /// The model could not tell.
    Unknown,
}

/// What an AI note says removing installed software could affect besides the
/// software itself (ADR-0061). A description, never a statement that removal
/// is safe.
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
pub enum RemovalImpact {
    /// Mainly its own features.
    OnlyThisApp,
    /// Other software may depend on it.
    OtherSoftwareDepends,
    /// Its background items would stop.
    StopsBackgroundActivity,
    /// Security, banking or sign-in may stop working.
    SecurityOrAccess,
    /// An operating-system feature may stop working.
    SystemFunction,
    /// The model could not tell.
    Unknown,
}

/// An AI note about installed software (`jev.software-note/2`, ADR-0061):
/// display only. It is never an input to the policy, a plan or the executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoftwareNote {
    /// What it is most likely for.
    pub purpose: SoftwarePurpose,
    /// What removing it could most likely affect.
    pub removal_impact: RemovalImpact,
    /// How sure the model was of `purpose`, bucketed for a question with that
    /// many options (`lumen_jev::software`).
    pub purpose_confidence: JudgmentConfidence,
    /// How sure the model was of `removal_impact`, bucketed the same way.
    /// Each answer keeps its own confidence: a sure purpose is not made to
    /// look unsure by an unsure impact, nor the other way round.
    pub removal_impact_confidence: JudgmentConfidence,
    /// Whether the model saw text trying to influence it; such a note is
    /// recorded but never shown.
    pub injection_suspected: bool,
}

impl SoftwareNote {
    /// Whether the note may be shown: only when no injection was suspected.
    pub const fn shown(&self) -> bool {
        !self.injection_suspected
    }

    /// The note with a removal impact that contradicts what is known set to
    /// [`RemovalImpact::Unknown`] with low confidence, like the judgment
    /// guards (ADR-0043): "only this app" is never said of software the
    /// platform marks protected, nor of software whose own purpose means other
    /// things rely on it (security or banking, a system component or driver, a
    /// runtime or library). The model's answer can never make removal look
    /// harmless there.
    ///
    /// Roadmap 16.8 hook: once Lumen knows what depends on a package, that
    /// fact, not the model's guess, decides the impact; it belongs here.
    #[must_use]
    pub const fn guarded(self, protected: bool) -> Self {
        let relied_on = matches!(
            self.purpose,
            SoftwarePurpose::SecurityOrBanking
                | SoftwarePurpose::SystemOrDriver
                | SoftwarePurpose::RuntimeOrLibrary
        );
        if matches!(self.removal_impact, RemovalImpact::OnlyThisApp) && (protected || relied_on) {
            Self {
                removal_impact: RemovalImpact::Unknown,
                removal_impact_confidence: JudgmentConfidence::Low,
                ..self
            }
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(services: Vec<SoftwareService>) -> SoftwareFacts<'static> {
        SoftwareFacts {
            origin: SoftwareOrigin::HomebrewCask,
            id: "firefox",
            name: Some("/Users/alice/Applications/Firefox.app"),
            publisher: Some("Mozilla"),
            version: Some(""),
            protected: false,
            hidden: true,
            services,
        }
    }

    #[test]
    fn requests_carry_identity_metadata_only() -> Result<(), Box<dyn std::error::Error>> {
        let request = SoftwareRequest::new(facts(vec![SoftwareService::new(
            ServiceKind::LaunchAgent,
            "org.mozilla.updater",
            true,
        )]))?;
        let json = serde_json::to_value(&request)?;
        assert_eq!(json["schema_version"], "jev.software-request/1.0");
        assert_eq!(json["origin"], "homebrew_cask");
        assert_eq!(json["hidden"], true);
        assert_eq!(json["services"][0]["kind"], "launch_agent");
        assert_eq!(json["services"][0]["label"]["text"], "org.mozilla.updater");
        let keys: Vec<&String> = json
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        assert_eq!(
            keys,
            [
                "hidden",
                "origin",
                "protected",
                "schema_version",
                "services",
                "services_truncated",
                "untrusted_strings"
            ]
        );
        let strings: Vec<&String> = json["untrusted_strings"]
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        assert_eq!(
            strings,
            ["id", "name", "publisher"],
            "empty values left out"
        );
        // A path-like name travels only as labelled untrusted text.
        let text = json.to_string();
        assert_eq!(text.matches("/Users/alice").count(), 1);
        assert_eq!(
            json["untrusted_strings"]["name"]["text"],
            "/Users/alice/Applications/Firefox.app"
        );
        assert!(!text.contains("\"path"), "no field is named path: {text}");
        Ok(())
    }

    #[test]
    fn services_are_capped_with_verified_ones_first() -> Result<(), Box<dyn std::error::Error>> {
        let services = (0..10)
            .map(|i| SoftwareService::new(ServiceKind::ScheduledTask, &format!("t{i}"), i == 9))
            .collect();
        let request = SoftwareRequest::new(facts(services))?;
        assert_eq!(request.services().len(), SoftwareRequest::MAX_SERVICES);
        assert_eq!(request.services()[0].label.raw(), "t9");
        assert_eq!(serde_json::to_value(&request)?["services_truncated"], true);
        Ok(())
    }

    #[test]
    fn requests_conform_to_their_schema_and_hash_stably() -> Result<(), Box<dyn std::error::Error>>
    {
        let request = SoftwareRequest::new(facts(Vec::new()))?;
        let schema = serde_json::to_value(schemars::schema_for!(SoftwareRequest))?;
        let validator = jsonschema::validator_for(&schema)?;
        let errors: Vec<String> = validator
            .iter_errors(&serde_json::to_value(&request)?)
            .map(|e| e.to_string())
            .collect();
        assert_eq!(errors, Vec::<String>::new());
        assert_eq!(
            request.hash(),
            SoftwareRequest::new(facts(Vec::new()))?.hash()
        );
        let mut other = facts(Vec::new());
        other.version = Some("2.0");
        assert_ne!(request.hash(), SoftwareRequest::new(other)?.hash());
        Ok(())
    }

    #[test]
    fn only_this_app_is_never_kept_where_others_rely_on_the_software() {
        let note = |purpose| SoftwareNote {
            purpose,
            removal_impact: RemovalImpact::OnlyThisApp,
            purpose_confidence: JudgmentConfidence::High,
            removal_impact_confidence: JudgmentConfidence::High,
            injection_suspected: false,
        };
        let media = note(SoftwarePurpose::Media);
        assert_eq!(
            media.guarded(false),
            media,
            "an ordinary app keeps its answer"
        );
        assert_eq!(
            media.guarded(true),
            SoftwareNote {
                removal_impact: RemovalImpact::Unknown,
                removal_impact_confidence: JudgmentConfidence::Low,
                ..media
            },
            "protected: unknown, with low confidence; the purpose is kept"
        );
        for purpose in [
            SoftwarePurpose::SecurityOrBanking,
            SoftwarePurpose::SystemOrDriver,
            SoftwarePurpose::RuntimeOrLibrary,
        ] {
            assert_eq!(
                note(purpose).guarded(false).removal_impact,
                RemovalImpact::Unknown
            );
        }
        let other = SoftwareNote {
            removal_impact: RemovalImpact::SystemFunction,
            ..note(SoftwarePurpose::SystemOrDriver)
        };
        assert_eq!(other.guarded(true), other, "other answers are kept");
    }
}
