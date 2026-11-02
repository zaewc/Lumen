#!/usr/bin/env bash
# The release a push to main makes (ADR-0054): the workspace version in
# Cargo.toml, which tauri.conf.json must match, and whether its `v` tag exists
# yet. Prints `tag=v<version>` and `released=true|false`, and appends them to
# $GITHUB_OUTPUT when that is set. With `--unreleased`, fails if the tag exists:
# a pull request into main must name a version that is not out yet.
set -euo pipefail

cd "$(dirname "$0")/.."
cargo=$(awk '
  /^\[workspace\.package\]/ { inside = 1; next }
  /^\[/ { inside = 0 }
  inside && /^version *=/ { gsub(/"/, "", $3); print $3; exit }
' Cargo.toml)
desktop=$(jq -r .version apps/desktop/src-tauri/tauri.conf.json)
if [ -z "$cargo" ] || [ "$cargo" != "$desktop" ]; then
  echo "::error::Cargo.toml has version '$cargo' but tauri.conf.json has '$desktop'" >&2
  exit 1
fi
tag="v$cargo"

# 0: the tag exists; 2: it does not; anything else is a failure, never "not
# released", so a network error cannot start a release twice.
status=0
git ls-remote --exit-code --tags \
  "https://github.com/${GITHUB_REPOSITORY:-zaewc/Lumen}.git" "refs/tags/$tag" >/dev/null ||
  status=$?
case "$status" in
  0) released=true ;;
  2) released=false ;;
  *)
    echo "::error::could not list the repository's tags (git exit $status)" >&2
    exit 1
    ;;
esac

if [ "${1:-}" = --unreleased ] && [ "$released" = true ]; then
  echo "::error::$tag is already released; raise the version in Cargo.toml and tauri.conf.json before merging into main" >&2
  exit 1
fi

echo "tag=$tag"
echo "released=$released"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  printf 'tag=%s\nreleased=%s\n' "$tag" "$released" >>"$GITHUB_OUTPUT"
fi
