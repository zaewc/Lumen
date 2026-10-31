# Mobile materials

The 2026-10-31 visual brief applies to the existing photo review app. It does not
add the desktop workflow to mobile or change library access, grouping, controls,
touch targets, safe areas or the measurement panel.

`theme.ts` centralizes light/dark colours, edge light, control geometry and short
shadows. The grid and empty/error content remain opaque. `Material.tsx` treats
only the existing header and bottom command bar as integrated chrome. Selected
segments get a nearly colourless material; the primary action stays solid with white text.
Controls respond to pressing without dimming their text or running animations.

On iOS 26+, a small local Expo view uses UIKit's native `UIGlassEffect(.regular)`
at full alpha, allowing the system to adapt it to appearance and accessibility.
Older supported iOS versions use the semantic `systemThinMaterial` fallback.
React text and controls remain siblings above the native background. On Android,
the same chrome uses an opaque cool surface with edge illumination, avoiding
expensive scrolling blur and additional dependencies. No effect is attached to
photo cells, and the existing frame-time probe remains available.

The native adapter follows Expo's [native view API](https://docs.expo.dev/modules/native-view-tutorial/)
and Apple's [UIVisualEffectView composition guidance](https://developer.apple.com/documentation/uikit/uivisualeffectview).
Availability checks preserve the existing iOS minimum version without adding a
blur package. Rebuild the native app after
adding the local module; Expo Go cannot load Lumen's local native modules.
