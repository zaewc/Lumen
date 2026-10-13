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
use objc2_core_foundation::{CFBundle, CFString, CFURL};

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

/// Reads one bundle. `None` if it has no bundle identifier (not a real app).
fn application(path: &Path) -> Option<Application> {
    let url = CFURL::from_directory_path(path)?;
    let bundle = CFBundle::new(None, Some(&url))?;
    let id = bundle.identifier()?.to_string();
    let key = |name: &str| bundle.value_for_info_dictionary_key(Some(&CFString::from_str(name)));
    let name = text(key("CFBundleDisplayName")).or_else(|| text(key("CFBundleName")));
    let location = RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()).ok()?;
    Some(Application {
        ids: BTreeSet::from([AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new(id),
        }]),
        name,
        version: text(key("CFBundleShortVersionString")),
        vendor: None,
        signature: Some(signing::check(&url)),
        install_locations: vec![location],
        sources: BTreeSet::from([source()]),
    })
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
                Some(app) => applications.push(app),
                // A bundle that cannot be read is not proof that nothing is there.
                None => complete = false,
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

/// Code-signature checks through the Security framework: the only `unsafe` code
/// in this module.
#[allow(unsafe_code)]
mod signing {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use lumen_domain::{CodeSignature, SignatureCheck, UntrustedText};
    use objc2_core_foundation::{CFDictionary, CFRetained, CFString, CFURL};
    use objc2_security::{
        SecCSFlags, SecCode, SecStaticCode, errSecCSUnsigned, kSecCSBasicValidateOnly,
        kSecCSSigningInformation, kSecCodeInfoTeamIdentifier,
    };

    /// Verifies the bundle's signature and certificate chain (not its contents)
    /// and, if valid, reads the signing team.
    pub(super) fn check(url: &CFURL) -> CodeSignature {
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
        // SAFETY: invariant: the key is a valid CFString and the dictionary is
        // alive; the returned value is borrowed from it. Why unsafe: untyped
        // dictionary access. Why it holds: `kSecCodeInfoTeamIdentifier` is a
        // static CF constant, and the value is type-checked before use below.
        let value = unsafe {
            let key: *const CFString = kSecCodeInfoTeamIdentifier;
            info.value(key.cast::<c_void>())
        };
        // SAFETY: invariant: a non-null value is a live CF object owned by `info`.
        // Why unsafe: turning a raw pointer into a reference. Why it holds: `info`
        // outlives this borrow, and `downcast_ref` checks the CF type.
        let value = unsafe { value.cast::<objc2_core_foundation::CFType>().as_ref() }?;
        let team = value.downcast_ref::<CFString>()?.to_string();
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

    #[test]
    fn unreadable_bundles_and_cancellation_make_the_inventory_partial()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[Node::Dir {
            path: "apps/Empty.app".into(),
        }])?;
        let inventory = AppInventory::with_roots(vec![fx.path("apps")?]);
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Partial, "no bundle identifier");
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(inventory.collect(&cancel).status, SourceStatus::Partial);
        Ok(())
    }
}
