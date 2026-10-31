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
