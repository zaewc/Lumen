//! The macOS application inventory (roadmap 6.6, ADR-0013).
//!
//! Finds application bundles in the standard locations, one level deep (vendor
//! folders and `Utilities`), without following symlinks. For each bundle it reads
//! the identifier, name and version through `CFBundle` (the system's own plist
//! parsing), and the signing team after the platform verified the signature.
//!
//! Signatures use `kSecCSBasicValidateOnly`: the signature and certificate chain
//! are verified, so the Team ID is authenticated, but signed contents are not
//! re-hashed. That is about 22× faster (44 apps: 0.89 s instead of 19.6 s, Xcode
//! alone 8.2 s) and is recorded as [`SignatureCheck::SignerVerified`](lumen_domain::SignatureCheck::SignerVerified), never as
//! `Valid`.

use std::collections::BTreeSet;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::ports::{InventoryReport, InventorySource};
use lumen_domain::{
    AppId, AppIdScheme, Application, RawPath, SourceName, SourceStatus, UntrustedText,
};
use objc2_core_foundation::{CFBundle, CFDictionary, CFString, CFURL};

/// The inventory source name.
const SOURCE: &str = "macos.applications";

/// Lists installed applications.
#[derive(Debug, Clone)]
pub struct AppInventory {
    roots: Vec<PathBuf>,
}

impl AppInventory {
    /// The standard locations: `/Applications`, `/System/Applications`, the
    /// Safari cryptex (`/Applications/Safari.app` is only a symlink into it, and
    /// symlinks are not followed) and the user's `~/Applications`.
    pub fn new() -> Self {
        let mut roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Cryptexes/App/System/Applications"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(Path::new(&home).join("Applications"));
        }
        Self { roots }
    }

    /// Specific locations instead of the standard ones.
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }
}

impl Default for AppInventory {
    fn default() -> Self {
        Self::new()
    }
}

fn source() -> SourceName {
    SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("valid source name"))
}

fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

/// Bundles directly in `dir` and one level below it. Returns `false` if `dir`
/// exists but could not be read completely.
fn find_bundles(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // A location that does not exist (no ~/Applications) has nothing in it.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    let mut complete = true;
    for entry in entries {
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };
        // `file_type` does not follow symlinks: a link to an app is not an app here.
        let Ok(kind) = entry.file_type() else {
            complete = false;
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let path = entry.path();
        if is_bundle(&path) {
            out.push(path);
        } else if depth > 0 {
            complete &= find_bundles(&path, depth - 1, out);
        }
    }
    complete
}

fn text(
    value: Option<objc2_core_foundation::CFRetained<objc2_core_foundation::CFType>>,
) -> Option<UntrustedText> {
    let value = value?.downcast::<CFString>().ok()?;
    Some(UntrustedText::new(value.to_string()))
}

/// What reading one bundle gave.
enum Read {
    /// An application with a bundle identifier.
    App(Box<Application>),
    /// Its information property list was read and names an executable, but no
    /// bundle identifier: a bundle with no ID, which owns nothing named by ID
    /// (ADR-0068 rule 5), such as a launcher shortcut.
    NoIdentifier,
    /// It could not be read, or says too little to tell.
    Unreadable,
}

/// Reads one bundle.
fn application(path: &Path) -> Read {
    let Some(url) = CFURL::from_directory_path(path) else {
        return Read::Unreadable;
    };
    let Some(bundle) = CFBundle::new(None, Some(&url)) else {
        return Read::Unreadable;
    };
    let key = |name: &str| bundle.value_for_info_dictionary_key(Some(&CFString::from_str(name)));
    let Some(id) = bundle.identifier().map(|id| id.to_string()) else {
        return if key("CFBundleExecutable").is_some() {
            Read::NoIdentifier
        } else {
            Read::Unreadable
        };
    };
    let name = text(key("CFBundleDisplayName")).or_else(|| text(key("CFBundleName")));
    let Ok(location) = RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()) else {
        return Read::Unreadable;
    };
    Read::App(Box::new(Application {
        ids: BTreeSet::from([AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new(id),
        }]),
        name,
        version: text(key("CFBundleShortVersionString")),
        vendor: None,
        signature: Some(verify_signature(&url)),
        install_locations: vec![location],
        sources: BTreeSet::from([source()]),
        uninstall: None,
    }))
}

/// The largest `Info.plist` [`bundle_identifier`] reads; real ones are a few
/// kilobytes.
const MAX_INFO_PLIST: u64 = 1024 * 1024;

/// The identifier in the bundle's `Contents/Info.plist`, for Lumen's helper,
/// which reads it as root (ADR-0063); `None` if it cannot be read or has none.
///
/// Unlike [`AppInventory`], which goes through `CFBundle`, it reads only that
/// one file, by handle: no link is followed anywhere in the path
/// (`O_NOFOLLOW_ANY`), opening never waits (`O_NONBLOCK`), and only a regular
/// file of at most 1 MiB is read. A bundle another account can write therefore
/// cannot point root at another file or hold it on a FIFO. No signature is
/// checked, which would read every file in the bundle as root.
#[must_use]
pub fn bundle_identifier(bundle: &Path) -> Option<String> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(bundle.join("Contents/Info.plist"))
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_INFO_PLIST {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_INFO_PLIST).read_to_end(&mut bytes).ok()?;
    let list = crate::cf::property_list(&bytes)?;
    let info = list.downcast_ref::<CFDictionary>()?;
    let id = crate::cf::get::<CFString>(info, &CFString::from_str("CFBundleIdentifier"))?;
    Some(id.to_string()).filter(|id| !id.is_empty())
}

impl InventorySource for AppInventory {
    fn name(&self) -> SourceName {
        source()
    }

    fn collect(&self, cancel: &CancelToken) -> InventoryReport {
        let mut bundles = Vec::new();
        let mut complete = true;
        for root in &self.roots {
            complete &= find_bundles(root, 1, &mut bundles);
        }
        let mut applications = Vec::with_capacity(bundles.len());
        for path in bundles {
            if cancel.is_cancelled() {
                complete = false;
                break;
            }
            match application(&path) {
                Read::App(app) => applications.push(*app),
                Read::NoIdentifier => {}
                // A bundle that cannot be read is not proof that nothing is there.
                Read::Unreadable => complete = false,
            }
        }
        InventoryReport {
            source: source(),
            status: if complete {
                SourceStatus::Complete
            } else {
                SourceStatus::Partial
            },
            applications,
            services: Vec::new(),
            packages: Vec::new(),
        }
    }
}

pub(crate) use signing::verify as verify_signature;

/// Code-signature checks through the Security framework: the only `unsafe` code
/// in this module.
#[allow(unsafe_code)]
mod signing {
    use std::ptr::NonNull;

    use lumen_domain::{CodeSignature, SignatureCheck, UntrustedText};
    use objc2_core_foundation::{CFDictionary, CFRetained, CFString, CFURL};
    use objc2_security::{
        SecCSFlags, SecCode, SecStaticCode, errSecCSUnsigned, kSecCSBasicValidateOnly,
        kSecCSSigningInformation, kSecCodeInfoTeamIdentifier,
    };

    /// Verifies the signature and certificate chain of a bundle or executable
    /// (not its contents) and, if valid, reads the signing team.
    pub(crate) fn verify(url: &CFURL) -> CodeSignature {
        let unknown = CodeSignature {
            signer: None,
            check: SignatureCheck::Unknown,
        };
        let mut raw: *const SecStaticCode = std::ptr::null();
        // SAFETY: invariant: on success the call stores a +1 retained static-code
        // reference in `raw`, and leaves it unchanged on error. Why unsafe: FFI
        // with an out-pointer. Why it holds: `raw` is a valid local; ownership is
        // taken exactly once below, only when the call succeeded and `raw` is
        // non-null.
        let status = unsafe {
            SecStaticCode::create_with_path(url, SecCSFlags::DefaultFlags, NonNull::from(&mut raw))
        };
        let Some(raw) = NonNull::new(raw.cast_mut()).filter(|_| status == 0) else {
            return unknown;
        };
        // SAFETY: invariant: `raw` is an owned +1 reference. Why unsafe: adopting
        // a raw CF reference. Why it holds: the create call returned it retained
        // (Create rule) and nothing else releases it.
        let code: CFRetained<SecStaticCode> = unsafe { CFRetained::from_raw(raw) };
        // SAFETY: invariant: `code` is a valid static-code object and no
        // requirement is passed. Why unsafe: FFI call. Why it holds: `code` is
        // owned for the duration of the call.
        let validity = unsafe { code.check_validity(SecCSFlags(kSecCSBasicValidateOnly), None) };
        if validity == errSecCSUnsigned {
            return CodeSignature {
                signer: None,
                check: SignatureCheck::Unsigned,
            };
        }
        if validity != 0 {
            return CodeSignature {
                signer: None,
                check: SignatureCheck::Invalid,
            };
        }
        CodeSignature {
            signer: team(&code),
            check: SignatureCheck::SignerVerified,
        }
    }

    /// The Team ID from the signing information of already validated code.
    /// Apple's own platform binaries have none.
    fn team(code: &SecStaticCode) -> Option<UntrustedText> {
        let mut raw: *const CFDictionary = std::ptr::null();
        // SAFETY: invariant: on success the call stores a +1 retained dictionary
        // in `raw`. Why unsafe: FFI with an out-pointer. Why it holds: `raw` is a
        // valid local, adopted once below only when non-null after success; the
        // code was validated first, as the header requires for trustworthy data.
        let status = unsafe {
            SecCode::copy_signing_information(
                code,
                SecCSFlags(kSecCSSigningInformation),
                NonNull::from(&mut raw),
            )
        };
        let raw = NonNull::new(raw.cast_mut()).filter(|_| status == 0)?;
        // SAFETY: invariant: `raw` is an owned +1 dictionary. Why unsafe: adopting
        // a raw CF reference. Why it holds: Copy rule; released once by the guard.
        let info: CFRetained<CFDictionary> = unsafe { CFRetained::from_raw(raw) };
        // SAFETY: invariant: reading an immutable extern static. Why unsafe: Rust
        // cannot see that the Security framework initialises it. Why it holds:
        // the constant is defined by the linked framework and never written.
        let key: &CFString = unsafe { kSecCodeInfoTeamIdentifier };
        let team = crate::cf::get::<CFString>(&info, key)?.to_string();
        (!team.is_empty()).then(|| UntrustedText::new(team))
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::SignatureCheck;
    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn bundle_id(app: &Application) -> String {
        app.ids
            .iter()
            .next()
            .map(|id| id.value.raw().to_owned())
            .unwrap_or_default()
    }

    #[test]
    fn finds_and_verifies_system_applications() {
        let report = AppInventory::new().collect(&CancelToken::new());
        assert_eq!(report.source.as_str(), SOURCE);
        // Calculator lives in /System/Applications; Safari in its cryptex (its
        // /Applications entry is a symlink). Both are Apple-signed everywhere.
        for id in ["com.apple.calculator", "com.apple.Safari"] {
            let app = report.applications.iter().find(|a| bundle_id(a) == id);
            assert_eq!(
                app.and_then(|a| a.signature.clone()).map(|s| s.check),
                Some(SignatureCheck::SignerVerified),
                "{id}"
            );
        }
    }

    #[test]
    fn the_helper_reads_one_identifier_by_handle() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            Node::File {
                path: "Word.app/Contents/Info.plist".into(),
                contents: info_plist("com.example.word"),
            },
            Node::File {
                path: "target/Info.plist".into(),
                contents: info_plist("com.example.secret"),
            },
            Node::Dir {
                path: "Linked.app/Contents".into(),
            },
            Node::Symlink {
                path: "Linked.app/Contents/Info.plist".into(),
                target: "../../target/Info.plist".into(),
            },
            Node::Symlink {
                path: "Through.app".into(),
                target: "Word.app".into(),
            },
            Node::File {
                path: "Empty.app/Contents/Info.plist".into(),
                contents: info_plist(""),
            },
            Node::Dir {
                path: "Bare.app".into(),
            },
        ])?;
        // The temporary folder's own path may pass through a link (`/var`).
        let root = std::fs::canonicalize(fx.root())?;
        let id = |name: &str| bundle_identifier(&root.join(name));
        assert_eq!(id("Word.app").as_deref(), Some("com.example.word"));
        for refused in ["Linked.app", "Through.app", "Empty.app", "Bare.app"] {
            assert_eq!(id(refused), None, "{refused}");
        }
        // A FIFO does not hold the reader.
        let fifo = root.join("Bare.app/Contents");
        std::fs::create_dir(&fifo)?;
        let status = std::process::Command::new("/usr/bin/mkfifo")
            .arg(fifo.join("Info.plist"))
            .status()?;
        assert!(status.success());
        assert_eq!(id("Bare.app"), None);
        Ok(())
    }

    fn info_plist(id: &str) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
             <key>CFBundleIdentifier</key><string>{id}</string>\
             <key>CFBundleName</key><string>Fake</string>\
             <key>CFBundleShortVersionString</key><string>1.2.3</string>\
             </dict></plist>"
        )
        .into_bytes()
    }

    #[test]
    fn reads_unsigned_bundles_one_level_deep_without_following_links()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            Node::File {
                path: "apps/Fake.app/Contents/Info.plist".into(),
                contents: info_plist("com.example.fake"),
            },
            Node::File {
                path: "apps/Vendor/Nested.app/Contents/Info.plist".into(),
                contents: info_plist("com.example.nested"),
            },
            Node::File {
                path: "apps/A/B/TooDeep.app/Contents/Info.plist".into(),
                contents: info_plist("com.example.toodeep"),
            },
            Node::Symlink {
                path: "apps/Linked.app".into(),
                target: "Fake.app".into(),
            },
        ])?;
        let inventory = AppInventory::with_roots(vec![fx.path("apps")?, fx.path("missing")?]);
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        let mut ids: Vec<_> = report.applications.iter().map(bundle_id).collect();
        ids.sort();
        assert_eq!(ids, ["com.example.fake", "com.example.nested"]);
        let fake = report
            .applications
            .iter()
            .find(|a| bundle_id(a) == "com.example.fake");
        assert_eq!(
            fake.and_then(|a| a.version.clone())
                .map(|v| v.raw().to_owned()),
            Some("1.2.3".to_owned())
        );
        assert_eq!(
            fake.and_then(|a| a.signature.clone()).map(|s| s.check),
            Some(SignatureCheck::Unsigned)
        );
        assert_eq!(
            fake.and_then(|a| a
                .signature
                .as_ref()
                .and_then(|s| s.verified_signer().cloned())),
            None
        );
        Ok(())
    }

    fn plist(body: &str) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>{body}</dict></plist>"
        )
        .into_bytes()
    }

    #[test]
    fn a_bundle_read_without_an_identifier_leaves_the_inventory_complete()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            Node::File {
                path: "apps/Shortcut.app/Contents/Info.plist".into(),
                contents: plist(
                    "<key>CFBundleExecutable</key><string>run.sh</string>\
                     <key>CFBundlePackageType</key><string>APPL</string>",
                ),
            },
            Node::File {
                path: "apps/Fake.app/Contents/Info.plist".into(),
                contents: info_plist("com.example.fake"),
            },
        ])?;
        let report = AppInventory::with_roots(vec![fx.path("apps")?]).collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        assert_eq!(
            report
                .applications
                .iter()
                .map(bundle_id)
                .collect::<Vec<_>>(),
            ["com.example.fake"]
        );
        Ok(())
    }

    #[test]
    fn unreadable_bundles_and_cancellation_make_the_inventory_partial()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            Node::Dir {
                path: "apps/Empty.app".into(),
            },
            Node::File {
                path: "names/Named.app/Contents/Info.plist".into(),
                contents: plist("<key>CFBundleName</key><string>Named</string>"),
            },
        ])?;
        let inventory = AppInventory::with_roots(vec![fx.path("apps")?]);
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Partial, "no property list");
        let named = AppInventory::with_roots(vec![fx.path("names")?]);
        assert_eq!(
            named.collect(&CancelToken::new()).status,
            SourceStatus::Partial,
            "too little to tell it has no identifier"
        );
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(inventory.collect(&cancel).status, SourceStatus::Partial);
        Ok(())
    }
}
