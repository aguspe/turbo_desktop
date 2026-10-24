#!/bin/bash
# Build a Ruby that can be moved, for shipping inside an app bundle.
#
# A package-manager Ruby cannot be shipped. On macOS libruby links gmp,
# openssl.bundle links libssl, and stdlib psych.bundle links libyaml, all by
# absolute path; Rails will not boot without psych, so copying an existing
# interpreter fails on any path. Linux has the same problem with a distro Ruby.
#
# So the interpreter is built for the job: --enable-load-relative makes it
# resolve its own lib/ and encodings relative to the binary, and libyaml and
# OpenSSL are compiled statically from source into a vendor prefix so nothing
# points outside the bundle.
#
# Windows is not built here. RubyInstaller already ships a portable archive that
# relocates correctly (see spike 3), so that platform downloads rather than
# builds.
#
# Usage: packaging/build-runtime.sh [--out DIR]

set -euo pipefail
cd "$(dirname "$0")/.."

RUBY_VERSION="${RUBY_VERSION:-3.4.8}"
OPENSSL_VERSION="${OPENSSL_VERSION:-3.5.4}"
YAML_VERSION="${YAML_VERSION:-0.2.5}"
OUT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "unknown option: $1"; exit 1 ;;
  esac
done

WORK="${WORK:-$PWD/.runtime-build}"
SRC="$WORK/src"; VENDOR="$WORK/vendor"
PREFIX="${OUT:-$WORK/out/ruby}"
JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"
mkdir -p "$SRC" "$VENDOR" "$(dirname "$PREFIX")"

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

case "$(uname -s)" in
  Darwin)
    case "$(uname -m)" in
      arm64) TRIPLE="arm64-darwin";  SSL_TARGET="darwin64-arm64-cc" ;;
      *)     TRIPLE="x86_64-darwin"; SSL_TARGET="darwin64-x86_64-cc" ;;
    esac ;;
  Linux)
    case "$(uname -m)" in
      aarch64|arm64) TRIPLE="aarch64-linux"; SSL_TARGET="linux-aarch64" ;;
      *)             TRIPLE="x86_64-linux";  SSL_TARGET="linux-x86_64" ;;
    esac ;;
  *) echo "Unsupported platform. Windows uses the RubyInstaller portable archive."; exit 1 ;;
esac
step "Target: $TRIPLE (openssl $SSL_TARGET)"

fetch() {
  local url="$1" file="$SRC/$(basename "$1")"
  [ -f "$file" ] || curl -fsSL --retry 3 -o "$file" "$url"
  echo "$file"
}

if [ -f "$VENDOR/lib/libyaml.a" ]; then
  step "libyaml $YAML_VERSION already built"
else
  step "Building libyaml $YAML_VERSION (static)"
  tar -xzf "$(fetch "https://github.com/yaml/libyaml/releases/download/$YAML_VERSION/yaml-$YAML_VERSION.tar.gz")" -C "$SRC"
  ( cd "$SRC/yaml-$YAML_VERSION"
    ./configure --prefix="$VENDOR" --enable-static --disable-shared --with-pic >/dev/null
    make -j"$JOBS" >/dev/null && make install >/dev/null )
fi

if [ -f "$VENDOR/lib/libssl.a" ] || [ -f "$VENDOR/lib64/libssl.a" ]; then
  step "OpenSSL $OPENSSL_VERSION already built"
else
  step "Building OpenSSL $OPENSSL_VERSION (static) — the long pole, ~20 min"
  tar -xzf "$(fetch "https://github.com/openssl/openssl/releases/download/openssl-$OPENSSL_VERSION/openssl-$OPENSSL_VERSION.tar.gz")" -C "$SRC"
  ( cd "$SRC/openssl-$OPENSSL_VERSION"
    # --libdir=lib matters more than it looks. OpenSSL installs to lib64 on
    # most Linux targets, while Ruby's configure looks in lib. libyaml is built
    # first and creates lib/, so a "symlink lib64 to lib if lib is missing"
    # fallback never fires — and Ruby then finds no static OpenSSL, silently
    # links the system one, and the extension fails at runtime with an
    # undefined symbol. Putting it in lib from the start removes the class.
    ./Configure "$SSL_TARGET" no-shared no-tests no-docs \
      --prefix="$VENDOR" --openssldir="$VENDOR/ssl" --libdir=lib >/dev/null
    make -j"$JOBS" >/dev/null && make install_sw >/dev/null )
  [ -f "$VENDOR/lib/libcrypto.a" ] || { echo "OpenSSL did not install a static libcrypto into $VENDOR/lib"; exit 1; }
fi

if [ -x "$PREFIX/bin/ruby" ]; then
  step "Ruby $RUBY_VERSION already built at $PREFIX"
else
  step "Building Ruby $RUBY_VERSION with --enable-load-relative"
  tar -xzf "$(fetch "https://cache.ruby-lang.org/pub/ruby/${RUBY_VERSION%.*}/ruby-$RUBY_VERSION.tar.gz")" -C "$SRC"
  ( cd "$SRC/ruby-$RUBY_VERSION"
    # PKG_CONFIG_PATH is cleared so configure cannot wander into a package
    # manager's prefix and link something we are not shipping.
    env -u PKG_CONFIG_PATH ./configure \
      --prefix="$PREFIX" \
      --enable-load-relative \
      --disable-install-doc \
      --with-openssl-dir="$VENDOR" \
      --with-libyaml-dir="$VENDOR" \
      --without-gmp \
      --enable-shared=no >/dev/null
    make -j"$JOBS" >/dev/null && make install >/dev/null )
fi

step "Built"
echo "  $("$PREFIX/bin/ruby" -v)"
echo "  $PREFIX  ($(du -sh "$PREFIX" | cut -f1))"
echo "  triple: $TRIPLE"
