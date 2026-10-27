# 3D illustrations and the app icon

Created with the built-in image_gen tool on 2026-10-25 for the owner's request.
These are conceptual illustrations of safety, evidence and on-device storage;
they do not depict the application UI or real scan results. Transparent alpha is
preserved in the checked-in WebP files. No external font or runtime dependency.

Final project assets:

- `apps/site/public/art/protect.webp`
- `apps/site/public/art/evidence.webp`
- `apps/site/public/art/local.webp`

## Exact generation prompt set

Each final prompt is this shared prefix followed by the corresponding subject.

Use case: stylized-concept. Asset type: one large editorial 3D icon for a white storage-safety
product website. Render a premium, restrained sculptural 3D object, silver and warm-white
surfaces with one soft cobalt-indigo accent, subtle translucent glass, satin ceramic, softly
beveled precise geometry. Soft studio lighting from upper left, realistic ambient occlusion and
delicate contact shadows on the object only. Square composition, centered, entire object
visible, fills about 80 percent of frame, generous clear transparent edge. Consistent front
three-quarter view, slightly elevated camera. Actual transparent background, no backdrop, no
floor plane, no gradient background. No words, letters, numbers, labels, watermark, logos, UI,
application screenshot or decorative particles. Output a crisp 1024px or larger square image
with alpha.

### protect

Subject: a substantial translucent pale-indigo shield protecting a single upright warm-white
document with a folded corner. Shield is the dominant form, document peeks above and to the
right. Communicates protecting files before reclaiming space. Simple two-part sculpture, no
badge frame.

### evidence

Subject: an oversized silver magnifying glass with a short dark-indigo handle hovering across
two neatly stacked warm-white document sheets with folded corners. A few subtle embossed short
indigo strokes imply evidence, with no legible text. Communicates carefully examining evidence
about a file. Magnifier is the dominant readable silhouette, no badge frame.

### local

Subject: a compact satin-silver external solid-state drive with softly rounded precise metal
edges, seen as a substantial upright rectangular storage object, with a small closed indigo
padlock resting against its lower front. One discreet blue indicator, no ports or cords
extending beyond the silhouette. Communicates files staying private on the user's own device and
deterministic safety. Simple two-part sculpture, no badge frame.


## App icon

Generated on 2026-10-27 by the owner with the same image tool and style, to replace the
flat indigo icon. The 1,254 px original was scaled to the 1,024 px master
`apps/desktop/icon.png`; `tauri icon` produced `apps/desktop/src-tauri/icons/`
(`icon.icns`, `icon.ico`, the PNG sizes), and the website (`apps/site/public/icon.png`,
`favicon.png`, `apple-touch-icon.png`) and the app's sidebar mark and favicon
(`apps/web/public/icon.png`, `favicon.png`) use downscaled copies. It stays legible at
32 px on light and dark backgrounds.

Prompt (the shared style above, adapted to a macOS icon):

```
Use case: app icon. Asset type: one macOS application icon in the same family as a set of
editorial 3D illustrations for a white storage-safety product website. Render a premium,
restrained sculptural 3D object, silver and warm-white surfaces with one soft cobalt-indigo
accent, subtle translucent glass, satin ceramic, softly beveled precise geometry. Soft studio
lighting from upper left, realistic ambient occlusion and delicate contact shadows.

Icon format: a 1024x1024 canvas with a transparent background. Inside it, a macOS-style
rounded-square plate (continuous-corner "squircle") about 824x824 px, centered, with a soft
drop shadow under the plate only. The plate is satin warm-white with a very subtle top-to-bottom
tonal shift, no pattern.

Subject on the plate: a thick satin-silver storage disc seen from a slightly elevated front
three-quarter view, with a small translucent cobalt-indigo glass four-point star of light
resting just above its upper right edge, as if the disc is being gently illuminated.
The disc is the dominant form and fills about 60 percent of the plate; the star is small and
quiet. Communicates understanding storage safely. Simple two-part sculpture.

No words, letters, numbers, labels, watermark, logos, UI or decorative particles. Must stay
legible at 32x32 px: strong simple silhouette, no thin details. Output a crisp 1024x1024 PNG
with alpha.
```
