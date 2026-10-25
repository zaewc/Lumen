# Mobile performance spike: protocol and results

> Decision gate of [ADR-0007](../decisions/0007-mobile-expo-native-modules-uniffi.md)
> (roadmap 11.16). Built in 11.16a–c; measured by the owner on physical devices.

## What decides

ADR-0007 keeps the mobile UI in React Native (Expo) only if a review grid of a large
photo library, fed by native enumeration and Rust grouping, is smooth on **mid-range**
devices:

| Criterion | Pass |
| --- | --- |
| Scrolling the grid at 50,000 items | ≥ 60 fps: on Android, < 5 % janky frames and a 95th-percentile frame time ≤ 16.7 ms at 60 Hz; on iOS, a hitch-time ratio < 5 ms/s |
| JavaScript work per frame while scrolling | < 4 ms (Hermes profile), with the on-screen probe showing p95 ≤ 20 ms and ≤ 1 % dropped frames |
| Analysis of 5,000 photos | completes; on iOS it continues in the background with a Live Activity and can be cancelled |

If either device fails the first two rows, the fallback is native screens (SwiftUI and
Jetpack Compose) on the same Rust core and native modules.

## Devices

Use one **mid-range** phone per platform, not the newest flagship: for example an iPhone
two or more generations old (iPhone 13 or SE 3rd generation) and an Android phone in the
Pixel "a" or Galaxy A class. Set adaptive refresh to **60 Hz** where the phone allows it,
or note the rate in the results.

## Build (release, on the device)

Debug builds run JavaScript without optimizations and inflate every number, so measure
release builds only.

```sh
cargo xtask android && cargo xtask ios     # the Rust core and bindings
cd apps/mobile
pnpm exec expo prebuild
pnpm exec expo run:android --variant release --device     # Android
pnpm exec expo run:ios --configuration Release --device   # iOS (needs a signing team)
```

## Steps

1. Open the app and allow photo access; the library opens in the grid. Tap **Stats**
   (top right) to show the measurement panel: the probe, the analysis timings and the
   core's version. Hiding the panel stops the probe.
2. **Scroll test:** in the panel, turn on **Repeat to 50,000**, then fling up and down
   continuously for 30 s.
   - On-screen probe: note JS frames/s, p50, p95 and dropped %.
   - Android: run `adb shell dumpsys gfxinfo dev.lumen.mobile reset` just before the
     30 s, and `adb shell dumpsys gfxinfo dev.lumen.mobile` right after; note "Janky
     frames" and the 50th/90th/95th/99th percentiles.
   - iOS: record the 30 s in Xcode Instruments with the **Animation Hitches** template;
     note the hitch-time ratio.
3. **JS per frame:** in React Native DevTools (Performance panel), record 10 s of
   scrolling; JS time per frame = total JS thread time ÷ frames.
4. **Analysis test:** tap **Find similar photos** (the newest 5,000) and note analyzed,
   groups, prints ms and grouping ms from the panel. On iOS 26+, press Home right after
   starting: the Live Activity should show progress and allow cancelling. When groups
   are found the grid switches to **Similar**; scroll it.
5. Note the phone's temperature and battery change, and anything that stuttered.

## Results

| Device | OS | Hz | Library | UI: janky % or hitch ms/s | UI p95 ms | Probe p95 ms | Dropped % | JS ms/frame | Prints ms (n) | Grouping ms | Groups | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| (iPhone) | | | | | | | | | | | | |
| (Android) | | | | | | | | | | | | |

Reference only, not a measurement: the Android emulator (software rendering) grouped 48
test images into the 12 planted groups (prints 163 ms, grouping 4 ms), and
`lumen-cluster` groups 5,000 × 768 vectors in about 33 ms on an M-series Mac.

## Decision

To be filled in from the results: React Native kept, or native screens per ADR-0007.
