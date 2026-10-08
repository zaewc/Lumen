//! What the running platform can observe and do.
//!
//! Lumen must never offer an action the platform cannot perform, nor claim to see
//! what it cannot see (ADR-0008, `docs/architecture/platforms.md`). Each platform
//! has a fixed *ceiling*; the runtime capabilities (which depend on permissions the
//! user granted, such as Full Disk Access or Android storage tiers) are always a
//! subset of that ceiling, enforced at construction.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Operating system Lumen is running on.
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
pub enum Platform {
    /// macOS.
    Macos,
    /// Windows.
    Windows,
    /// Linux; used for development and CI, not a product target.
    Linux,
    /// Android.
    Android,
    /// iOS and iPadOS.
    Ios,
}

impl Platform {
    /// All platforms.
    pub const ALL: [Self; 5] = [
        Self::Macos,
        Self::Windows,
        Self::Linux,
        Self::Android,
        Self::Ios,
    ];
}

/// Something Lumen can observe.
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
pub enum Observation {
    /// Walk the user's filesystem (all of it on desktop; shared storage on Android).
    UserFilesystem,
    /// Folders the user explicitly picked (iOS document picker, Android SAF).
    PickedFolders,
    /// Read other applications' files (caches, support data).
    OtherAppsFiles,
    /// Read other applications' storage *sizes* without their files
    /// (Android `StorageStatsManager`).
    OtherAppsSizes,
    /// Enumerate installed applications and packages.
    Applications,
    /// Enumerate services, launch agents, startup items and scheduled tasks.
    Services,
    /// Query the platform media library (`MediaStore`, `PhotoKit`).
    MediaLibrary,
    /// Detect cloud-backed items and their local/downloaded state.
    CloudItems,
}

/// A way Lumen can free space.
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
pub enum CleanupMechanism {
    /// Same-volume rename into Lumen's quarantine store (ADR-0015).
    QuarantineRename,
    /// Android `MediaStore.createTrashRequest`.
    MediaTrash,
    /// iOS `PhotoKit` delete into Recently Deleted.
    PhotoLibraryDelete,
    /// Evict or dehydrate a cloud-backed item; the content stays in the cloud.
    CloudEvict,
    /// Run a developer tool's own cleanup command (ADR-0021).
    ToolCommand,
    /// Android `ACTION_CLEAR_APP_CACHE`: clears all apps' external caches at once.
    SystemCacheClear,
    /// Archive an unused Android app (`PackageInstaller.requestArchive`).
    ArchiveApp,
    /// Send the user to the system's own settings screen; Lumen changes nothing.
    OpenSystemSettings,
}

/// Whether an action can be undone.
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
pub enum Reversibility {
    /// Lumen (or the platform) can restore the item exactly.
    Reversible,
    /// Restorable only under conditions outside Lumen's control (an installer
    /// still present, network access, a system retention window).
    Conditional,
    /// Cannot be undone.
    Irreversible,
    /// Lumen takes no action itself.
    NotApplicable,
}

impl CleanupMechanism {
    /// How reversible this mechanism is.
    pub const fn reversibility(self) -> Reversibility {
        match self {
            Self::QuarantineRename | Self::CloudEvict => Reversibility::Reversible,
            Self::MediaTrash | Self::PhotoLibraryDelete | Self::ArchiveApp => {
                Reversibility::Conditional
            }
            Self::ToolCommand | Self::SystemCacheClear => Reversibility::Irreversible,
            Self::OpenSystemSettings => Reversibility::NotApplicable,
        }
    }
}

/// Error returned when capabilities exceed what the platform allows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CapabilityError {
    /// An observation the platform cannot provide.
    #[error("{platform:?} cannot provide observation {observation:?}")]
    UnsupportedObservation {
        /// Platform.
        platform: Platform,
        /// Requested observation.
        observation: Observation,
    },
    /// A mechanism the platform cannot perform.
    #[error("{platform:?} cannot perform cleanup mechanism {mechanism:?}")]
    UnsupportedMechanism {
        /// Platform.
        platform: Platform,
        /// Requested mechanism.
        mechanism: CleanupMechanism,
    },
}

/// What the running host can observe and do. Always within the platform ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct PlatformCapabilities {
    platform: Platform,
    observations: BTreeSet<Observation>,
    mechanisms: BTreeSet<CleanupMechanism>,
}

impl PlatformCapabilities {
    /// Builds runtime capabilities, rejecting anything beyond the platform ceiling.
    ///
    /// # Errors
    ///
    /// Returns a [`CapabilityError`] naming the first unsupported observation or
    /// mechanism.
    pub fn new(
        platform: Platform,
        observations: impl IntoIterator<Item = Observation>,
        mechanisms: impl IntoIterator<Item = CleanupMechanism>,
    ) -> Result<Self, CapabilityError> {
        let ceiling = Self::ceiling(platform);
        let observations: BTreeSet<_> = observations.into_iter().collect();
        let mechanisms: BTreeSet<_> = mechanisms.into_iter().collect();
        if let Some(&observation) = observations.difference(&ceiling.observations).next() {
            return Err(CapabilityError::UnsupportedObservation {
                platform,
                observation,
            });
        }
        if let Some(&mechanism) = mechanisms.difference(&ceiling.mechanisms).next() {
            return Err(CapabilityError::UnsupportedMechanism {
                platform,
                mechanism,
            });
        }
        Ok(Self {
            platform,
            observations,
            mechanisms,
        })
    }

    /// The most a platform can ever do, with every permission granted, in the
    /// current product scope (system-scope writes are excluded in v1, ADR-0005).
    pub fn ceiling(platform: Platform) -> Self {
        use CleanupMechanism as M;
        use Observation as O;
        let (observations, mechanisms): (&[Observation], &[CleanupMechanism]) = match platform {
            Platform::Macos | Platform::Windows => (
                &[
                    O::UserFilesystem,
                    O::OtherAppsFiles,
                    O::Applications,
                    O::Services,
                    O::CloudItems,
                ],
                &[
                    M::QuarantineRename,
                    M::CloudEvict,
                    M::ToolCommand,
                    M::OpenSystemSettings,
                ],
            ),
            Platform::Linux => (&[O::UserFilesystem], &[M::QuarantineRename, M::ToolCommand]),
            Platform::Android => (
                &[
                    O::UserFilesystem,
                    O::PickedFolders,
                    O::OtherAppsSizes,
                    O::Applications,
                    O::MediaLibrary,
                ],
                &[
                    M::QuarantineRename,
                    M::MediaTrash,
                    M::SystemCacheClear,
                    M::ArchiveApp,
                    M::OpenSystemSettings,
                ],
            ),
            Platform::Ios => (
                &[O::PickedFolders, O::MediaLibrary, O::CloudItems],
                &[M::PhotoLibraryDelete, M::CloudEvict, M::OpenSystemSettings],
            ),
        };
        Self {
            platform,
            observations: observations.iter().copied().collect(),
            mechanisms: mechanisms.iter().copied().collect(),
        }
    }

    /// The platform.
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Whether the host can make this observation.
    pub fn observes(&self, observation: Observation) -> bool {
        self.observations.contains(&observation)
    }

    /// Whether the host can perform this cleanup mechanism.
    pub fn supports(&self, mechanism: CleanupMechanism) -> bool {
        self.mechanisms.contains(&mechanism)
    }

    /// Supported observations.
    pub fn observations(&self) -> impl Iterator<Item = Observation> + '_ {
        self.observations.iter().copied()
    }

    /// Supported mechanisms.
    pub fn mechanisms(&self) -> impl Iterator<Item = CleanupMechanism> + '_ {
        self.mechanisms.iter().copied()
    }
}

impl<'de> Deserialize<'de> for PlatformCapabilities {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            platform: Platform,
            observations: BTreeSet<Observation>,
            mechanisms: BTreeSet<CleanupMechanism>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.platform, wire.observations, wire.mechanisms)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const ALL_MECHANISMS: [CleanupMechanism; 8] = [
        CleanupMechanism::QuarantineRename,
        CleanupMechanism::MediaTrash,
        CleanupMechanism::PhotoLibraryDelete,
        CleanupMechanism::CloudEvict,
        CleanupMechanism::ToolCommand,
        CleanupMechanism::SystemCacheClear,
        CleanupMechanism::ArchiveApp,
        CleanupMechanism::OpenSystemSettings,
    ];

    #[test]
    fn ios_never_quarantines_or_runs_tools_or_reads_other_apps() {
        let ios = PlatformCapabilities::ceiling(Platform::Ios);
        assert!(!ios.supports(CleanupMechanism::QuarantineRename));
        assert!(!ios.supports(CleanupMechanism::ToolCommand));
        assert!(!ios.observes(Observation::OtherAppsFiles));
        assert!(!ios.observes(Observation::OtherAppsSizes));
        assert!(!ios.observes(Observation::UserFilesystem));
    }

    #[test]
    fn android_sees_other_apps_sizes_but_not_files() {
        let android = PlatformCapabilities::ceiling(Platform::Android);
        assert!(android.observes(Observation::OtherAppsSizes));
        assert!(!android.observes(Observation::OtherAppsFiles));
        assert!(!android.supports(CleanupMechanism::ToolCommand));
    }

    #[test]
    fn construction_rejects_capabilities_beyond_the_ceiling() {
        assert_eq!(
            PlatformCapabilities::new(Platform::Ios, [], [CleanupMechanism::QuarantineRename]),
            Err(CapabilityError::UnsupportedMechanism {
                platform: Platform::Ios,
                mechanism: CleanupMechanism::QuarantineRename,
            })
        );
        assert!(matches!(
            PlatformCapabilities::new(Platform::Android, [Observation::OtherAppsFiles], []),
            Err(CapabilityError::UnsupportedObservation { .. })
        ));
    }

    #[test]
    fn deserialization_enforces_the_ceiling() {
        let claim = r#"{"platform":"ios","observations":[],"mechanisms":["tool_command"]}"#;
        assert!(serde_json::from_str::<PlatformCapabilities>(claim).is_err());
        let ok = r#"{"platform":"ios","observations":["media_library"],"mechanisms":["photo_library_delete"]}"#;
        assert!(serde_json::from_str::<PlatformCapabilities>(ok).is_ok());
    }

    #[test]
    fn irreversible_mechanisms_are_exactly_tool_commands_and_system_cache_clear() {
        let irreversible: Vec<_> = ALL_MECHANISMS
            .into_iter()
            .filter(|m| m.reversibility() == Reversibility::Irreversible)
            .collect();
        assert_eq!(
            irreversible,
            [
                CleanupMechanism::ToolCommand,
                CleanupMechanism::SystemCacheClear
            ]
        );
    }

    proptest! {
        #[test]
        fn any_subset_of_the_ceiling_is_accepted_and_nothing_more(
            platform_index in 0usize..5,
            mask in any::<u8>(),
        ) {
            let platform = Platform::ALL[platform_index];
            let ceiling = PlatformCapabilities::ceiling(platform);
            let requested: Vec<_> = ALL_MECHANISMS
                .into_iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, m)| m)
                .collect();
            let result = PlatformCapabilities::new(platform, [], requested.clone());
            let within = requested.iter().all(|m| ceiling.supports(*m));
            prop_assert_eq!(result.is_ok(), within);
            if let Ok(caps) = result {
                for m in ALL_MECHANISMS {
                    prop_assert_eq!(caps.supports(m), requested.contains(&m));
                }
            }
        }

        #[test]
        fn ceilings_round_trip_through_json(platform_index in 0usize..5) {
            let caps = PlatformCapabilities::ceiling(Platform::ALL[platform_index]);
            let json = serde_json::to_string(&caps).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let back: PlatformCapabilities = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(back, caps);
        }
    }
}
