//! Protected locations (roadmap 7.3, ADR-0014 stage 1).
//!
//! `data/protected-locations.json` lists locations whose contents Lumen must
//! never touch, each with the official source that documents it: operating-system
//! locations (`system`), credentials, keys and browser profiles
//! (`sensitive`), and cloud providers' sync folders (`cloud`), where moving an
//! item out deletes it in the cloud and on every device. An item at or below
//! one of them is `Keep` (`protect.location.<id>`), unless it lies in one of the
//! location's documented exceptions (e.g. `/usr/local`).
//!
//! A location's last name may end in `*` to match every name it starts with:
//! `OneDrive - *` is every work or school `OneDrive` folder, whatever the
//! organization is called.
//!
//! A directory that holds one of them is `Keep` too (security audit FS-3):
//! moving `~/Library` or a home folder would move the keychains, browser
//! profiles or sync folders inside it. Such an ancestor fires
//! `protect.contains-<kind>`, naming what it holds (`system`, `sensitive` or
//! `cloud`). Whether the location exists is not checked: a directory where it
//! would be is kept either way.
//!
//! The data is embedded and validated at first use. If it is invalid, every
//! item is kept (`protect.location-data`), and so is every item when a root the
//! data needs (home; on Windows also the Windows and local app data directories)
//! is unknown (`protect.location-context`): a deny-list that cannot be checked
//! must fail closed.
//!
//! Paths are compared component by component, without case on macOS and
//! Windows (their default volumes are case-insensitive), exactly elsewhere.
//! Paths only ever add protection here; actions use identities (ADR-0016).

use std::collections::BTreeSet;
use std::sync::OnceLock;

use lumen_domain::{PathFlavor, Platform, RawPath, RiskFactor, Verdict};
use serde::Deserialize;

use crate::{Finding, PolicyInput};

const DATA: &str = include_str!("../data/protected-locations.json");

/// Rule identifier used when the data cannot be loaded.
pub(crate) const DATA_RULE: &str = "protect.location-data";
/// Rule identifier used when a root the data needs is unknown.
pub(crate) const CONTEXT_RULE: &str = "protect.location-context";

/// The directory a location is relative to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Root {
    /// `/` on Unix-like systems.
    Filesystem,
    /// The Windows directory (`%WINDIR%`).
    WindowsDir,
    /// The root of any Windows volume (`C:\`).
    Volume,
    /// The user's home directory.
    Home,
    /// Windows `FOLDERID_LocalAppData`.
    LocalAppData,
}

/// What a location protects, which sets the risk recorded. Declared from the
/// most critical, which [`check_holds`] relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    /// Part of the operating system.
    System,
    /// Credentials, keys or a browser profile.
    Sensitive,
    /// A cloud provider's sync folder.
    Cloud,
}

/// One protected location.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Location {
    id: String,
    kind: Kind,
    platforms: BTreeSet<Platform>,
    root: Root,
    path: String,
    #[serde(default)]
    except: Vec<String>,
    source: String,
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.contains(['\\', ':', '*', '?'])
                && !part.chars().any(char::is_control)
        })
}

/// A safe relative path whose last name may end in one `*` after at least
/// one other character.
fn safe_pattern(path: &str) -> bool {
    match path.strip_suffix('*') {
        Some(stem) => !stem.ends_with('/') && safe_relative(stem),
        None => safe_relative(path),
    }
}

fn valid(location: &Location) -> bool {
    let windows_root = matches!(
        location.root,
        Root::WindowsDir | Root::Volume | Root::LocalAppData
    );
    let any_root = location.root == Root::Home;
    let id_ok = (1..=47).contains(&location.id.len())
        && location.id.as_bytes()[0].is_ascii_lowercase()
        && location
            .id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    id_ok
        && !location.platforms.is_empty()
        && location
            .platforms
            .iter()
            .all(|p| any_root || (*p == Platform::Windows) == windows_root)
        && safe_pattern(&location.path)
        && (location.except.is_empty() || safe_relative(&location.path))
        && location.except.iter().all(|e| {
            safe_relative(e)
                && e.strip_prefix(&location.path)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        && location.source.starts_with("https://")
        && !location.source.contains(char::is_whitespace)
}

fn parse(data: &str) -> Option<Vec<Location>> {
    let locations: Vec<Location> = serde_json::from_str(data).ok()?;
    let mut ids = BTreeSet::new();
    locations
        .iter()
        .all(|l| valid(l) && ids.insert(l.id.clone()))
        .then_some(locations)
}

fn locations() -> Option<&'static [Location]> {
    static LOADED: OnceLock<Option<Vec<Location>>> = OnceLock::new();
    LOADED.get_or_init(|| parse(DATA)).as_deref()
}

fn case_insensitive(platform: Platform) -> bool {
    matches!(
        platform,
        Platform::Macos | Platform::Windows | Platform::Ios
    )
}

fn fold(text: &str, platform: Platform) -> String {
    if case_insensitive(platform) {
        text.to_lowercase()
    } else {
        text.to_owned()
    }
}

/// The non-empty components of a path, folded for `platform`. A Windows
/// device prefix (`\\?\`, `\\.\`) is dropped so `\\?\C:\x` and `C:\x` agree.
pub(crate) fn components(path: &RawPath, platform: Platform) -> Vec<String> {
    let separators: &[u8] = match path.flavor() {
        PathFlavor::Unix => b"/",
        PathFlavor::Windows => b"\\/",
    };
    let mut parts: Vec<String> = path
        .as_bytes()
        .split(|b| separators.contains(b))
        .filter(|part| !part.is_empty())
        .map(|part| fold(&String::from_utf8_lossy(part), platform))
        .collect();
    if path.flavor() == PathFlavor::Windows && parts.first().is_some_and(|p| p == "?" || p == ".") {
        parts.remove(0);
    }
    parts
}

/// One name of a location's path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    /// This name.
    Exact(String),
    /// Any name starting with this.
    Prefix(String),
}

impl Part {
    fn matches(&self, name: &str) -> bool {
        match self {
            Self::Exact(exact) => name == exact,
            Self::Prefix(prefix) => name.starts_with(prefix.as_str()),
        }
    }
}

fn pattern(path: &str, platform: Platform) -> Vec<Part> {
    path.split('/')
        .map(|part| match part.strip_suffix('*') {
            Some(prefix) => Part::Prefix(fold(prefix, platform)),
            None => Part::Exact(fold(part, platform)),
        })
        .collect()
}

/// The components `location` is relative to, if they are known for `names`.
fn base(location: &Location, names: &[String], input: &PolicyInput) -> Option<Vec<String>> {
    let platform = input.context.platform;
    let context = &input.context;
    match location.root {
        Root::Filesystem => Some(Vec::new()),
        Root::WindowsDir => context
            .windows_dir
            .as_ref()
            .map(|d| components(d, platform)),
        Root::Home => context.home.as_ref().map(|d| components(d, platform)),
        Root::LocalAppData => context
            .local_app_data
            .as_ref()
            .map(|d| components(d, platform)),
        Root::Volume => names
            .first()
            .filter(|drive| drive.len() == 2 && drive.ends_with(':'))
            .map(|drive| vec![drive.clone()]),
    }
}

/// `relative` below `base`, as parts to match names against.
fn full(base: &[String], relative: &str, platform: Platform) -> Vec<Part> {
    let mut parts: Vec<Part> = base.iter().cloned().map(Part::Exact).collect();
    parts.extend(pattern(relative, platform));
    parts
}

/// Whether `names` is `location` or below it, outside its exceptions.
fn covers(location: &Location, names: &[String], input: &PolicyInput) -> bool {
    let platform = input.context.platform;
    let Some(base) = base(location, names, input) else {
        return false;
    };
    let under = |relative: &str| {
        let full = full(&base, relative, platform);
        names.len() >= full.len()
            && full
                .iter()
                .zip(names)
                .all(|(part, name)| part.matches(name))
    };
    under(&location.path) && !location.except.iter().any(|e| under(e))
}

/// Whether `names` lies strictly above `location`, so a directory there holds
/// it. Only a location's last name can be a prefix, and an ancestor never
/// reaches it.
fn holds(location: &Location, names: &[String], input: &PolicyInput) -> bool {
    let Some(base) = base(location, names, input) else {
        return false;
    };
    let full = full(&base, &location.path, input.context.platform);
    names.len() < full.len()
        && full
            .iter()
            .zip(names)
            .all(|(part, name)| part.matches(name))
}

fn risk_of(kind: Kind) -> RiskFactor {
    match kind {
        Kind::System => RiskFactor::OsProtected,
        Kind::Sensitive => RiskFactor::SensitiveData,
        Kind::Cloud => RiskFactor::CloudBacked,
    }
}

/// `Keep` if any path of the subject is in a protected location.
pub(crate) fn check(input: &PolicyInput) -> Option<Finding> {
    let keep = |rule: String, kind: Kind| Finding {
        verdict: Verdict::Keep,
        evidence: BTreeSet::new(),
        risk: BTreeSet::from([risk_of(kind)]),
        rule: Some(rule),
    };
    let Some(locations) = locations() else {
        return Some(keep(DATA_RULE.to_owned(), Kind::System));
    };
    let platform = input.context.platform;
    let context = &input.context;
    // Without these roots their protections could not be checked.
    let windows = platform == Platform::Windows;
    if context.home.is_none()
        || windows && (context.windows_dir.is_none() || context.local_app_data.is_none())
    {
        return Some(keep(CONTEXT_RULE.to_owned(), Kind::System));
    }
    for path in &input.paths {
        let names = components(path, platform);
        if let Some(found) = locations
            .iter()
            .filter(|l| l.platforms.contains(&platform))
            .find(|l| covers(l, &names, input))
        {
            return Some(keep(format!("protect.location.{}", found.id), found.kind));
        }
    }
    None
}

/// `Keep` if any path of the subject holds a protected location: it is a
/// strict ancestor of one. The rule names the most critical kind held
/// (system, then sensitive, then cloud) and the risk lists every kind held.
///
/// Missing data or roots are [`check`]'s to report; this check then finds
/// nothing, and `check` keeps the item.
pub(crate) fn check_holds(input: &PolicyInput) -> Option<Finding> {
    let locations = locations()?;
    let platform = input.context.platform;
    let mut kinds = BTreeSet::new();
    for path in &input.paths {
        let names = components(path, platform);
        kinds.extend(
            locations
                .iter()
                .filter(|l| l.platforms.contains(&platform) && holds(l, &names, input))
                .map(|l| l.kind),
        );
    }
    let first = *kinds.first()?;
    Some(Finding {
        verdict: Verdict::Keep,
        evidence: BTreeSet::new(),
        risk: kinds.into_iter().map(risk_of).collect(),
        rule: Some(
            match first {
                Kind::System => "protect.contains-system",
                Kind::Sensitive => "protect.contains-sensitive",
                Kind::Cloud => "protect.contains-cloud",
            }
            .to_owned(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use lumen_domain::{EvidenceHash, FileId, FileIdentity, Subject, VolumeId};

    use super::*;
    use crate::Context;

    fn unix(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn windows(path: &str) -> RawPath {
        let wide: Vec<u16> = path.encode_utf16().collect();
        RawPath::from_windows_wide(&wide).unwrap_or_else(|_| unreachable!())
    }

    /// A complete context for `platform`.
    fn context(platform: Platform) -> Context {
        match platform {
            Platform::Windows => Context {
                platform,
                home: Some(windows(r"C:\Users\ana")),
                windows_dir: Some(windows(r"C:\Windows")),
                local_app_data: Some(windows(r"C:\Users\ana\AppData\Local")),
                mechanisms: BTreeSet::new(),
                complete_sources: BTreeSet::new(),
                single_user: false,
            },
            Platform::Macos => Context {
                platform,
                home: Some(unix("/Users/ana")),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::new(),
                complete_sources: BTreeSet::new(),
                single_user: false,
            },
            _ => Context {
                platform,
                home: Some(unix("/home/ana")),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::new(),
                complete_sources: BTreeSet::new(),
                single_user: false,
            },
        }
    }

    fn rule_at(context: Context, path: RawPath) -> Option<String> {
        check(&PolicyInput {
            subject: Subject::File {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
            },
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            evidence: Vec::new(),
            paths: vec![path],
            context,
            observed: None,
            knowledge: std::collections::BTreeMap::new(),
            jev: None,
        })
        .and_then(|f| f.rule)
    }

    fn mac(path: &str) -> Option<String> {
        rule_at(context(Platform::Macos), unix(path))
    }

    fn linux(path: &str) -> Option<String> {
        rule_at(context(Platform::Linux), unix(path))
    }

    fn win(path: &str) -> Option<String> {
        rule_at(context(Platform::Windows), windows(path))
    }

    fn held_at(context: Context, path: RawPath) -> Option<String> {
        check_holds(&PolicyInput {
            subject: Subject::File {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
            },
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            evidence: Vec::new(),
            paths: vec![path],
            context,
            observed: None,
            knowledge: std::collections::BTreeMap::new(),
            jev: None,
        })
        .and_then(|f| f.rule)
    }

    #[test]
    fn folders_holding_a_protected_location_are_kept() {
        let sensitive = Some("protect.contains-sensitive");
        let system = Some("protect.contains-system");
        let mac = |path: &str| held_at(context(Platform::Macos), unix(path));
        for path in [
            "/Users/ana",
            "/Users",
            "/Users/ana/Library",
            "/users/ANA/library",
            "/Users/ana/Library/Application Support",
            "/Users/ana/Library/Application Support/Google",
            "/Users/ana/Library/Application Support/BraveSoftware",
        ] {
            assert_eq!(mac(path).as_deref(), sensitive, "{path}");
        }
        for path in ["/", "/private", "/private/var"] {
            assert_eq!(mac(path).as_deref(), system, "{path}");
        }
        for path in [
            // The location itself and what is below it are `check`'s.
            "/Users/ana/.ssh",
            "/Users/ana/Library/Application Support/Google/Chrome",
            "/usr",
            // Siblings, look-alikes and names that only start the same.
            "/Users/ana/.ssh-old",
            "/Users/ana/.ss",
            "/Users/ana/Library/Application Support/Googlex",
            "/Users/ana/Library/Application Support/Microsoft",
            "/Users/ana/Library/Caches",
            "/Users/ana/projects",
            "/private/var/folders",
            "/usr/local",
            "/Users/bob",
        ] {
            assert_eq!(mac(path), None, "{path}");
        }
        let linux = |path: &str| held_at(context(Platform::Linux), unix(path));
        assert_eq!(linux("/home/ana/.config").as_deref(), sensitive);
        assert_eq!(linux("/home/ana/.Config"), None, "case-sensitive");
        assert_eq!(linux("/home/ANA"), None, "case-sensitive");
        assert_eq!(linux("/private/var"), None, "macOS-only entries");
    }

    #[test]
    fn windows_folders_holding_a_protected_location_are_kept() {
        let win = |path: &str| held_at(context(Platform::Windows), windows(path));
        for path in [
            r"C:\",
            r"C:\Windows",
            r"c:\windows\softwaredistribution",
            r"D:\",
        ] {
            assert_eq!(
                win(path).as_deref(),
                Some("protect.contains-system"),
                "{path}"
            );
        }
        for path in [
            r"C:\Users",
            r"\\?\C:\Users\ana",
            r"C:\Users\ana\AppData",
            r"C:\Users\ana\AppData\Local\Microsoft",
            r"c:\users\ana\appdata\local\google\chrome",
        ] {
            assert_eq!(
                win(path).as_deref(),
                Some("protect.contains-sensitive"),
                "{path}"
            );
        }
        for path in [
            r"C:\Windows\Temp",
            r"C:\Windows\SoftwareDistribution\Download",
            r"C:\Users\ana\AppData\Local\Temp",
            r"C:\Users\ana\OneDrive - Contoso",
            r"D:\data",
            r"\\server\share",
        ] {
            assert_eq!(win(path), None, "{path}");
        }
    }

    #[test]
    fn a_folder_holding_several_kinds_records_each_risk() {
        let finding = check_holds(&PolicyInput {
            subject: Subject::File {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
            },
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            evidence: Vec::new(),
            paths: vec![unix("/Users/ana/Library")],
            context: context(Platform::Macos),
            observed: None,
            knowledge: std::collections::BTreeMap::new(),
            jev: None,
        });
        assert_eq!(
            finding.map(|f| f.risk),
            Some(BTreeSet::from([
                RiskFactor::SensitiveData,
                RiskFactor::CloudBacked
            ]))
        );
    }

    #[test]
    fn a_regenerable_folder_holding_a_protected_location_is_kept() -> Result<(), crate::PolicyError>
    {
        use lumen_domain::{Basis, Evidence, Fact, Provenance, SourceName, Timestamp};
        let fact = |fact| Evidence {
            subject: crate::testing::file(1),
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        };
        let entry = SourceName::new("entry").unwrap_or_else(|_| unreachable!());
        let mut input = crate::testing::input(vec![
            fact(Fact::InUse(false)),
            fact(Fact::KnowledgeBaseMatch {
                entry: entry.clone(),
            }),
        ]);
        input.knowledge =
            std::collections::BTreeMap::from([(entry, crate::KnowledgeClass::Regenerable)]);
        input.paths = vec![unix("/home/t/.config/app")];
        assert_eq!(crate::evaluate(&input)?.verdict(), Verdict::Quarantine);
        input.paths = vec![unix("/home/t/.config")];
        let decision = crate::evaluate(&input)?;
        assert_eq!(decision.verdict(), Verdict::Keep);
        let fired: Vec<&str> = decision
            .fired_rules()
            .iter()
            .map(|r| r.rule.as_str())
            .collect();
        assert_eq!(fired, ["protect.contains-sensitive"]);
        Ok(())
    }

    #[test]
    fn macos_system_locations_are_kept_except_documented_exceptions() {
        let sip_usr = Some("protect.location.macos-sip-usr");
        assert_eq!(
            mac("/System/Library/Caches/x").as_deref(),
            Some("protect.location.macos-sip-system")
        );
        assert_eq!(mac("/USR/lib/x").as_deref(), sip_usr, "case-insensitive");
        assert_eq!(mac("/usr").as_deref(), sip_usr);
        assert_eq!(mac("/usr/local/lib/x"), None, "documented exception");
        assert_eq!(mac("/usr/localx").as_deref(), sip_usr);
        assert_eq!(
            mac("/private/var/db/x").as_deref(),
            Some("protect.location.macos-sip-var-db")
        );
        assert_eq!(mac("/private/var/folders/x"), None);
        assert_eq!(mac("/Users/ana/System"), None, "anchored at the root");
        assert_eq!(linux("/usr/lib"), None, "macOS-only entries");
    }

    #[test]
    fn windows_system_locations_are_kept() {
        assert_eq!(
            win(r"c:\windows\winsxs\amd64_x").as_deref(),
            Some("protect.location.windows-component-store")
        );
        assert_eq!(
            win(r"\\?\C:\Windows\System32\drivers").as_deref(),
            Some("protect.location.windows-system32"),
            "device prefix ignored"
        );
        assert_eq!(
            win(r"D:\pagefile.sys").as_deref(),
            Some("protect.location.windows-page-file"),
            "any volume root"
        );
        assert_eq!(win(r"C:\data\pagefile.sys"), None);
        assert_eq!(win(r"C:\Windows\Temp\x"), None);
    }

    #[test]
    fn credentials_keys_and_browser_profiles_are_kept() {
        assert_eq!(
            mac("/Users/ana/.ssh/id_ed25519").as_deref(),
            Some("protect.location.ssh")
        );
        assert_eq!(
            linux("/home/ana/.gnupg").as_deref(),
            Some("protect.location.gnupg")
        );
        assert_eq!(
            mac("/Users/ana/Library/Keychains/login.keychain-db").as_deref(),
            Some("protect.location.macos-keychains")
        );
        assert_eq!(
            win(r"C:\Users\ana\.aws\credentials").as_deref(),
            Some("protect.location.aws-cli")
        );
        assert_eq!(
            mac("/Users/ana/Library/Application Support/Google/Chrome/Default").as_deref(),
            Some("protect.location.chrome-macos")
        );
        assert_eq!(
            win(r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default").as_deref(),
            Some("protect.location.chrome-windows")
        );
        assert_eq!(
            linux("/home/ana/.config/google-chrome").as_deref(),
            Some("protect.location.chrome-linux")
        );
        assert_eq!(mac("/Users/bob/.ssh"), None, "another user's home");
        assert_eq!(
            mac("/Users/ana/Library/Caches/Google/Chrome"),
            None,
            "caches"
        );
    }

    #[test]
    fn other_chromium_browsers_profiles_are_kept_and_their_caches_are_not() {
        for (path, rule) in [
            (
                "/Users/ana/Library/Application Support/Microsoft Edge/Default/Cookies",
                "edge-macos",
            ),
            (
                "/Users/ana/Library/Application Support/BraveSoftware/Brave-Browser/Default",
                "brave-macos",
            ),
            (
                "/Users/ana/Library/Application Support/Arc/User Data/Default",
                "arc-macos",
            ),
            (
                "/Users/ana/Library/Application Support/Arc/StorableSidebar.json",
                "arc-macos",
            ),
        ] {
            assert_eq!(
                mac(path).as_deref(),
                Some(format!("protect.location.{rule}").as_str()),
                "{path}"
            );
        }
        for (path, rule) in [
            (
                r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default",
                "edge-windows",
            ),
            (
                r"C:\Users\ana\AppData\Local\BraveSoftware\Brave-Browser\User Data",
                "brave-windows",
            ),
        ] {
            assert_eq!(
                win(path).as_deref(),
                Some(format!("protect.location.{rule}").as_str()),
                "{path}"
            );
        }
        // Their separate disk caches stay decidable.
        for path in [
            "/Users/ana/Library/Caches/Microsoft Edge/Default/Cache",
            "/Users/ana/Library/Caches/BraveSoftware/Brave-Browser/Default/Cache",
            "/Users/ana/Library/Caches/Arc/User Data/Default/Cache",
            "/Users/ana/Library/Application Support/BraveSoftware/Brave-Browser-Beta",
        ] {
            assert_eq!(mac(path), None, "{path}");
        }
    }

    #[test]
    fn cloud_sync_folders_and_everything_below_them_are_kept() {
        for (path, rule) in [
            (
                "/Users/ana/Library/CloudStorage/Dropbox/project/node_modules",
                "macos-cloud-storage",
            ),
            (
                "/Users/ana/Library/CloudStorage/OneDrive-Contoso",
                "macos-cloud-storage",
            ),
            (
                "/Users/ana/Library/CloudStorage/GoogleDrive-ana@example.com/My Drive/x",
                "macos-cloud-storage",
            ),
            (
                "/Users/ana/Library/Mobile Documents/com~apple~CloudDocs/app/build",
                "icloud-drive-macos",
            ),
            ("/Users/ana/Dropbox/app/node_modules", "dropbox"),
            ("/Users/ana/Dropbox (Contoso)/x", "dropbox-named"),
            ("/Users/ana/OneDrive/x", "onedrive-macos-legacy"),
            ("/Users/ana/OneDrive - Contoso/x", "onedrive-organization"),
        ] {
            assert_eq!(
                mac(path).as_deref(),
                Some(format!("protect.location.{rule}").as_str()),
                "{path}"
            );
        }
        for (path, rule) in [
            (
                r"C:\Users\ana\OneDrive\app\node_modules",
                "onedrive-windows",
            ),
            (
                r"C:\Users\ana\onedrive - contoso ltd\x",
                "onedrive-organization",
            ),
            (r"C:\Users\ana\iCloud Drive\x", "icloud-drive-windows"),
            (r"C:\Users\ana\iCloudDrive", "icloud-drive-windows-legacy"),
            (r"C:\Users\ana\Dropbox\x", "dropbox"),
        ] {
            assert_eq!(
                win(path).as_deref(),
                Some(format!("protect.location.{rule}").as_str()),
                "{path}"
            );
        }
        assert_eq!(
            linux("/home/ana/Dropbox/x").as_deref(),
            Some("protect.location.dropbox")
        );
        // Neighbours, other users' homes and look-alikes stay decidable.
        for path in [
            "/Users/ana/Library/Caches/com.getdropbox.dropbox",
            "/Users/ana/OneDriveBackup",
            "/Users/ana/Dropbox2",
            "/Users/ana/projects/Dropbox/x",
            "/Users/bob/Library/CloudStorage/Dropbox",
        ] {
            assert_eq!(mac(path), None, "{path}");
        }
        assert_eq!(win(r"C:\OneDriveTemp\x"), None);
        assert_eq!(linux("/home/ana/OneDrive/x"), None, "not a Linux location");
    }

    #[test]
    fn cloud_locations_record_cloud_risk() {
        let finding = check(&PolicyInput {
            subject: Subject::File {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
            },
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            evidence: Vec::new(),
            paths: vec![unix("/Users/ana/Library/CloudStorage/Box-Box/x")],
            context: context(Platform::Macos),
            observed: None,
            knowledge: std::collections::BTreeMap::new(),
            jev: None,
        });
        assert_eq!(
            finding.map(|f| f.risk),
            Some(BTreeSet::from([RiskFactor::CloudBacked]))
        );
    }

    #[test]
    fn only_a_last_name_may_end_in_a_wildcard() {
        for good in ["a/b*", "OneDrive - *", "Dropbox (*", "a b"] {
            assert!(safe_pattern(good), "{good:?}");
        }
        for bad in ["*", "a/*", "a*/b", "a**", "a*b", "a/b*/", ""] {
            assert!(!safe_pattern(bad), "{bad:?}");
        }
    }

    #[test]
    fn sensitive_locations_record_sensitive_risk() {
        let finding = check(&PolicyInput {
            subject: Subject::File {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
            },
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            evidence: Vec::new(),
            paths: vec![unix("/Users/ana/.ssh")],
            context: context(Platform::Macos),
            observed: None,
            knowledge: std::collections::BTreeMap::new(),
            jev: None,
        });
        assert_eq!(
            finding.map(|f| f.risk),
            Some(BTreeSet::from([RiskFactor::SensitiveData]))
        );
    }

    #[test]
    fn unknown_roots_keep_everything() {
        let anywhere = || windows(r"D:\data\x");
        let mut no_windows_dir = context(Platform::Windows);
        no_windows_dir.windows_dir = None;
        let mut no_local_app_data = context(Platform::Windows);
        no_local_app_data.local_app_data = None;
        let mut homeless = context(Platform::Macos);
        homeless.home = None;
        for (context, path) in [
            (no_windows_dir, anywhere()),
            (no_local_app_data, anywhere()),
            (homeless, unix("/tmp/x")),
        ] {
            assert_eq!(rule_at(context, path).as_deref(), Some(CONTEXT_RULE));
        }
    }

    #[test]
    fn relative_paths_are_plain_names() {
        assert!(safe_relative("a/b c/d.e"));
        for bad in [
            "", "/a", "a/", "a//b", ".", "a/./b", "..", "a/..", r"a\b", "c:", "a*", "a?", "a\u{7}",
        ] {
            assert!(!safe_relative(bad), "{bad:?}");
        }
    }

    #[test]
    fn linux_paths_compare_exactly_and_unc_shares_are_not_volumes() {
        assert_eq!(linux("/home/ana/.SSH/id_ed25519"), None, "case-sensitive");
        assert_eq!(
            linux("/home/ana/.ssh").as_deref(),
            Some("protect.location.ssh")
        );
        assert_eq!(
            win(r"\\server\pagefile.sys"),
            None,
            "a share is not a drive"
        );
        assert_eq!(
            win(r"\\ab\pagefile.sys"),
            None,
            "two letters are not a drive"
        );
    }

    #[test]
    fn the_embedded_data_is_valid() {
        let loaded = locations().map(<[Location]>::len);
        assert!(loaded.is_some_and(|n| n > 0), "{loaded:?}");
    }

    #[test]
    fn invalid_data_is_rejected() {
        let entry = |fields: &[(&str, &str)]| {
            let mut json = serde_json::json!({
                "id": "a",
                "kind": "system",
                "platforms": ["macos"],
                "root": "filesystem",
                "path": "usr",
                "except": ["usr/local"],
                "source": "https://x.example",
            });
            for (key, value) in fields {
                json[*key] = serde_json::from_str(value).unwrap_or_else(|_| unreachable!());
            }
            format!("[{json}]")
        };
        let ok = entry(&[]);
        assert!(parse(&ok).is_some());
        assert!(
            parse(&entry(&[
                ("root", r#""home""#),
                ("platforms", r#"["windows"]"#)
            ]))
            .is_some()
        );
        assert!(
            parse(&entry(&[
                ("kind", r#""cloud""#),
                ("path", r#""Dropbox (*""#),
                ("except", "[]")
            ]))
            .is_some()
        );
        for bad in [
            entry(&[("id", r#""A""#)]),
            entry(&[("kind", r#""other""#)]),
            entry(&[("platforms", "[]")]),
            entry(&[("platforms", r#"["windows"]"#)]),
            entry(&[("root", r#""windows_dir""#)]),
            entry(&[("root", r#""local_app_data""#)]),
            entry(&[("id", r#""1abc""#)]),
            entry(&[("path", r#""../usr""#), ("except", "[]")]),
            entry(&[("path", r#""""#), ("except", "[]")]),
            entry(&[("except", r#"["usr/../x"]"#)]),
            entry(&[("except", r#"["opt/local"]"#)]),
            entry(&[("except", r#"["usrlocal"]"#)]),
            entry(&[("source", r#""http://x.example""#)]),
            entry(&[("path", r#""usr*""#)]),
            entry(&[("path", r#""*""#), ("except", "[]")]),
            format!("[{},{}]", &ok[1..ok.len() - 1], &ok[1..ok.len() - 1]),
            "not json".to_owned(),
        ] {
            assert!(parse(&bad).is_none(), "{bad}");
        }
    }
}
