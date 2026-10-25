# Download-page redesign

Checked on 2026-10-25 for the owner's revised brief: a product-scale download
page, white surfaces, large type and no invented product visuals or claims.

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
validated GitHub release loader. No new dependencies, fonts or analytics. Three conceptual 3D illustrations follow
the owner's later request; their exact prompts and paths are in
[site-illustrations.md](../design/site-illustrations.md).

One primary download action, one native disclosure for the other desktop builds
and installation/verification help, then separate editorial safety chapters and source/docs
links. No feature cards or marketing navigation. The owner removed the two hero
release/maturity notices; unsigned-app opening instructions remain in the
installation disclosure, including without JavaScript.

Validate actual rendered light layouts under both system themes and narrow/wide layouts, keyboard focus,
platform detection, release/loading/missing/error states, no-JavaScript links,
disclosures, hostile API data and the external-only CSP.

## Rendered review

Chrome passed 20 cases: Korean/English, 320–1440px, both system theme preferences,
macOS/Windows/Linux, iPad detection, release/empty/loading/error states, unavailable
assets, no JavaScript, keyboard focus, nested disclosure links, malformed fragments
and 404. All pages stay light, with no horizontal overflow, inline styles/scripts
or CSP violations. Inspected actual rendered 1440px, 1024px and 390px screenshots.
The desktop hero headline is 64–80px; the later illustration pass reduces its
minimum height to 78vh minus navigation.

## Revised art direction

The owner's follow-up replaces the compact document composition with a wide
product launch page: a 75–90vh desktop hero, 64–80px headline, larger product
presence and separate editorial safety chapters. This website-specific direction
supersedes the earlier small-type preference; the application UI is unchanged.
The site always uses a white/light palette, including when the OS prefers dark.
The hero omits release-preparation and maturity notices at the owner's request.
Successful release metadata, loading/error feedback and installation help remain.

## Illustrations and real product capture

The owner subsequently requested large 3D objects beside or partly overlapping the
headlines to explain each idea and reduce empty space. A shield protecting a file,
a magnifier examining documents and a locked local drive accompany the three
chapters, with alternating alignment, transparent edges and a shorter hero.
The same objects sit above the text on narrow screens without obscuring letters.

The repository has no usable app screenshot. Remove the large visible empty
canvas; keep an insertion-point comment in `Home.astro` for a future authentic
capture. No illustration is presented as application UI or real storage data.

With no public release, the enhanced main action links to the development page.
Loading, error and no-JavaScript fallbacks still link to Releases; a valid published
release selects the detected desktop's installer, or opens the download choices.
