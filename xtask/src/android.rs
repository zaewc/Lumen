//! `cargo xtask android`: builds the mobile core for Android and checks it
//! (ADR-0007, research report 05 §10).
//!
//! For each ABI, `lumen-ffi` is built as a release `cdylib` and linked by the
//! NDK's clang for the minimum SDK, with 16 KB page alignment requested in
//! `.cargo/config.toml`. Every `LOAD` segment of the resulting `.so` must then
//! be aligned to at least 16 KB, as Google Play requires; the check reads the
//! program headers with the NDK's `llvm-readelf` instead of trusting the
//! defaults. Finally the Kotlin bindings are generated from the arm64 library
//! (`UniFFI` library mode) into `target/android/kotlin`, and every ABI's library
//! is laid out as `target/android/jniLibs/<abi>/liblumen_ffi.so`, which is
//! where the mobile app's `lumen-core` module picks both up (roadmap 11.14).
//!
//! The NDK is found through `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT` or
//! `ANDROID_NDK_LATEST_HOME`, or as the newest `ndk/<version>` below
//! `ANDROID_HOME`; r28 or newer is required (16 KB by default).

use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The oldest Android release the app supports (platforms.md: `minSdk 30`).
const MIN_SDK: u32 = 30;

/// The page size Google Play requires native libraries to support.
const PAGE: u64 = 16 * 1024;

/// The oldest NDK whose defaults align to 16 KB.
const MIN_NDK_MAJOR: u32 = 28;

/// `(Rust target, NDK clang prefix, Android ABI)`.
const ABIS: [(&str, &str, &str); 2] = [
    (
        "aarch64-linux-android",
        "aarch64-linux-android",
        "arm64-v8a",
    ),
    ("x86_64-linux-android", "x86_64-linux-android", "x86_64"),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The NDK's major version, from its `source.properties`.
fn ndk_major(ndk: &Path) -> Option<u32> {
    let text = fs::read_to_string(ndk.join("source.properties")).ok()?;
    let revision = text
        .lines()
        .find_map(|line| line.strip_prefix("Pkg.Revision"))?
        .trim_start_matches([' ', '='])
        .trim();
    revision.split('.').next()?.parse().ok()
}

fn find_ndk() -> Result<PathBuf, Box<dyn Error>> {
    let named = [
        "ANDROID_NDK_HOME",
        "ANDROID_NDK_ROOT",
        "ANDROID_NDK_LATEST_HOME",
    ]
    .iter()
    .find_map(|name| env::var_os(name).map(PathBuf::from));
    let ndk = if let Some(ndk) = named {
        ndk
    } else {
        let sdk = env::var_os("ANDROID_HOME")
            .or_else(|| env::var_os("ANDROID_SDK_ROOT"))
            .map(PathBuf::from)
            .ok_or("no NDK: set ANDROID_NDK_HOME or ANDROID_HOME")?;
        fs::read_dir(sdk.join("ndk"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| ndk_major(path).is_some())
            .max_by_key(|path| ndk_major(path))
            .ok_or("no NDK below $ANDROID_HOME/ndk")?
    };
    let major = ndk_major(&ndk).ok_or_else(|| format!("{} is not an NDK", ndk.display()))?;
    if major < MIN_NDK_MAJOR {
        return Err(format!("NDK r{major} found; r{MIN_NDK_MAJOR} or newer is required").into());
    }
    Ok(ndk)
}

/// The directory of the NDK's host tools.
fn tools(ndk: &Path) -> Result<PathBuf, Box<dyn Error>> {
    // The NDK ships x86_64 host tools for macOS on every Mac.
    let host = if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    };
    let bin = ndk.join("toolchains/llvm/prebuilt").join(host).join("bin");
    if bin.is_dir() {
        Ok(bin)
    } else {
        Err(format!("no NDK host tools at {}", bin.display()).into())
    }
}

fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed with {status}").into())
    }
}

/// The alignments of the `LOAD` segments in `library`.
fn load_alignments(readelf: &Path, library: &Path) -> Result<Vec<u64>, Box<dyn Error>> {
    let out = Command::new(readelf).arg("-lW").arg(library).output()?;
    if !out.status.success() {
        return Err(format!("llvm-readelf failed on {}", library.display()).into());
    }
    let text = String::from_utf8(out.stdout)?;
    let mut alignments = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("LOAD"))
    {
        let last = line.split_whitespace().last().ok_or("empty LOAD line")?;
        let hex = last.strip_prefix("0x").ok_or("LOAD alignment is not hex")?;
        alignments.push(u64::from_str_radix(hex, 16)?);
    }
    if alignments.is_empty() {
        return Err(format!("{} has no LOAD segments", library.display()).into());
    }
    Ok(alignments)
}

/// Builds and checks every ABI, then generates the Kotlin bindings. Returns
/// the number of problems found.
pub(crate) fn android() -> Result<usize, Box<dyn Error>> {
    let ndk = find_ndk()?;
    let bin = tools(&ndk)?;
    let suffix = if cfg!(windows) { ".cmd" } else { "" };
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let readelf = bin.join(format!("llvm-readelf{exe}"));
    let root = root();
    let jni_libs = root.join("target/android/jniLibs");
    crate::ios::clear(&root, &jni_libs)?;
    let mut problems = 0;
    for (target, prefix, abi) in ABIS {
        let clang = bin.join(format!("{prefix}{MIN_SDK}-clang{suffix}"));
        let upper = target.to_uppercase().replace('-', "_");
        run(Command::new(env!("CARGO"))
            .current_dir(&root)
            .args(["build", "--release", "--locked", "-p", "lumen-ffi", "--lib"])
            .args(["--target", target])
            .env(format!("CARGO_TARGET_{upper}_LINKER"), &clang)
            .env(format!("CC_{}", target.replace('-', "_")), &clang)
            .env(
                format!("AR_{}", target.replace('-', "_")),
                bin.join(format!("llvm-ar{exe}")),
            ))?;
        let library = root
            .join("target")
            .join(target)
            .join("release/liblumen_ffi.so");
        let alignments = load_alignments(&readelf, &library)?;
        if alignments.iter().all(|&align| align >= PAGE) {
            println!(
                "{abi}: {} LOAD segments aligned to >= 16 KB",
                alignments.len()
            );
        } else {
            eprintln!("{abi}: LOAD alignments {alignments:x?} are not all >= 0x4000");
            problems += 1;
        }
        // Into the folder just cleared, so nothing is overwritten.
        let abi_dir = jni_libs.join(abi);
        fs::create_dir_all(&abi_dir)?;
        fs::write(abi_dir.join("liblumen_ffi.so"), fs::read(&library)?)?;
    }
    let out_dir = root.join("target/android/kotlin");
    crate::ios::clear(&root, &out_dir)?;
    run(Command::new(env!("CARGO"))
        .current_dir(&root)
        .args([
            "run",
            "--locked",
            "-p",
            "lumen-ffi",
            "--features",
            "bindgen",
        ])
        .args(["--bin", "uniffi-bindgen", "--", "generate", "--library"])
        .arg(root.join("target/aarch64-linux-android/release/liblumen_ffi.so"))
        .args(["--language", "kotlin", "--no-format", "--out-dir"])
        .arg(&out_dir))?;
    let kotlin = out_dir.join("uniffi/lumen_ffi/lumen_ffi.kt");
    if fs::read_to_string(&kotlin).is_ok_and(|text| text.contains("fun `coreVersion`()")) {
        println!("Kotlin bindings: {}", kotlin.display());
    } else {
        eprintln!(
            "the Kotlin bindings at {} lack coreVersion()",
            kotlin.display()
        );
        problems += 1;
    }
    Ok(problems)
}
