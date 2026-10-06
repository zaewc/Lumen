# ADR-0007: Build mobile with Expo, native Expo Modules, and the Rust core via UniFFI

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

The Android and iOS apps must be first-class clients, not shrunk desktop UIs. Their
value comes from heavy native APIs (PhotoKit, Vision, `StorageStatsManager`,
MediaStore, Storage Access Framework, background tasks) plus the shared Rust core for
evidence, policy and duplicate clustering.

## Decision drivers

- Native API access in Swift and Kotlin is unavoidable.
- Mature, production-grade Rust bindings.
- Reuse of TypeScript domain types, design tokens and UI skills from the web app.
- An escape hatch to fully native UI if the JavaScript layer becomes a liability.

## Considered options

1. Expo (development builds, Continuous Native Generation) with a React Native UI;
   platform work in local Expo Modules (Swift/Kotlin) that call the Rust core through
   UniFFI's Swift and Kotlin bindings.
2. Fully native SwiftUI + Jetpack Compose on the same Rust core and UniFFI bindings.
3. Direct JS ↔ Rust bindings (`uniffi-bindgen-react-native`).
4. Kotlin Multiplatform / Compose Multiplatform.
5. Flutter.
6. Tauri mobile.

## Decision outcome

Chosen option: **1**, gated by a performance spike, with **2** as the documented
fallback.

- One app at `apps/mobile` (Expo SDK pinned; upgrade quarterly; React Native New
  Architecture, which is the only architecture since 0.82).
- Local Expo Modules are the hexagonal platform adapters, e.g. `lumen-photos`,
  `lumen-files`, `lumen-bgtask`, `lumen-android-storage`.
- The Rust core ships as `LumenCore.xcframework` and Android `.so` files
  (`arm64-v8a`, `x86_64`; 16 KB page alignment verified in CI) with UniFFI bindings,
  consumed **inside** the Expo Modules.
- Boundary rule: JavaScript receives paged, read-only view models, progress events and
  command intents (`proposeQuarantine(ids)`). Scanning, hashing, feature prints and
  graph writes stay in native code and Rust. No per-file calls cross into JS in hot
  loops.
- Expo Go is not used (it cannot load custom native code).

**Decision gate (before mobile UI work):** a spike that enumerates 50k `PHAsset`s,
computes 5k Vision feature prints in a `BGContinuedProcessingTask`, clusters them in
Rust and renders a review grid. Pass criteria: ≥ 60 fps scrolling on a mid-range
iPhone and Android device and < 4 ms of JS-thread work per frame. On failure, switch to
option 2; the Swift/Kotlin adapters and Rust bindings carry over unchanged.

### Consequences

- Good: Swift/Kotlin adapters are needed anyway and are portable to a native UI.
- Good: UniFFI Swift/Kotlin bindings are production-proven (Firefox).
- Bad: three languages on mobile (TS, Swift/Kotlin, Rust) and a config plugin to wire
  Rust builds into EAS or local builds.
- Bad: React Native supports only three minors; upgrades are a recurring cost.

Rejected:

- Option 3: pre-production by its own documentation.
- Option 4: duplicates the Rust core's role; Swift export is alpha.
- Option 5: no TypeScript reuse and still needs platform channels.
- Option 6: a WebView UI is a poor fit for large media grids.

## More information

- [iOS and mobile framework research, Implication 8](../research/06-ios-and-mobile-framework.md)
- [Android research, Implication 7](../research/05-android-platform.md)
- ADR-0008 (per-platform scope)
