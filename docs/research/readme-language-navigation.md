# README language navigation

Verified 2026-11-07.

Generated SVG tabs reproduce the desktop app's language order, system font stack,
13px medium labels, neutral baseline and 2px cobalt selection rule. Sources:

- [Language names and order](../../apps/web/src/shared/i18n/locale.ts)
- [Segmented control](../../apps/web/src/shared/ui/Segmented.tsx)
- [Light and dark colour tokens](../../apps/web/src/app/styles.css)

Run `node scripts/generate-readme-language-tabs.mjs` from the repository root to
refresh the images and the marked navigation blocks in all translated READMEs.
No dependencies or external image service are needed.

GitHub [strips scripts and custom styling](https://github.com/github/markup#github-markup)
from README markup. HTML/CSS radio tabs cannot reproduce the app's interaction.
Each SVG tab is therefore
an ordinary link to a translated README; that document marks its own language as
selected. A `picture` chooses the light or dark image, as for the app screenshots.
Transparent images inherit the README background. The 88 × 44px links wrap on
narrow screens and keep a readable label and a touch-sized target.

Verification used GitHub's Markdown API (`gh api markdown -f mode=gfm
-f context=zaewc/Lumen -F text=@README.ko.md`), which preserved all seven linked
`picture` elements. Its rendered markup was inspected in isolated headless Chrome
at 1120px and 375px in both colour schemes: all seven images loaded with the right
theme, one language was selected, and the navigation had no horizontal overflow.
All seven README destinations and 98 image references resolve locally; SVG XML
parses, regeneration is deterministic, and `cargo xtask lint` passes.
