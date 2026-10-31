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
- `backdrop-filter: url()` renders only in Chromium, so the navigation lens now
  bends an aria-hidden copy of the list with `filter: url()`, which Chromium,
  WebKit (WKWebView, WebKitGTK, Safari, iOS) and Gecko render. The original
  desktop CSP remains unchanged: the map is a `data:` image, styles are set through
  the DOM. See "Cross-engine lens" below.
- Enlarged local line-detail captures exposed a map viewport mismatch. Setting
  `feImage` to the actual surface dimensions fixes the cropped field; the detail
  fixture is confined to a temporary QA tab and never ships in the product/assets.
- iOS 26+ uses native [UIGlassEffect](https://developer.apple.com/documentation/uikit/uiglasseffect).
  Availability checks preserve the existing minimum OS with semantic thin material.
- Geometry checks retain top navigation's original anchor, rather than applying
  the skill showcase's centered bottom-dock assumptions to a different layout.

## Cross-engine lens — 2026-10-31

The copy-layer approach and the engine rules follow the owner's
`liquid-glass-web` skill notes (`references/patterns.md`, segmented control /
tab indicator; `references/browser-quirks.md`); its engine code has no license we
could confirm, so none of it is copied. The implementation extends the MIT-noticed
`glass-map.ts` from those documented measurements.

- **Units.** `primitiveUnits="objectBoundingBox"`; WebKit renders an element whose
  filter uses `userSpaceOnUse` primitives blank. `color-interpolation-filters="sRGB"`
  keeps 128 neutral.
- **Scale.** With bounding-box units, `feDisplacementMap` `scale` converts per
  engine: Blink by the width on both axes, WebKit by the width (x) and height (y),
  Gecko by √((w² + h²)/2). For a bend of `px`: Blink `2px/w`; Gecko
  `2px/√((w²+h²)/2)`; WebKit `2px/min(w,h)` with R and G first compressed towards
  0.5 by `min/w` and `min/h` (`feColorMatrix`). A minimal WKWebView page on macOS
  26.6 reproduced the WebKit figures. `bendScale` and its unit tests encode them.
- **Bounding box.** WebKit sized the bend by the filtered element's overflowing
  content, not its border box: a lens-sized box holding the 430 px list bent about
  4.5x too far. The copy therefore covers exactly the list, with no overflow, and
  the lens is a filter sub-region plus `clip-path`.
- **Caching.** WebKit caches filter output per id. The map is decoded with
  `Image.decode()` before `feImage` uses it; every move or change issues a new id,
  again about 60 ms and 250 ms later. The `<svg>` is never `display: none`, and the
  copy stays unfiltered and hidden until its first map is ready.
- **Fold limit.** With the original 0.58 spread and 110% of the measured scale the
  rim folds: it repeated the icon's stroke as a detached, colour-fringed arc. The
  map now measures its fold-free bend (the steepest inward fall of the encoded
  field); the lens uses a linear encoding and at most half that bend (≤ 2x local
  magnification, ≤ 5 px), which bends the icon and label ends without smearing.
- **Verification.** Chrome 156, the system WebKit through WKWebView and Firefox 159
  (WebDriver BiDi, 2x), light and dark, English and Korean, 1440 and 390 px wide,
  before and after navigating, mid-slide, and with the filter removed for a pixel
  diff. Real iPhone and Windows WebView2 hardware were not checked.
