#!/usr/bin/env bash
# Builds lumen-helper (ADR-0063) for the macOS app bundle, as Tauri's
# beforeBundleCommand: for the target being bundled (TAURI_ENV_TARGET_TRIPLE,
# both architectures joined for universal-apple-darwin), in the profile being
# bundled (TAURI_ENV_DEBUG), to target/lumen-helper/lumen-helper, which
# tauri.macos.conf.json copies to Lumen.app/Contents/MacOS/. It is ad-hoc signed
# here for Tauri's bundle step; scripts/sign-macos.sh later replaces that with
# the Developer ID signature.
set -euo pipefail

cd "$(dirname "$0")/.."
triple=${TAURI_ENV_TARGET_TRIPLE:-$(rustc -vV | sed -n 's/^host: //p')}
profile=release
flags=(--release)
if [ "${TAURI_ENV_DEBUG:-false}" = true ]; then
  profile=debug
  flags=()
fi
# Cargo builds where it is told to; the bundle takes the helper from here.
build=${CARGO_TARGET_DIR:-target}
out=target/lumen-helper
mkdir -p "$out"

case "$triple" in
  universal-apple-darwin)
    cargo build "${flags[@]}" --locked -p lumen-helper \
      --target aarch64-apple-darwin --target x86_64-apple-darwin
    lipo -create -output "$out/lumen-helper" \
      "$build/aarch64-apple-darwin/$profile/lumen-helper" \
      "$build/x86_64-apple-darwin/$profile/lumen-helper"
    ;;
  *-apple-darwin)
    cargo build "${flags[@]}" --locked -p lumen-helper --target "$triple"
    cp "$build/$triple/$profile/lumen-helper" "$out/lumen-helper"
    ;;
  *)
    echo "lumen-helper is bundled only on macOS, not for $triple" >&2
    exit 1
    ;;
esac
codesign --force --sign - "$out/lumen-helper"
echo "built $out/lumen-helper for $triple ($profile)"
