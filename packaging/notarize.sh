#!/bin/bash
# Notarise and staple a signed .app, then verify Gatekeeper accepts it.
#
# This is the step that makes a downloaded app open on someone else's Mac.
# Measured on macOS 26.5.1: an ad-hoc signed bundle is `rejected` by `spctl`
# with "no usable signature", quarantined or not. So this is not optional
# polish — without it the app cannot be distributed at all.
#
# Needs a real identity, which is the one thing the rest of this pipeline does
# not: an Apple Developer Program membership, a "Developer ID Application"
# certificate in the keychain, and an app-specific password or API key.
#
# Usage:
#   packaging/notarize.sh --app dist/Ledger.app \
#     --identity "Developer ID Application: Your Name (TEAMID)" \
#     --keychain-profile notary
#
# Create the keychain profile once:
#   xcrun notarytool store-credentials notary \
#     --apple-id you@example.com --team-id TEAMID --password <app-specific>

set -euo pipefail
APP=""; IDENTITY=""; PROFILE="notary"; ENTS="$(dirname "$0")/entitlements.plist"

while [ $# -gt 0 ]; do
  case "$1" in
    --app) APP="$2"; shift 2 ;;
    --identity) IDENTITY="$2"; shift 2 ;;
    --keychain-profile) PROFILE="$2"; shift 2 ;;
    --entitlements) ENTS="$2"; shift 2 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown option: $1"; exit 1 ;;
  esac
done

[ -d "$APP" ] || { echo "--app must be a .app bundle"; exit 1; }
if [ -z "$IDENTITY" ]; then
  echo "--identity is required. Available Developer ID certificates:"
  security find-identity -v -p codesigning 2>/dev/null | grep "Developer ID Application" || echo "  none in the keychain"
  exit 1
fi

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

step "Re-signing with the real identity"
# Inside-out, and the entitlements go on the interpreter: the app's main
# executable is a launcher script, and `ruby` is what dlopens the extensions.
args=(--force --timestamp --options runtime --entitlements "$ENTS" --sign "$IDENTITY")
n=0
while IFS= read -r -d '' f; do
  codesign "${args[@]}" "$f" && n=$((n + 1))
done < <(find "$APP/Contents/Resources" \( -name '*.dylib' -o -name '*.bundle' -o -name '*.so' \) -type f -print0)
codesign "${args[@]}" "$APP/Contents/Resources/ruby/bin/ruby"
codesign "${args[@]}" "$APP"
echo "  signed $n nested binaries, the interpreter and the bundle"

step "Submitting to the notary service"
# notarytool takes an archive, not a bundle.
ZIP="$(mktemp -d)/$(basename "$APP" .app).zip"
ditto -c -k --keepParent "$APP" "$ZIP"
xcrun notarytool submit "$ZIP" --keychain-profile "$PROFILE" --wait

step "Stapling"
xcrun stapler staple "$APP"

step "Verifying the way Gatekeeper will"
codesign --verify --strict --deep --verbose=2 "$APP"
spctl -a -vvv "$APP"
echo
echo "  A downloaded copy carries com.apple.quarantine. Test that too:"
echo "    xattr -w com.apple.quarantine '0083;0;Safari;' <copy>.app && spctl -a -vvv <copy>.app"
