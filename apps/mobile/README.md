# Lumen mobile

Expo SDK 58 app (ADR-0007). Native projects are generated (continuous native
generation); `ios/` and `android/` are not committed.

## Run on Android

```sh
cargo xtask android                       # the Rust core: .so per ABI + Kotlin bindings
cd apps/mobile
pnpm exec expo prebuild --platform android
pnpm android                              # build, install and start Metro
```

`modules/lumen-core` is the local Expo module around the Rust core. Its Gradle
build takes the libraries and bindings from `target/android`, so run
`cargo xtask android` again after changing `crates/lumen-ffi`.

## Run on iOS

```sh
cargo xtask ios                           # LumenCore.xcframework + Swift bindings into modules/lumen-core/ios/rust
cd apps/mobile
pnpm exec expo prebuild --platform ios    # also runs pod install
pnpm ios                                  # build, install on the simulator and start Metro
```

Run `cargo xtask ios` again after changing `crates/lumen-ffi`.
