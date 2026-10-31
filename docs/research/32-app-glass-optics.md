# Application glass optics — 2026-10-31

The owner's constraint is visual only: keep the existing top navigation, content
layout, control order and workflows. The composition adapts the skills' Lens
Inspector recipe to the existing workspace; no new scene or product UI is added.

- Apple's [Materials HIG](https://developer.apple.com/design/human-interface-guidelines/materials)
  places material on a functional control layer above content. Its [Landmarks example](https://developer.apple.com/documentation/swiftui/landmarks-building-an-app-with-liquid-glass)
  and [current design kit](https://developer.apple.com/news/?id=e2lxw9l1) informed visual review.
- The requested `liquid-glass-design` 0.3.2 inverse-lens implementation is adapted
  into strict TypeScript with its MIT notice. Scale comes from the field's measured
  range; no foreground text or arbitrary page snapshot is displaced.
- Chromium supports the SVG backdrop path. WebKit/Firefox retain explicit frosted
  material fallback, following the skill's engine gate; syntax support alone is
  insufficient. The original desktop CSP remains unchanged.
- Enlarged local line-detail captures exposed a map viewport mismatch. Setting
  `feImage` to the actual surface dimensions fixes the cropped field; the detail
  fixture is confined to a temporary QA tab and never ships in the product/assets.
- iOS 26+ uses native [UIGlassEffect](https://developer.apple.com/documentation/uikit/uiglasseffect).
  Availability checks preserve the existing minimum OS with semantic thin material.
- Geometry checks retain top navigation's original anchor, rather than applying
  the skill showcase's centered bottom-dock assumptions to a different layout.
