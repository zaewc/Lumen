# UI and UX principles

The owner's design brief for every Lumen interface: the desktop app (`apps/web` in
`apps/desktop`), the mobile app (`apps/mobile`) and the website (`apps/site`).

The interface should feel precise, visual, calm and distinctive. Its identity comes
from understanding storage, relationships and evidence, before deciding what to do.
The desktop and browser direction is specified in [storage-identity.md](storage-identity.md)
(owner, 2026-10-28), replacing the earlier operating-system utility appearance.

## Core principles

- Start from the user's task, not from components.
- Every screen has one clear primary purpose.
- Reduce the number of visible elements whenever possible; prefer removing UI over
  adding UI.
- Build hierarchy with spacing, typography, alignment and contrast before containers or
  decoration.
- Make the primary action immediately obvious; secondary actions stay secondary.
- Optimize for scanning rather than reading.
- Use progressive disclosure instead of showing every option at once.
- Prefer familiar interaction patterns; make the interface understandable without
  instructions.

Before adding an element, ask:

1. Does the user need this?
2. Does it need to be visible right now?
3. Is there a simpler way to express it?
4. Would the interface still work without it? If yes, strongly consider removing it.

## Avoid generated-looking UI

Avoid: excessive rounded cards; cards nested inside cards; excessive borders and
containers; unnecessary gradients (especially purple/blue SaaS gradients); glowing
elements; glassmorphism without a functional reason; giant hero text; excessive pills,
badges and shadows; floating decorative elements; decorative charts; blobs or background
shapes; an icon box for every feature; three-column feature-card grids by default;
whitespace so generous it lowers information density; excessive animation; marketing
copy inside the product; unnecessary headings and explanatory text; generic "Welcome
back" headers; generic copy such as "Unlock the power of..."; making every section
visually distinct when simple grouping would work.

A group of related elements does not automatically need a card around it. Use
whitespace and alignment before borders, backgrounds or containers.

## Visual direction

Restrained, precise, calm, coherent, product-specific, slightly opinionated, highly
usable. A few strong decisions rather than many weak ones:

- a restrained palette with one clear accent colour;
- consistent spacing and alignment, intentional density;
- strong typographic hierarchy (typography carries most of it);
- subtle borders, minimal shadow.

No default "AI fonts" (Inter, Roboto, Arial, Space Grotesk); use the platform's own
type (San Francisco and Apple SD Gothic Neo on Apple platforms, Segoe UI and Malgun
Gothic on Windows, Roboto only where it is Android's system font). No unusual fonts for
novelty.

## Platform awareness

- **Desktop:** density, keyboard access and shortcuts, context menus, resizable
  layouts, efficient workflows; never a stretched mobile app.
- **Mobile:** thumb reach, native navigation, touch targets, gestures, safe areas,
  one-handed use; never a shrunken desktop layout.
- **Web:** responsive behaviour, accessibility, keyboard navigation, semantic HTML,
  predictable browser behaviour, fast perceived performance.

Preserve platform accessibility, keyboard and window behavior. Visual composition
serves Lumen's product identity rather than imitating operating-system applications.

## Interaction

Immediate and predictable; every action has clear feedback. Design every relevant
state: hover, focus, active, disabled, loading, empty, error, success. Empty states say
what to do next; error states say what happened and how to recover. Motion only to
communicate state change, hierarchy, spatial relationship or feedback; subtle and short.

## Copy

Write like a product team, not a marketing page: short and direct ("Save changes", not
"Submit"; "No projects yet", not an enthusiastic paragraph). Do not explain obvious UI.
Keep terminology consistent across the product. The full writing rules, including
Korean tone and Lumen's glossary, are in [ux-writing.md](ux-writing.md).

## Process

Before a screen: identify the user's primary goal, the minimum information needed and
the primary action; remove unnecessary controls and content; establish hierarchy; choose
the simplest layout that supports it; only then style it. In an existing product,
establish a coherent spacing, typography, colour and interaction system before
redesigning individual components. Do not preserve weak visual choices for consistency.

## Final review

Can anything be removed? Is anything competing with the primary action? Too many
containers, cards or labels? Is text explaining what the UI could show? Does it look
like a generic generated SaaS interface? Does it respect its platform? Is the hierarchy
obvious within two seconds? Would an experienced product designer call any element
decorative? If so, simplify. When unsure between two approaches, choose the simpler one.
