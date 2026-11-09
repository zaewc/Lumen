//! Embeds `Info.plist` in the helper's `__TEXT,__info_plist` section on macOS,
//! so its code-signing identifier is `dev.lumen.desktop.helper` however it is
//! signed, rather than one derived from the file name (research 37).

fn main() {
    println!("cargo:rerun-if-changed=Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Info.plist");
        println!(
            "cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
            plist.display()
        );
    }
}
