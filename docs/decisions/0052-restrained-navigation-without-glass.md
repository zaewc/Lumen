# ADR-0052: Restrained navigation and opaque surfaces, without glass

- Status: Accepted (owner decision, 2026-11-01)
- Date: 2026-11-01
- Supersedes in part: the navigation optics and the integrated and floating material
  layers of [app-materials.md](../design/app-materials.md) (#581, #592, #594)

## Context and problem statement

On 2026-10-31 the owner asked for a refracting lens over the selected navigation
place on every engine. Because `backdrop-filter: url()` renders only in Chromium,
the lens bends an `aria-hidden` copy of the navigation instead of the backdrop
(`NavigationGlass.tsx` and `glass-map.ts`, about 620 lines, plus their CSS), sits in a
pill-shaped track with a faint grain, and is joined by translucent, blurred headers,
dialogs and buttons.

On 2026-11-01 the owner's redesign brief asked for a visual language that does not
depend on glass ("if all blur and transparency disappeared, Lumen should still look
like Lumen"), restrained selection without large rounded selected shapes or oversized
tab containers, and content that is never blurred. A web UI audit the same day found
the lens and the material layers to be the most decorative parts of the interface,
and that `materials.css` redefines four surface colours with values that invert the
dark-mode hierarchy. Asked directly, the owner chose to remove the lens.

## Decision drivers

- Selection is information: it should be clear, quiet and the same on every engine.
- No meaning or hierarchy may depend on blur, transparency or refraction.
- One source of colour tokens, with metadata text at 4.5:1 or more on every surface.
- Less code to keep working across Chromium, WebKit and Gecko.

## Considered options

1. **Remove the lens and the glass layers; mark the current place with a 2 px
   cobalt rule under its text label.**
2. **Keep the lens as an enhancement over the rule.** Rejected by the owner: the
   selection would still be a large rounded shape, and two indicators would compete.
3. **Keep everything.** Rejected: it contradicts the brief.

## Decision outcome

Chosen option: **1**, specified in [visual-language.md](../design/visual-language.md).

- The navigation is four text tabs without icons, container, track or grain. The
  current place is `ink` with a 2 px cobalt rule under its label; the weight stays
  the same so labels do not shift.
- `NavigationGlass.tsx`, `glass-map.ts`, their test and the lens CSS are removed.
- Headers, dialogs, the inspector, the InfoTip panel, secondary buttons and
  segmented controls become opaque, with hairlines and at most one soft shadow for
  what sits above the page. Nothing is blurred.
- `materials.css` no longer defines colours; `styles.css` is the one token source.
- The mobile app keeps its native platform materials
  ([mobile-materials.md](../design/mobile-materials.md)); this decision covers the
  desktop app and the web dashboard.

### Consequences

- Good: the same appearance on every engine, with less code and no copy layer.
- Good: reduced-transparency and forced-colours modes need no special path.
- Bad: reverses work merged the day before; the research on non-Chromium
  `backdrop-filter` stays useful only as a record.

## More information

- Owner brief, 2026-11-01 (Lumen visual language).
- [Apple: adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass),
  the reference for the superseded material layers.
