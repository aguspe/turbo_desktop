#!/bin/bash
# Wrap a .app in a disk image, which is how macOS apps are actually delivered.
#
# No certificate needed to build one. Signing and notarising the *contents* is
# a separate step, and what Gatekeeper judges on first launch.
set -euo pipefail
APP="${1:?usage: dmg.sh <path to .app> [output.dmg]}"
NAME="$(basename "$APP" .app)"
OUT="${2:-$(dirname "$APP")/$NAME.dmg}"
STAGE="$(mktemp -d)/$NAME"

mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"   # the drag-to-install convention

rm -f "$OUT"
hdiutil create -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO -quiet "$OUT"
rm -rf "$(dirname "$STAGE")"

echo "$OUT ($(du -sh "$OUT" | cut -f1))"
