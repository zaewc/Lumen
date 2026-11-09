//! Application icons for the Apps page (roadmap 16.4).
//!
//! [`AppIcons::remember`] records where each macOS application the software
//! list shows lies, by the list's key, whenever the list is read. The page
//! then asks for one icon at a time by that key ([`AppIcons::icon`]): it
//! never names a path. The icon is drawn small by the system (64 × 64 pixel
//! PNG, [`lumen_platform_macos::app_icon_png`]) and kept in memory per key,
//! version and location, so scrolling back does not ask again; a new version
//! or location is drawn again.
//!
//! Icons are display only: they are never stored, logged or sent to AI review
//! (ADR-0061 sends identity fields only). Windows and Linux have none yet.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use lumen_application::ports::InventoryReport;
use lumen_domain::{AppIdScheme, RawPath};

/// The most applications remembered, so memory stays bounded whatever the
/// inventory reports. Each icon is at most 32 KiB of PNG.
pub const MAX_REMEMBERED: usize = 4096;

/// How an icon is drawn from an application's location: PNG bytes, or `None`.
pub type Render = fn(&RawPath) -> Option<Vec<u8>>;

/// Whether an application's icon was drawn yet.
#[derive(Debug, Clone)]
enum Drawn {
    /// Not asked for yet.
    Not,
    /// Drawn: the data URL, or `None` if the system gave no icon.
    Done(Option<Arc<str>>),
}

#[derive(Debug)]
struct Entry {
    path: RawPath,
    version: Option<String>,
    icon: Drawn,
}

/// Icons of the applications in the latest software list, drawn on request.
pub struct AppIcons {
    entries: Mutex<BTreeMap<String, Entry>>,
    render: Render,
}

impl std::fmt::Debug for AppIcons {
    /// How many applications are remembered; never the paths or images.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppIcons")
            .field(
                "remembered",
                &self.entries.try_lock().ok().map(|entries| entries.len()),
            )
            .finish_non_exhaustive()
    }
}

impl Default for AppIcons {
    fn default() -> Self {
        Self::new()
    }
}

impl AppIcons {
    /// Icons drawn by this platform: the system's icon on macOS, none
    /// elsewhere yet.
    #[must_use]
    pub fn new() -> Self {
        Self::with_renderer(system)
    }

    /// Icons drawn by `render` (tests use a fixture).
    #[must_use]
    pub fn with_renderer(render: Render) -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            render,
        }
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records the macOS applications in `reports` by the software list's
    /// key, replacing what was recorded before. An icon already drawn is kept
    /// only if the application's version and location are unchanged. An
    /// application with no single location, or a key that several share, is
    /// not recorded: which icon is meant would be unknown.
    pub fn remember(&self, reports: &[InventoryReport]) {
        let mut seen: BTreeMap<String, Option<(RawPath, Option<String>)>> = BTreeMap::new();
        for app in reports.iter().flat_map(|report| &report.applications) {
            let Some(id) = app.ids.iter().find(|id| id.scheme == AppIdScheme::BundleId) else {
                continue;
            };
            let found = match app.install_locations.as_slice() {
                [path] => Some((
                    path.clone(),
                    app.version.as_ref().map(|v| v.raw().to_owned()),
                )),
                _ => None,
            };
            seen.entry(crate::uninstall::key(&id.value))
                .and_modify(|slot| *slot = None)
                .or_insert(found);
        }
        let mut entries = self.entries();
        let mut old = std::mem::take(&mut *entries);
        for (key, (path, version)) in seen
            .into_iter()
            .filter_map(|(key, found)| found.map(|found| (key, found)))
            .take(MAX_REMEMBERED)
        {
            let icon = old
                .remove(&key)
                .filter(|entry| entry.path == path && entry.version == version)
                .map_or(Drawn::Not, |entry| entry.icon);
            entries.insert(
                key,
                Entry {
                    path,
                    version,
                    icon,
                },
            );
        }
    }

    /// The icon of the application the latest list shows under `key`, as a
    /// `data:image/png;base64,` URL, or `None` if the list has no such
    /// application or the system gave no icon. Drawn once per key, version
    /// and location.
    #[must_use]
    pub fn icon(&self, key: &str) -> Option<Arc<str>> {
        let (path, version) = {
            let entries = self.entries();
            let entry = entries.get(key)?;
            if let Drawn::Done(icon) = &entry.icon {
                return icon.clone();
            }
            (entry.path.clone(), entry.version.clone())
        };
        // Drawn without holding the lock, so other rows are not kept waiting.
        let icon: Option<Arc<str>> = (self.render)(&path)
            .map(|png| format!("data:image/png;base64,{}", base64(&png)).into());
        let mut entries = self.entries();
        if let Some(entry) = entries.get_mut(key)
            && entry.path == path
            && entry.version == version
        {
            entry.icon = Drawn::Done(icon.clone());
        }
        icon
    }
}

#[cfg(target_os = "macos")]
fn system(path: &RawPath) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt as _;
    let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()));
    lumen_platform_macos::app_icon_png(path)
}

#[cfg(not(target_os = "macos"))]
fn system(_path: &RawPath) -> Option<Vec<u8>> {
    None
}

/// Standard base64 with padding (RFC 4648 section 4).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> shift & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use lumen_domain::{AppId, Application, SourceName, SourceStatus, UntrustedText};

    use super::*;

    static DRAWN: AtomicUsize = AtomicUsize::new(0);

    fn counting(path: &RawPath) -> Option<Vec<u8>> {
        DRAWN.fetch_add(1, Ordering::SeqCst);
        path.as_bytes().ends_with(b".app").then(|| b"png".to_vec())
    }

    fn path(text: &str) -> RawPath {
        RawPath::from_unix_bytes(text).unwrap()
    }

    fn app(id: &str, at: &str, version: &str) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]),
            name: None,
            version: Some(UntrustedText::new(version)),
            vendor: None,
            signature: None,
            install_locations: vec![path(at)],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn report(applications: Vec<Application>) -> Vec<InventoryReport> {
        vec![InventoryReport {
            source: SourceName::new("macos.applications").unwrap(),
            status: SourceStatus::Complete,
            applications,
            services: Vec::new(),
            packages: Vec::new(),
        }]
    }

    #[test]
    fn base64_matches_rfc_4648_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
        assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
    }

    #[test]
    fn icons_are_drawn_once_per_key_version_and_location() {
        // One test owns the counter, so parallel tests cannot disturb it.
        let icons = AppIcons::with_renderer(counting);
        let before = DRAWN.load(Ordering::SeqCst);
        icons.remember(&report(vec![
            app("dev.zed.Zed", "/Applications/Zed.app", "1.0"),
            app("com.example.Odd", "/Applications/Odd", "1.0"),
        ]));
        let url = icons.icon("MacApp:dev.zed.Zed").unwrap();
        assert_eq!(&*url, "data:image/png;base64,cG5n");
        assert_eq!(icons.icon("MacApp:dev.zed.Zed").unwrap(), url);
        // No icon is remembered too, and not asked for again.
        assert_eq!(icons.icon("MacApp:com.example.Odd"), None);
        assert_eq!(icons.icon("MacApp:com.example.Odd"), None);
        assert_eq!(DRAWN.load(Ordering::SeqCst) - before, 2);

        // Reading the list again keeps an unchanged app's icon.
        icons.remember(&report(vec![app(
            "dev.zed.Zed",
            "/Applications/Zed.app",
            "1.0",
        )]));
        assert!(icons.icon("MacApp:dev.zed.Zed").is_some());
        assert_eq!(DRAWN.load(Ordering::SeqCst) - before, 2);
        // A new version is drawn again, and a vanished app has none.
        icons.remember(&report(vec![app(
            "dev.zed.Zed",
            "/Applications/Zed.app",
            "1.1",
        )]));
        assert!(icons.icon("MacApp:dev.zed.Zed").is_some());
        assert_eq!(DRAWN.load(Ordering::SeqCst) - before, 3);
        assert_eq!(icons.icon("MacApp:com.example.Odd"), None);
        assert_eq!(DRAWN.load(Ordering::SeqCst) - before, 3);
    }

    #[test]
    fn only_listed_apps_with_one_location_and_a_unique_key_get_icons() {
        let icons = AppIcons::with_renderer(|_| Some(b"png".to_vec()));
        let mut two_places = app("com.example.Two", "/Applications/Two.app", "1");
        two_places
            .install_locations
            .push(path("/Users/me/Applications/Two.app"));
        icons.remember(&report(vec![
            two_places,
            app("com.example.Twin", "/Applications/Twin.app", "1"),
            app("com.example.Twin", "/Users/me/Applications/Twin.app", "1"),
            app("com.example.One", "/Applications/One.app", "1"),
        ]));
        assert_eq!(icons.icon("MacApp:com.example.Two"), None);
        assert_eq!(icons.icon("MacApp:com.example.Twin"), None);
        assert!(icons.icon("MacApp:com.example.One").is_some());
        // Not a key the list gave: never a path, never another origin.
        assert_eq!(icons.icon("/Applications/One.app"), None);
        assert_eq!(icons.icon("Homebrew:com.example.One"), None);
    }
}
