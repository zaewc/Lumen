# Download-page redesign

Checked on 2026-10-25 for the owner's brief: a small download interface, restrained
type and spacing, evidence-led safety, and no invented product visuals or claims.

## Product and release evidence

- `gh release list --repo zaewc/Lumen` returns no releases, including drafts. The
  page must show that state honestly, without a version, download count or promise
  of a release date. Live release lookup remains the authority after publication.
- README identifies the project as pre-alpha. Its older read-only status paragraph
  predates the implemented policy and quarantine flow: roadmap 7–11 and
  `apps/desktop/src-tauri/src/e2e.rs` cover scan, plan, confirmation, quarantine,
  restore and finalization. Copy follows those implementations and their limits.
- ADR-0038 and `docs/operations/release.md` define unsigned desktop distribution:
  universal macOS, Windows x64, and Linux amd64. Mobile targets in the product's
  positioning do not imply a downloadable phone app.
- Policy, quarantine and AI claims follow ADR-0014, ADR-0015 and ADR-0020: uncertain
  items stay put, the user confirms changes, restoration ends at finalization, AI
  supplies evidence rather than authority, and file contents never go to a model.
- The repository has app icons but no suitable product screenshots. Use the real
  brand icon and typography, without fabricating a preview.

## Composition

Keep the existing Astro build, locale URLs, external scripts/styles, strict CSP and
validated GitHub release loader. No new dependencies, fonts, analytics or images.

One primary download action, one native disclosure for the other desktop builds
and installation/verification help, then concise safety statements and source/docs
links. No feature cards or marketing navigation. Keep the pre-alpha and unsigned
status visible even when release lookup fails or JavaScript is unavailable.

Validate actual rendered light/dark and narrow/wide layouts, keyboard focus,
platform detection, release/loading/missing/error states, no-JavaScript links,
disclosures, hostile API data and the external-only CSP.
