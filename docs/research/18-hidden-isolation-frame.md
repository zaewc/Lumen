# The lower-left white pane

Verified on macOS WKWebView, 2026-10-28, with the production UI build and CSP.

Tauri 2.12.3's `scripts/isolation.js` appends a sandboxed iframe to the body.
It first injects an inline style, `#__tauri_isolation__ { display: none !important }`.
The production `style-src 'self'` blocks that style in the main document. In the
pinned codegen, the matching style hash is added to the isolation assets, not the
main UI's assets. The development CSP allows inline styles, concealing the defect.

The frame retains its default 300 × 150 CSS px box, adds 150 px of document scroll,
and appears at the lower left when that outer document scrolls to its end. At 2×
display scaling this matches the reported 600 × 300 white rectangle. The previous
overscroll treatment did not remove this extra layout box.

The external application stylesheet now hides only `#__tauri_isolation__`. The
frame remains in the document for IPC; isolation, its command allowlist and the
production CSP are unchanged. Allowing arbitrary inline styles or removing the
frame would weaken the existing security design (ADR-0004, ADR-0023).

## Reproduce and verify

On macOS, with Node 24, workspace dependencies and Xcode tools:

```sh
pnpm --filter @lumen/web build
node scripts/check-native-frame.mjs
```

This uses the actual built UI and production CSP in WKWebView. It injects the
pinned Tauri frame pattern with a local message responder, then scrolls the outer
document to the end. It checks light/dark appearances at 1120 and 420 px, zero frame
dimensions, no added document scroll, blocked inline styles and successful frame
messaging. Snapshots are written only to a temporary fixture directory. Removing
the external hiding rule reproduces the visible 300 × 150 pane and fails the check.
This rendering fixture does not replace the desktop host's IPC/security tests.
