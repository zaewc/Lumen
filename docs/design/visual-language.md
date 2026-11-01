# Lumen visual language

The owner's 2026-11-01 brief asks for a visual language of Lumen's own: **Lumen
reveals what is already there.** It is an instrument for understanding storage, not
a cleaner. This document is the specification that the redesign PRs implement. It
covers the desktop app and the web dashboard; the mobile app keeps its platform
materials. It refines [storage-identity.md](storage-identity.md), whose composition,
data and interaction rules stay in force, and replaces the navigation and surface
parts of [app-materials.md](app-materials.md)
([ADR-0052](../decisions/0052-restrained-navigation-without-glass.md)).

The redesign changes how things look and move. It does not change the information
architecture, the four places, any workflow, the policy, the transport or any
confirmation. Where a concept needs data the interface does not receive, this
document says so and draws only what exists.

## Principles

1. **Space is the subject.** The largest visual element on a screen is storage
   itself, drawn to scale, never a decorative chart or a hero number.
2. **Certainty is visible.** Every fact is drawn as resolved, partial or unknown.
   Unknown is never blank, zero or free space ([ADR-0008](../decisions/0008-capability-model-and-coverage.md)).
3. **Attention, not spotlight.** Selection raises what is related and lowers the
   rest a step. Nothing glows, blurs or floats.
4. **Decisions are conclusions.** A verdict is drawn below the evidence it follows
   from, like a sum under its lines.
5. **Lumen without glass is still Lumen.** No meaning or hierarchy depends on blur,
   transparency or refraction.

## Foundations

### Colour

Restrained cool neutrals and one information colour. All values are tokens in
`app/styles.css`; `materials.css` stops redefining them (it currently overrides
four surfaces with different values and inverts the dark hierarchy).

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `canvas` | #f8f9fb | #121821 | Window and page |
| `plane` | #eff2f6 | #1a2230 | Inset areas: the map field, code, the header |
| `raised` | #ffffff | #212b3a | Dialogs and the inspector only |
| `line` / `line-strong` | #e1e6ee / #c3ccd9 | white 9% / 18% | Hairlines, outlines |
| `ink` | #182232 | #ecf1f8 | Primary text, settled marks |
| `ink-2` | #46546a | #b4c1d2 | Secondary text |
| `ink-3` | #5a6880 | #94a5bb | Metadata (≥ 5:1 on every surface; today's light ink-3 is 4.45:1) |
| `accent` (cobalt) | #3a52cc | #8fa2ff (marks), #5268d1 (fills) | Information |
| `attention` | amber-700 | amber-300 | Incomplete coverage, needs attention |
| `irreversible` | red-700 | red-400 | Only "Remove for good" and its confirmation |

**Cobalt means "this is what you are looking at" or "Lumen proposes this".** It
marks the selection, the inspected item and its relations, quarantine proposals and
held items, the primary action and focus. It never fills an area larger than a
control, never tints a row background, and is never decoration. Success uses ink
and a check mark, not green.

### Type

Platform fonts only ([ui-principles.md](ui-principles.md)). Every figure uses
`tabular-nums`, right-aligned in columns. A unit is set at 0.8 em in `ink-2` after
its number (`4.2` `GB`), so columns of sizes read as a scale.

| Role | Size / weight | Notes |
| --- | --- | --- |
| Measure | 32 / 600, tracking -0.03em | One per screen: available space, the inspected size |
| Figure | 20 / 500, tracking -0.02em | Totals beside a label: per status, progress counts |
| Page title | 22 / 600 | Replaces 28 px; the page header shrinks from 96 to 64 px |
| Section | 15 / 600 | List sections, step headings (replaces 24 px) |
| Body | 14 / 400 | |
| Secondary | 13 / 400 | |
| Metadata | 12 / 400 | Never below 12 px; 11 px mono goes to 12 px |
| Path, command | 12 / mono | Selectable |

### Space, shape, elevation

- 4 px unit. Rows 36 px (list) and 44 px (settings). Content max width stays 1248 px.
- Radius: 4 px for controls and map regions, 8 px for dialogs. No pills, no 999 px.
- Separation by hairlines and alignment. Elevation only for things that sit above
  the page (dialogs, the inspector, the InfoTip panel): one 1 px `line` outline and
  one soft shadow, no inset highlights, no blur. Sticky headers are opaque `canvas`
  with a hairline that appears once content scrolls under them.
- No cards, no card inside a card, no icon boxes, no badges. The inline confirmation
  in a list row or the inspector is a hairline-bounded row, not a red box.

### Marks

The marks in `DecisionMark` stay and gain two states. Each mark always appears with
its word.

| State | Mark (9 px) | Region texture (map, bars) | Meaning |
| --- | --- | --- | --- |
| KEEP | solid square, `ink-3` | solid `ink` at low contrast | settled, stays |
| REVIEW | dashed open circle, `ink-2` | 135° hatch | unresolved; a person can judge |
| Own command | chevron, `ink` | dotted edge | the tool removes it |
| QUARANTINE (proposed) | open square, cobalt | cobalt outline, cobalt 8% fill | deliberate, reversible |
| Held | square with inner square, cobalt | — | moved, restorable |
| Needs attention | open square, dashed `attention` | — | interrupted; see Quarantine |
| Not read | — | open, dashed outline, no fill | coverage gap; never free space |

QUARANTINE is deliberate, not dangerous: it is never red. Red belongs only to final
removal.

### Certainty

Every fact the interface shows has one of three states, drawn the same way
everywhere (list, map, inspector, Overview).

| State | Drawing | Examples from the data |
| --- | --- | --- |
| Resolved | ink text, solid 6 px dot | exact sizes, a known origin, a matched rule |
| Partial | ink-2 text, half-filled dot, `≈` before figures | `reclaimable_exact: false`, `scan.partial`, `protect.in-use-partial`, `protect.incomplete-scan`, AI kind at low or medium confidence |
| Unknown | ink-3 text, open dot, the word "Unknown" or "Not read" | `coverage_complete: false`, `denied`/`failed`/`not_scanned` counts, `source: null`, `protect.in-use-unknown`, `protect.not-observed`, `classify.devkb-unknown`, `default.review`, `ai_note.kind: unknown`, held items with `original` or `reclaim_bytes` null |

A pure function in `entities/scan-session` maps rule IDs and fields to a certainty
state, with tests over every rule in `shared/i18n/rules.ts`. A rule not in the map
is Unknown.

### Icons

20 px grid, 1.5 stroke, round caps, `currentColor`. Icons appear only where they
carry meaning: state marks, close, disclosure chevron, check, warning, copy. The
navigation is text only. The sparkle (AI), shield and gauge are removed, as are the
nine unused icons. AI review is labelled with words ("AI review"), never a symbol.

### Motion

Motion explains a change of state and lasts 120–220 ms, `cubic-bezier(.2,.7,.3,1)`.

- The map zooms between levels by interpolating region rectangles (220 ms).
- Attention shifts by fading unrelated regions and rows to their lowered step (160 ms).
- Regions resolve from the hatch of "not yet decided" to their verdict texture when
  results arrive (one 220 ms crossfade, not per item).
- The inspector slides 12 px and fades (160 ms).

No pointer-following light, springs, parallax or looping decoration except the
indeterminate progress line. `prefers-reduced-motion` makes every change instant.

## Signature concepts and where they belong

### Storage topography: Explore, and the disk on Storage

A squarified treemap of the assessment, the largest element on Explore above the
list. Area is `reclaimable_bytes`. Levels are the ones the data really has:

1. **All assessed items**, grouped by kind (`developer`, `app_cache`, `app_data`,
   `other`) and, as today, own-command items first.
2. **Developer tools** by `group.source` (Xcode, pnpm, …).
3. **Items**, each region textured by its verdict.

Regions below 0.5 % of the frame join one "N smaller items" region per group, so the
map stays legible with thousands of candidates. Labels appear only where they fit
(name and size, tabular). A coverage gap is drawn as a separate "Not read" region
with dashed edges and the count of unread folders, sized by a fixed small share
because its true size is unknown; it is never scaled as if measured.

The map is not a folder tree. The interface has no directory sizes, and a map of
path segments would invent them. Nested candidates' sizes are already excluded from
their parents, so areas never double-count.

On Storage, the 64-bar capacity field becomes a **disk strip**: one 12 px band, used
space filled in `line-strong` (low contrast, and apart from the solid `ink` of kept
items below it), free space as `canvas` with an outline, and a
cobalt bracket under the share that the last assessment covers ("Assessed: 18.4 GB").
The bracket is the entrance to the map: it links to Explore, and the map opens at
the same proportions. Disk figures and candidate figures stay separate quantities.

### Spatially continuous drill-down: Explore

Selecting a group region zooms the map into it (all → kind → tool) and narrows the
list below to that section; a breadcrumb above the map ("All assessed / Developer /
Xcode") steps back out. Selecting an item region opens the inspector, like selecting
its row. The DecisionMap stays as the map's legend and keeps its role as the status
filter; the map applies the same filter by lowering regions outside it. Zoom is view
state only: no new filter parameter, no new data.

### Evidence lens: Explore while inspecting

When an item is inspected, the map and list lower everything except the item and the
items related to it **by data the interface has**:

- the same tool (`group.source` or `tool_command.tool`);
- the same processes in `free_when_closed`;
- the same kind;
- the relations the evidence API exposes (step 6 of the plan), by their kind.

Related regions get a 1 px cobalt outline outside their edge (a proposal already has a
cobalt edge inside), the inspected one 2 px inside. Items related only by kind stay
as they are, without an outline, so the close relations stand out. The inspector
names each relation in words ("1 other item from Xcode · 3.1 GB"). Only relations
the API provides are drawn; none is inferred from paths or names.

### Inspection light: the inspector

The inspector stays a modal sheet with the same focus, Escape and selection
behaviour. Its backdrop stops blurring and dimming: the page stays at full
legibility, lowered one step except for the inspected row and its map region. The
inspector panel is opaque `raised` with a hairline edge.

### Evidence stack: the inspector

The inspector reads top to bottom as an argument:

1. **What it is:** name, kind, the measure (size; `≈` if not exact), location (mono).
2. **Evidence lines:** each reason, time and relation the API provides as one line
   with its certainty dot and its source in small caps: RULE, LOCATION, TIME,
   RELATION, PROCESS, AI REVIEW. A time the platform does not record reliably is
   drawn Partial or Unknown, never as a date it cannot vouch for. AI review shows its kind and
   confidence as words, never as a score or percentage.
3. **A rule** across the stack, like a sum line.
4. **The decision:** mark, word, and what it means for the person ("Lumen proposes
   moving this to Quarantine. You can restore it from there.").
5. **What can be done:** the tool command, or "Quit Discord to free", as today.

Rule counts are not confidence: the stack never shows "3 of 4" or a meter.

### Understanding resolving: empty, analysing, complete

- **Empty (no scan):** the map frame is a single "Not read yet" region, dashed, with
  the scan action beside it.
- **Scanning:** the frame fills from the left in hatch as `allocated_bytes` grows,
  against the disk's used space on the home volume, labelled "Read so far", never as
  a percentage of the scan. Counts stay tabular and update in place.
- **Deciding:** stage name and, where the host gives a total, that stage's progress
  as a 2 px line. Regions are not drawn until decisions exist.
- **Complete:** regions appear in their verdict textures in one crossfade; coverage
  gaps stay visible.

## Navigation and places

- **Navigation:** four text tabs in a 56 px header at every width. The current place
  is `ink` with a 2 px cobalt rule under its label; others are `ink-2`, at the same
  weight so labels do not shift. Hover
  shows the rule in `line-strong`. No container, track, pill, lens or icons. The
  right rail (activity, AI review status, update) keeps its content, as text.
- **Storage:** disk strip with the measure (available space), the last assessment's
  DecisionMap, the Quarantine row.
- **Explore:** unchanged steps; the results step gains the map above the list. The
  DecisionMap under the map keeps its totals and filter but not its bar, and list rows
  show sizes without bars: both would repeat the map.
- **Quarantine:** the same table; rows show the Held or Needs attention mark and the
  time held as text. Restore is a quiet button; "Remove for good…" is quiet `ink-2`
  text that turns red only in its confirmation row.
- **Settings, first launch, dialogs:** simple. Opaque surfaces, the new type scale,
  the segmented control as text with a cobalt rule, the switch unchanged.

## Not doing

Gradients, glow, glass, blur, noise or grain, pills, cards, KPI tiles, hero numbers,
sparkles or AI stars, shields, colour-coded danger for Quarantine, decorative charts,
confidence meters, invented relations or folder sizes.

## Implementation plan

Each step is one PR under about 400 changed lines, verified in the rendered app
(light and dark, English and Korean, 1120×800 and 420×560) with
`scripts/capture-app-screens.mjs`, whose selectors each PR keeps working.

1. This document, [ADR-0052](../decisions/0052-restrained-navigation-without-glass.md)
   and index updates.
2. **Tokens:** one colour source, type scale, radius, contrast fix; remove blur and
   inset highlights from content surfaces; opaque sticky headers.
3. **Navigation:** text tabs and the cobalt rule; remove the lens and its code.
4. **Marks and certainty:** the certainty map with tests, the two new marks, icon
   cleanup.
5. **Topography model:** squarified layout and grouping as pure, tested functions.
6. **Evidence API** (its own ADR, Rust and generated client, no interface): expose
   the times, facts and relations the core already computed for each decision, each
   with its certainty. Runs in parallel with steps 2–5.
7. **Topography on Explore:** the map, legend link to DecisionMap, zoom and breadcrumb.
8. **Evidence lens and inspection light:** relations, lowered steps, sheet backdrop.
9. **Evidence stack:** the inspector's new order with the new evidence lines.
10. **Storage and progress:** the disk strip and the resolving states.
11. **Quarantine and supporting UI:** marks in rows, confirmation rows, InfoTip,
    Notice, Settings.
12. **Subtraction and polish:** remove what the rendered review shows to be redundant;
    refresh screenshots.

## Decisions and limits

- **The navigation lens** is removed
  ([ADR-0052](../decisions/0052-restrained-navigation-without-glass.md), owner,
  2026-11-01).
- **More evidence first:** the owner chose to expose the evidence the core already
  has (entry times, facts, graph relations) before the evidence stack and lens are
  built (plan step 6, with its own ADR). Nothing new is scanned or sent to a model.
- **Choosing review items** ([ADR-0051](../decisions/0051-quarantine-review-items-the-user-chooses.md))
  is accepted but not yet in the interface. Its per-item choice uses the QUARANTINE
  mark in an unfilled "chosen" state when it lands; this redesign does not build it.
