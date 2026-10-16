//! `cargo xtask ios`: builds the mobile core for iOS and checks it (ADR-0007).
//!
//! `lumen-ffi` is built as a release static library for devices
//! (`aarch64-apple-ios`) and the Apple-silicon simulator
//! (`aarch64-apple-ios-sim`), for the app's minimum iOS version. The Swift
//! bindings are generated from a host build (`UniFFI` library mode; the
//! metadata is the same on every target), and both slices are packaged with
//! the C header and module map as `target/ios/LumenCore.xcframework`.
//!
//! The bindings are then proven, not just generated: they are compiled with
//! the host static library into a small program that calls `coreVersion()`
//! and must print the crate version.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The app's minimum iOS version (Expo SDK 56+; research report 06).
const MIN_IOS: &str = "16.4";

/// `(Rust target, XCFramework slice)`.
const TARGETS: [&str; 2] = ["aarch64-apple-ios", "aarch64-apple-ios-sim"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed with {status}").into())
    }
}

fn cargo_build(root: &Path, target: Option<&str>) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(root)
        .args(["build", "--release", "--locked", "-p", "lumen-ffi", "--lib"])
        .env("IPHONEOS_DEPLOYMENT_TARGET", MIN_IOS);
    if let Some(target) = target {
        command.args(["--target", target]);
    }
    run(&mut command)
}

/// Removes a previous build output, which must lie under `target/`.
#[allow(
    clippy::disallowed_methods,
    reason = "regenerated build output under target/, never user files"
)]
fn clear(root: &Path, dir: &Path) -> Result<(), Box<dyn Error>> {
    if !dir.starts_with(root.join("target")) {
        return Err(format!("refusing to clear {} outside target/", dir.display()).into());
    }
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// Generates the Swift bindings into `out/swift` and the C header and module
/// map into `out/headers`.
fn bindings(root: &Path, out: &Path) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let bindings = out.join("swift");
    clear(root, &bindings)?;
    run(Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "run",
            "--locked",
            "-p",
            "lumen-ffi",
            "--features",
            "bindgen",
        ])
        .args(["--bin", "uniffi-bindgen", "--", "generate", "--library"])
        .arg(root.join("target/release/liblumen_ffi.dylib"))
        .args(["--language", "swift", "--no-format", "--out-dir"])
        .arg(&bindings))?;
    let headers = out.join("headers");
    clear(root, &headers)?;
    fs::create_dir_all(&headers)?;
    // Into the directory just cleared, so nothing is overwritten.
    for (from, to) in [
        ("lumen_ffiFFI.h", "lumen_ffiFFI.h"),
        ("lumen_ffiFFI.modulemap", "module.modulemap"),
    ] {
        fs::write(headers.join(to), fs::read(bindings.join(from))?)?;
    }
    Ok((bindings, headers))
}

/// Packages the device and simulator libraries as `LumenCore.xcframework`.
/// Returns the number of problems found.
fn package(root: &Path, out: &Path, headers: &Path) -> Result<usize, Box<dyn Error>> {
    let framework = out.join("LumenCore.xcframework");
    clear(root, &framework)?;
    let mut create = Command::new("xcodebuild");
    create.arg("-create-xcframework");
    for target in TARGETS {
        create
            .arg("-library")
            .arg(
                root.join("target")
                    .join(target)
                    .join("release/liblumen_ffi.a"),
            )
            .arg("-headers")
            .arg(headers);
    }
    run(create.arg("-output").arg(&framework))?;
    let slices = fs::read_dir(&framework)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count();
    if slices == TARGETS.len() {
        println!("{}: {slices} slices", framework.display());
        Ok(0)
    } else {
        eprintln!(
            "{} has {slices} slices, not {}",
            framework.display(),
            TARGETS.len()
        );
        Ok(1)
    }
}

/// Calls the core through the Swift bindings on the host. Returns the number
/// of problems found.
fn prove(
    root: &Path,
    out: &Path,
    bindings: &Path,
    headers: &Path,
) -> Result<usize, Box<dyn Error>> {
    let probe = out.join("probe");
    fs::create_dir_all(&probe)?;
    let main = probe.join("main.swift");
    fs::write(&main, "print(coreVersion())\n")?;
    let program = probe.join("core-version");
    run(Command::new("xcrun")
        .args(["swiftc", "-O"])
        .arg(&main)
        .arg(bindings.join("lumen_ffi.swift"))
        .arg("-I")
        .arg(headers)
        .arg("-Xcc")
        .arg(format!(
            "-fmodule-map-file={}",
            headers.join("module.modulemap").display()
        ))
        .arg(root.join("target/release/liblumen_ffi.a"))
        .arg("-o")
        .arg(&program))?;
    let output = Command::new(&program).output()?;
    let printed = String::from_utf8_lossy(&output.stdout);
    let expected = env!("CARGO_PKG_VERSION");
    if output.status.success() && printed.trim() == expected {
        println!("Swift bindings: coreVersion() = {expected}");
        Ok(0)
    } else {
        eprintln!("the Swift probe printed {printed:?}, expected {expected:?}");
        Ok(1)
    }
}

/// Builds the slices and the `XCFramework`, generates and proves the Swift
/// bindings. Returns the number of problems found.
pub(crate) fn ios() -> Result<usize, Box<dyn Error>> {
    if !cfg!(target_os = "macos") {
        return Err("the iOS build needs macOS and Xcode".into());
    }
    let root = root();
    let out = root.join("target/ios");
    for target in TARGETS {
        cargo_build(&root, Some(target))?;
    }
    cargo_build(&root, None)?;
    let (bindings, headers) = bindings(&root, &out)?;
    Ok(package(&root, &out, &headers)? + prove(&root, &out, &bindings, &headers)?)
}
