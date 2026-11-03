#!/usr/bin/env bash
# Signs and notarizes the macOS desktop app (ADR-0055).
#
# Usage: scripts/sign-macos.sh <in-dir> <out-dir>
#
# <in-dir> holds the desktop job's unsigned Lumen_<version>_universal.dmg and
# Lumen_<version>_universal.app.tar.gz. The app is signed inside out with the
# Developer ID and the Hardened Runtime, notarized and stapled; it then
# replaces the app in Tauri's disk image, which is signed, notarized and
# stapled in turn; the updater archive is made again from the signed app.
# Both files are written to <out-dir> under the same names.
#
# Release runs need these, from the `release` environment:
#   APPLE_CERTIFICATE           the Developer ID Application certificate and key (.p12, base64)
#   APPLE_CERTIFICATE_PASSWORD  its export password
#   APPLE_API_KEY_ID            an App Store Connect API key's ID, for notarytool
#   APPLE_API_ISSUER            its issuer ID
#   APPLE_API_PRIVATE_KEY       its private key (the .p8 file's text)
# With LUMEN_SIGN_DRY_RUN=1 (pull requests) the same steps run with an ad-hoc
# signature and without notarization, and no secret is read.
set -euo pipefail

in_dir=$(cd "$1" && pwd)
mkdir -p "$2"
out_dir=$(cd "$2" && pwd)
dry_run=${LUMEN_SIGN_DRY_RUN:-0}

work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/sign-macos.XXXXXX")
keychain="$work/signing.keychain-db"
mount="$work/volume"
cleanup() {
  if [ -d "$mount" ]; then hdiutil detach -quiet -force "$mount" || true; fi
  if [ -f "$keychain" ]; then security delete-keychain "$keychain" || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

shopt -s nullglob
dmgs=("$in_dir"/Lumen_*_universal.dmg)
shopt -u nullglob
if [ "${#dmgs[@]}" -ne 1 ]; then
  echo "::error::expected one Lumen_<version>_universal.dmg in $in_dir"
  exit 1
fi
dmg_name=$(basename "${dmgs[0]}")
version=${dmg_name#Lumen_}
version=${version%_universal.dmg}
archive_name="Lumen_${version}_universal.app.tar.gz"
if [ ! -s "$in_dir/$archive_name" ]; then
  echo "::error::missing $archive_name in $in_dir"
  exit 1
fi

# The signing identity: ad-hoc for a dry run, otherwise the Developer ID
# from a keychain that exists only for this run.
if [ "$dry_run" = 1 ]; then
  identity=-
else
  for name in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_API_KEY_ID \
              APPLE_API_ISSUER APPLE_API_PRIVATE_KEY; do
    if [ -z "${!name:-}" ]; then
      echo "::error::the release environment has no $name secret (ADR-0055)"
      exit 1
    fi
  done
  (
    umask 077
    printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$work/certificate.p12"
    printf '%s\n' "$APPLE_API_PRIVATE_KEY" > "$work/api-key.p8"
  )
  keychain_password=$(openssl rand -hex 24)
  security create-keychain -p "$keychain_password" "$keychain"
  security set-keychain-settings -lut 3600 "$keychain"
  security unlock-keychain -p "$keychain_password" "$keychain"
  security import "$work/certificate.p12" -k "$keychain" \
    -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
  rm -f "$work/certificate.p12"
  # Lets codesign use the key without a dialog nobody could answer.
  security set-key-partition-list -S apple-tool:,apple:,codesign: \
    -s -k "$keychain_password" "$keychain" > /dev/null
  # The keychain joins the search list so codesign finds the intermediate
  # certificates beside the identity.
  existing=()
  while IFS= read -r line; do
    line=${line#"${line%%[![:space:]]*}"}
    existing+=("${line//\"/}")
  done < <(security list-keychains -d user)
  security list-keychains -d user -s "$keychain" "${existing[@]}"
  identities=$(security find-identity -v -p codesigning "$keychain" |
    grep -c '"Developer ID Application: ' || true)
  if [ "$identities" -ne 1 ]; then
    echo "::error::APPLE_CERTIFICATE must hold exactly one Developer ID Application identity, found $identities"
    exit 1
  fi
  identity=$(security find-identity -v -p codesigning "$keychain" |
    sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p')
  echo "Signing as $identity"
fi

# Signs one file or bundle. A dry run has no timestamp server to ask.
sign() {
  local options=(--force --sign "$identity")
  if [ "$dry_run" = 1 ]; then
    options+=(--timestamp=none)
  else
    options+=(--timestamp --keychain "$keychain")
  fi
  codesign "${options[@]}" "$@"
}

# Submits a file to Apple's notary service, waits, and fails with the log
# unless it is accepted.
notarize() {
  local file=$1 result id status
  result=$(xcrun notarytool submit "$file" \
    --key "$work/api-key.p8" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER" \
    --wait --timeout 1h --output-format json)
  id=$(jq -r .id <<< "$result")
  status=$(jq -r .status <<< "$result")
  echo "Notarization of $(basename "$file"): $status ($id)"
  if [ "$status" != Accepted ]; then
    xcrun notarytool log "$id" \
      --key "$work/api-key.p8" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER" || true
    echo "::error::Apple did not notarize $(basename "$file"): $status"
    exit 1
  fi
}

# The app, unpacked from the updater archive, which keeps its permissions.
mkdir "$work/app"
tar -xzf "$in_dir/$archive_name" -C "$work/app"
app="$work/app/Lumen.app"
if [ ! -d "$app/Contents/MacOS" ]; then
  echo "::error::$archive_name does not hold Lumen.app"
  exit 1
fi

# Inside out: every Mach-O file below Contents, then the bundle. Lumen ships
# no nested bundles; one appearing needs its own signing order, so it stops
# the run instead of being signed as a plain file.
nested=$(find "$app/Contents" -mindepth 1 -type d \
  \( -name '*.app' -o -name '*.framework' -o -name '*.xpc' -o -name '*.appex' \
     -o -name '*.bundle' -o -name '*.plugin' \) -print -quit)
if [ -n "$nested" ]; then
  echo "::error::nested bundle ${nested#"$work/app/"} needs its own signing order (ADR-0055)"
  exit 1
fi
main="$app/Contents/MacOS/$(plutil -extract CFBundleExecutable raw "$app/Contents/Info.plist")"
while IFS= read -r -d '' file; do
  if [ "$file" != "$main" ] && file -b "$file" | grep -q '^Mach-O'; then
    sign --options runtime "$file"
  fi
done < <(find "$app/Contents" -type f -print0)
sign --options runtime "$app"
codesign --verify --strict --deep --verbose=2 "$app"
details=$(codesign -d --verbose=2 "$app" 2>&1)
if ! grep -q 'flags=.*runtime' <<< "$details"; then
  echo "::error::Lumen.app is not signed with the Hardened Runtime"
  exit 1
fi

if [ "$dry_run" != 1 ]; then
  ditto -c -k --keepParent "$app" "$work/Lumen.zip"
  notarize "$work/Lumen.zip"
  xcrun stapler staple "$app"
  xcrun stapler validate "$app"
  spctl --assess --type execute --verbose=2 "$app"
fi

# The disk image: Tauri's layout kept, the app replaced. The writable copy
# grows by 64 MB so the signature and ticket fit; compression drops the slack.
hdiutil convert -quiet "$in_dir/$dmg_name" -format UDRW -o "$work/rw.dmg"
sectors=$(hdiutil resize -limits "$work/rw.dmg" | awk 'NR == 1 { print $2 }')
hdiutil resize -quiet -sectors $((sectors + 131072)) "$work/rw.dmg"
mkdir "$mount"
hdiutil attach -quiet -nobrowse -noautoopen -noverify -mountpoint "$mount" "$work/rw.dmg"
if [ ! -d "$mount/Lumen.app" ]; then
  echo "::error::$dmg_name does not hold Lumen.app"
  exit 1
fi
rm -rf "$mount/Lumen.app"
ditto "$app" "$mount/Lumen.app"
rm -rf "$mount/.fseventsd"
hdiutil detach -quiet "$mount"
rmdir "$mount"
hdiutil convert -quiet "$work/rw.dmg" -format UDZO -imagekey zlib-level=9 -o "$out_dir/$dmg_name"
sign "$out_dir/$dmg_name"
codesign --verify --strict --verbose=2 "$out_dir/$dmg_name"

if [ "$dry_run" != 1 ]; then
  notarize "$out_dir/$dmg_name"
  xcrun stapler staple "$out_dir/$dmg_name"
  xcrun stapler validate "$out_dir/$dmg_name"
  spctl --assess --type open --context context:primary-signature --verbose=2 "$out_dir/$dmg_name"
fi

# The updater's archive, made as the desktop job makes it, from the signed app.
COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs \
  -C "$work/app" -czf "$out_dir/$archive_name" Lumen.app
ls -l "$out_dir"
