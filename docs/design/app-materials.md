# App materials

The owner's 2026-10-31 brief changes visual materials only, preserving navigation,
placement, dimensions, copy, workflows and confirmation contracts. It applies to
the desktop/dashboard application and mobile; the Astro download site is excluded.

The shared application imports `apps/web/src/app/materials.css` after its base
styles. Tauri retains its existing build, CSP and IPC. The Astro site imports none
of these styles.

| Layer | Treatment |
| --- | --- |
| Content | Opaque cool neutral canvas, crisp existing text and semantic colours |
| Integrated chrome | Restrained tinted material on navigation and sticky toolbars |
| Floating controls | Slight edge light and short shadow on secondary actions and selected segments |
| Overlays | Higher opacity and separation on existing confirmations and inspectors |

Material tokens centralize tint, opacity, blur, saturation, edge illumination,
shadow, corners and interaction timing. Glass is nearly colourless; its environment
supplies colour. Cobalt is reserved for primary actions and content states. Primary and
irreversible actions keep their solid accessible fills. Content is never blurred.
Unsupported backdrop filtering uses opaque surfaces. Reduced transparency and
increased contrast remove transparency; reduced motion keeps immediate feedback.

This follows Apple's [guidance on adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass):
use material for navigation and controls, preserving legibility and accessibility.


## Visual contract

- Surface: recurring-use storage workspace and native mobile photo review.
- User job: inspect storage and evidence, then make a safe, explicit decision.
- Reference family: Apple's current Materials HIG and Liquid Glass navigation;
  the user's visual-only brief takes priority over template composition recipes.
- Design source: repository UI/design docs, the 2026-10-31 owner brief,
  `ui-anti-slop-codex`, then `liquid-glass-design` production techniques.
- Density and layout: existing spacing, information architecture, control positions,
  view sizes and navigation order; no added product controls or decorative scene.
- CRUD create mode: none added; existing confirmation dialogs and evidence drawer
  retain their roles and focus/escape behavior.
- Required states: light/dark, initial/empty/dense content, selection, hover, press,
  keyboard focus, disabled, popovers/dialogs, reduced motion/transparency/contrast.
- Forbidden tells: blue navigation underline, blue selected cards, visible selection
  borders, rectangular selection blocks, glow, glass content cards or blurred text.
- Verification: actual app screenshots, native rendering, geometry and contrast
  checks, Apple reference comparison and a separate polish pass before recapture.

## Navigation optics

One lens marks the selected place and refracts in every engine: Chromium (WebView2,
Chrome, Edge), WebKit (the macOS and Linux desktop webviews, Safari, every iOS
browser) and Gecko (Firefox). `backdrop-filter: url()` renders only in Chromium,
so the lens bends a copy instead of the backdrop:

- **Copy layer.** Above the real list, an `aria-hidden`, `inert`, non-interactive
  copy of the same items, rendered from the same React data with the same layout
  classes and drawn as selected on the lens material. `filter: url()` bends it and
  `clip-path` shows it only inside the lens. The real links underneath stay the
  focusable, announced, clickable elements; the copy adds no names, roles or stops.
  This replaces "no copied UI" for this one indicator: WebKit and Gecko cannot
  refract the backdrop, and the copy is decoration.
- **Map.** The thin inverse-lens field at 2x, cached by shape (eight entries);
  moving the lens changes only the filter sub-region and the clip, never the map.
  Blue carries a rim light, brightest towards the upper left.
- **Strength.** The rim bends at most 5 px and never more than half the field's
  measured fold limit, so the rim magnifies the icon and label ends at most 2x and
  never repeats them; labels stay legible. A slight colour fringe stays at the rim.
- **Track grain.** The track carries faint 45° hairlines in an 8 px tile, drawn
  from the navigation's corner in both the track and the copy, so they meet exactly
  at the lens edge and visibly bend inside it. The track stays opaque: the lens
  bends the copy, not what lies behind the window, so translucency would add
  nothing to bend and cost legibility.
- **Per engine.** Filter units, the displacement scale per engine, map decoding and
  a new filter id on every change follow the measured rules in
  [the compatibility findings](../research/32-app-glass-optics.md).

Pointer light changes the lens position's light only; it never regenerates maps.
Before the first map is ready, without JavaScript, in an unknown engine or with the
`data-lg-force-fallback` QA hook, the lens is the material pill under the real
labels. Reduced transparency, increased contrast and forced colours use an opaque
surface with no filter and no grain; reduced motion moves the lens without a slide.
