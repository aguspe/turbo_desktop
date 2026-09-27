#!/bin/bash
# Remove what an end user's machine will never read.
#
# A shipped bundle carries a lot that only a build machine needs: static
# archives left over from linking, the .gem files RubyGems keeps after
# installing, headers for compiling extensions that are already compiled,
# generated documentation, and each gem's own test suite.
#
# Everything here is additive-safe: the app is booted after pruning to prove it,
# because "smaller" is worthless if it also means "broken".

set -euo pipefail
ROOT="${1:?usage: prune.sh <bundle Resources dir>}"
KEEP_DEV="${KEEP_DEV:-0}"

before=$(du -sm "$ROOT" | cut -f1)
removed() { printf '  %-30s %s\n' "$1" "$2"; }

size_of() { local total=0 f; while IFS= read -r -d '' f; do
  total=$((total + $(stat -f%z "$f" 2>/dev/null || stat -c%s "$f" 2>/dev/null || echo 0)))
done < <(cat); echo $((total / 1024 / 1024))M; }

# Static archives: linking is done, and nothing dlopens a .a
find "$ROOT" -name '*.a' -type f -print0 | tee >(size_of | xargs -I{} echo "  static archives (.a)          {}" >&2) | xargs -0 rm -f 2>/dev/null || true

# RubyGems keeps the original .gem after installing it. Nothing reads them.
find "$ROOT" -name '*.gem' -type f -delete 2>/dev/null || true
removed ".gem cache" "removed"

# Headers only matter when compiling extensions, which happens on the build machine.
rm -rf "$ROOT/ruby/include" 2>/dev/null || true
removed "ruby/include" "removed"

# Debug symbols
find "$ROOT" -name '*.dSYM' -type d -exec rm -rf {} + 2>/dev/null || true
removed "dSYM bundles" "removed"

if [ "$KEEP_DEV" != "1" ]; then
  # Generated docs, and each gem's own tests
  find "$ROOT" -type d \( -name 'ri' -o -name 'rdoc' \) -exec rm -rf {} + 2>/dev/null || true
  # Only at each gem's root. A bare `-name test` also deletes rack-test's
  # lib/rack/test/, which is library code, and the app then fails to boot.
  for base in "$ROOT/gems/gems" "$ROOT/ruby/lib/ruby/gems"/*/gems; do
    [ -d "$base" ] || continue
    find "$base" -mindepth 2 -maxdepth 2 -type d \( -name 'test' -o -name 'spec' -o -name 'features' \) \
      -exec rm -rf {} + 2>/dev/null || true
  done
  removed "docs and gem test suites" "removed"
fi

# Strip symbols from the binaries. Apple's strip regenerates the ad-hoc linker
# signature on arm64, so this does not invalidate anything — but the real
# codesign pass happens after this, which settles it either way.
stripped=0
while IFS= read -r -d '' f; do
  strip -S -x "$f" 2>/dev/null && stripped=$((stripped + 1))
done < <(find "$ROOT" \( -name '*.bundle' -o -name '*.dylib' -o -path '*/bin/ruby' \) -type f -print0)
removed "stripped binaries" "$stripped"

after=$(du -sm "$ROOT" | cut -f1)
echo "  ─────────────────────────────────────"
printf '  %-30s %sM -> %sM  (saved %sM)\n' "total" "$before" "$after" "$((before - after))"
