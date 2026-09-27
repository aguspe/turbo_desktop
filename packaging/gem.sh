#!/bin/bash
# Wrap a built interpreter in a platform gem, so a developer gets a runtime to
# package with `bundle add` and no compiler.
#
# One gem per platform triple. RubyGems resolves the right one automatically:
# the developer writes `gem "turbo_desktop-runtime"` and gets the build for
# their machine, the same mechanism nokogiri and sqlite3 use.
#
# Usage: packaging/gem.sh --runtime out/ruby --triple arm64-darwin --version 0.1.0

set -euo pipefail
cd "$(dirname "$0")/.."

RUNTIME=""; TRIPLE=""; VERSION="0.1.0"; OUT="$PWD/dist/gems"
while [ $# -gt 0 ]; do
  case "$1" in
    --runtime) RUNTIME="$2"; shift 2 ;;
    --triple)  TRIPLE="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --out)     OUT="$2"; shift 2 ;;
    *) echo "unknown option: $1"; exit 1 ;;
  esac
done
[ -d "$RUNTIME" ] || { echo "--runtime must be a built interpreter prefix"; exit 1; }
[ -n "$TRIPLE" ]  || { echo "--triple is required"; exit 1; }

STAGE="$(mktemp -d)/turbo_desktop-runtime"
mkdir -p "$STAGE/runtime" "$OUT"
cp -R packaging/runtime-gem/lib "$STAGE/lib"
cp -R "$RUNTIME" "$STAGE/runtime/ruby"

cat > "$STAGE/turbo_desktop-runtime.gemspec" <<SPEC
Gem::Specification.new do |spec|
  spec.name     = "turbo_desktop-runtime"
  spec.version  = "$VERSION"
  spec.platform = "$TRIPLE"
  spec.summary  = "A relocatable Ruby for packaging Turbo Desktop apps"
  spec.description = <<~TEXT
    A prebuilt, relocatable Ruby that can be copied into an application bundle.
    A package-manager Ruby cannot: it links libyaml, OpenSSL and gmp by absolute
    path, and Rails will not boot without psych. This one is built with
    --enable-load-relative against statically linked dependencies, so it runs
    from wherever it is put.
  TEXT
  spec.authors  = ["Turbo Desktop contributors"]
  spec.license  = "MIT"
  spec.homepage = "https://github.com/aguspe/turbo_desktop"
  spec.required_ruby_version = ">= 3.1"

  spec.files = Dir["lib/**/*", "runtime/**/*"].select { |f| File.file?(f) || File.symlink?(f) }
  spec.require_paths = ["lib"]
  spec.metadata["rubygems_mfa_required"] = "true"
end
SPEC

( cd "$STAGE" && gem build turbo_desktop-runtime.gemspec --output "$OUT/turbo_desktop-runtime-$VERSION-$TRIPLE.gem" )
ls -lh "$OUT/turbo_desktop-runtime-$VERSION-$TRIPLE.gem"
rm -rf "$(dirname "$STAGE")"
