//! Protected locations (roadmap 7.3, ADR-0014 stage 1).
//!
//! `data/protected-locations.json` lists locations whose contents Lumen must
//! never touch, each with the official source that documents it: operating-system
//! locations (`system`) and credentials, keys and browser profiles
//! (`sensitive`). An item at or below one of them is `Keep` (`protect.location.<id>`),
//! unless it lies in one of the location's documented exceptions (e.g.
//! `/usr/local`).
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

/// What a location protects, which sets the risk recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    /// Part of the operating system.
    System,
    /// Credentials, keys or a browser profile.
    Sensitive,
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
        && safe_relative(&location.path)
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
fn components(path: &RawPath, platform: Platform) -> Vec<String> {
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

fn pattern(path: &str, platform: Platform) -> Vec<String> {
    path.split('/').map(|part| fold(part, platform)).collect()
}

/// Whether `names` is `location` or below it, outside its exceptions.
fn covers(location: &Location, names: &[String], input: &PolicyInput) -> bool {
    let platform = input.context.platform;
    let base: Vec<String> = match location.root {
        Root::Filesystem => Vec::new(),
        Root::WindowsDir => match &input.context.windows_dir {
            Some(dir) => components(dir, platform),
            None => return false,
        },
        Root::Home => match &input.context.home {
            Some(dir) => components(dir, platform),
            None => return false,
        },
        Root::LocalAppData => match &input.context.local_app_data {
            Some(dir) => components(dir, platform),
            None => return false,
        },
        Root::Volume => match names.first() {
            Some(drive) if drive.len() == 2 && drive.ends_with(':') => vec![drive.clone()],
            _ => return false,
        },
    };
    let under = |relative: &str| {
        let mut full = base.clone();
        full.extend(pattern(relative, platform));
        names.starts_with(&full)
    };
    under(&location.path) && !location.except.iter().any(|e| under(e))
}

/// `Keep` if any path of the subject is in a protected location.
pub(crate) fn check(input: &PolicyInput) -> Option<Finding> {
    let keep = |rule: String, kind: Kind| Finding {
        verdict: Verdict::Keep,
        evidence: BTreeSet::new(),
        risk: BTreeSet::from([match kind {
            Kind::System => RiskFactor::OsProtected,
            Kind::Sensitive => RiskFactor::SensitiveData,
        }]),
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
            },
            Platform::Macos => Context {
                platform,
                home: Some(unix("/Users/ana")),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::new(),
            },
            _ => Context {
                platform,
                home: Some(unix("/home/ana")),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::new(),
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
        for bad in [
            entry(&[("id", r#""A""#)]),
            entry(&[("kind", r#""other""#)]),
            entry(&[("platforms", "[]")]),
            entry(&[("platforms", r#"["windows"]"#)]),
            entry(&[("root", r#""windows_dir""#)]),
            entry(&[("root", r#""local_app_data""#)]),
            entry(&[("path", r#""../usr""#)]),
            entry(&[("path", r#""""#)]),
            entry(&[("except", r#"["opt/local"]"#)]),
            entry(&[("except", r#"["usrlocal"]"#)]),
            entry(&[("source", r#""http://x.example""#)]),
            format!("[{},{}]", &ok[1..ok.len() - 1], &ok[1..ok.len() - 1]),
            "not json".to_owned(),
        ] {
            assert!(parse(&bad).is_none(), "{bad}");
        }
    }
}
