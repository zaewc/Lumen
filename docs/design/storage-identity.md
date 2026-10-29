# Lumen: understanding storage

The owner's 2026-10-28 brief replaces the previous operating-system utility
direction for the desktop and browser interface. Lumen is a visual instrument for
understanding storage and evaluating evidence, before making a decision.

## Composition

- A compact horizontal workspace navigation keeps the four places and their
  keyboard shortcuts. The content gets the full width for spatial information.
- Overview pairs a measured view of disk capacity with the last assessment.
  Physical disk usage and the candidates in a scan remain separate quantities.
- Results connect a decision overview to grouped artifacts and an evidence
  inspector. The inspector exposes origins, location, reasons and tool actions.
- Quarantine emphasizes where items came from and the available restore action.
  Settings uses readable sections rather than imitating System Settings.

## Visual system

- A cool paper canvas, dark ink, generous but useful type, and a small cobalt
  signature. Dark mode has its own surface hierarchy and legible secondary text.
- Settings offers System, Light and Dark. The local preference persists; System
  follows appearance changes while running. Tokens, semantic dark variants, native
  controls and desktop chrome use the same choice. Screenshot fixtures select System.
  The selectors follow [Tailwind's custom dark mode](https://tailwindcss.com/docs/dark-mode);
  desktop chrome uses [Tauri's app theme](https://docs.rs/tauri/2.12.3/tauri/struct.AppHandle.html#method.set_theme).
- A 4 px spacing unit; 12/14 px metadata/body, 28 px page titles and larger
  tabular figures only for the storage measurements that anchor a view.
- Rules and alignment organize lists. Surfaces are reserved for input, inspection
  and confirmation. No card grids, decorative charts or AI branding.
- KEEP uses a solid mark; REVIEW an open/dashed mark; QUARANTINE a bounded square;
  tool commands a directional mark. Words accompany every meaningful symbol.
- Selected navigation and controls have a clear cobalt anchor. Hover, press and
  focus are distinct; motion connects state changes and honors reduced motion.

## Data and interaction

The visual language follows the actual progression: scan, examine evidence, then
decide. A scan is indeterminate unless the host supplies a total. Unreadable areas
remain visibly unknown. Evidence counts are not confidence scores, and no dependency
or ownership relationship is invented when the API does not provide it.

No policy, transport, selection, confirmation or irreversible-action contract changes.
AI disclosure and its off switch remain visible. Quarantine does not free space
until final removal. Both languages, both appearances, keyboard operation and
narrow windows are reviewed in the rendered app, followed by a separate polish pass.

Confirmed tool execution belongs to the application session. Closing its inspector
or changing pages keeps the running state and latest output. The workspace shows
ongoing execution even with the inspector closed; another command waits until it
finishes. Copying and unconfirmed choices stay local to the inspector. Session
state is in memory and does not imply that work survives quitting the application.

Quarantine actions follow the same session lifetime. A confirmed batch retains its
original item selection and progress while other pages are open. Returning shows
ongoing work or the last outcome; the list waits before offering another action.
After a partial failure, held items and disk figures refresh to reflect what happened.

Confirmed clean-up and update installation also continue across navigation. A new
scan waits for a clean-up to finish so it cannot replace that operation's evidence
or report. Unconfirmed plans are local to their view and never run automatically.
AI setting writes share their pending/error status across views, without copying
typed API keys into the session-operation cache.

The scan, evidence and decision stages appear in the start, progress, confirmation
and completion views. They describe actual workflow states and never imply a scan
percentage. Settings and held-item rows use the same readable type and spacing.
In short windows, confirmation titles/actions stay visible while their explanation
scrolls. The long AI disclosure starts focus on its heading and keeps a structured
list, following the [WAI dialog focus guidance](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/).
Dark primary buttons retain at least 4.5:1 white-text contrast, including hover;
irreversible actions use the same readable red fill in both appearances.
