#!/bin/bash
# Turn a Rails app plus a relocatable interpreter into a self-contained Linux
# application directory, and a tarball of it.
#
# The same contract as the macOS packer, because the parts that matter are not
# platform-specific: the interpreter must relocate, `rails server` must not be
# used, writable state lives outside the application tree, and the server
# announces itself on stdout and exits when stdin closes.
#
# What differs is only the shape. There is no bundle format and no signing, so
# this produces an ordinary directory that runs from wherever it is unpacked,
# plus a .desktop entry for menu integration. AppImage is built on top when
# appimagetool is available, since that is the format people expect to download.
#
# Usage:
#   packaging/pack-linux.sh --app ../my_rails_app --runtime out/ruby \
#     --gems out/gems --name "Ledger" --app-id dev.example.ledger

set -euo pipefail
cd "$(dirname "$0")/.."
HERE="$PWD/packaging"

APP_SRC=""; RUNTIME=""; GEMS=""; NAME="Turbo Desktop App"
APP_ID="dev.turbodesktop.app"; OUT="$PWD/dist"; KEEP_DEV=0; APPIMAGE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --app)      APP_SRC="$2"; shift 2 ;;
    --runtime)  RUNTIME="$2"; shift 2 ;;
    --gems)     GEMS="$2"; shift 2 ;;
    --name)     NAME="$2"; shift 2 ;;
    --app-id)   APP_ID="$2"; shift 2 ;;
    --out)      OUT="$2"; shift 2 ;;
    --keep-dev) KEEP_DEV=1; shift ;;
    --appimage) APPIMAGE=1; shift ;;
    -h|--help)  sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown option: $1"; exit 1 ;;
  esac
done

[ -d "$APP_SRC" ]           || { echo "--app must be a Rails app directory"; exit 1; }
[ -x "$RUNTIME/bin/ruby" ]  || { echo "--runtime must contain bin/ruby"; exit 1; }
[ -f "$APP_SRC/config.ru" ] || { echo "$APP_SRC has no config.ru — is it a Rails app?"; exit 1; }

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
SLUG="$(echo "$NAME" | tr '[:upper:] ' '[:lower:]-')"
DIR="$OUT/$SLUG"

step "Assembling $SLUG"
rm -rf "$DIR"; mkdir -p "$DIR/lib" "$DIR/bin" "$DIR/share/applications"
cp -R "$RUNTIME" "$DIR/lib/ruby"
[ -n "$GEMS" ] && [ -d "$GEMS" ] && cp -R "$GEMS" "$DIR/lib/gems"
rsync -a --exclude 'tmp/' --exclude 'log/' --exclude '.git/' --exclude 'node_modules/' \
      "$APP_SRC/" "$DIR/lib/app/"
cp "$HERE/templates/boot.rb" "$DIR/lib/app/boot.rb"
echo "  interpreter, gems and app copied"

cat > "$DIR/bin/$SLUG" <<LAUNCH
#!/bin/bash
# Resolve the interpreter beside this script, wherever the tree was unpacked,
# and keep everything writable outside it. The application directory may sit
# somewhere read-only such as /opt.
set -euo pipefail
here="\$(cd "\$(dirname "\$(readlink -f "\${BASH_SOURCE[0]}")")/.." && pwd)"

# XDG rather than a home-directory dotfile, because that is where a Linux
# desktop expects application state to live.
export DESKTOP_DATA_DIR="\${DESKTOP_DATA_DIR:-\${XDG_DATA_HOME:-\$HOME/.local/share}/$APP_ID}"
mkdir -p "\$DESKTOP_DATA_DIR"/{tmp,log,storage}

export GEM_HOME="\$here/lib/gems"
export GEM_PATH="\$GEM_HOME:\$(echo "\$here"/lib/ruby/lib/ruby/gems/*)"
export RAILS_ENV="\${RAILS_ENV:-production}"
export BUNDLE_GEMFILE="\$here/lib/app/Gemfile"
cd "\$here/lib/app"

# Never \`rails server\`: railties creates tmp dirs under Rails.root regardless
# of config.paths, which fails when the tree is read-only.
exec "\$here/lib/ruby/bin/ruby" "\${@:-boot.rb}"
LAUNCH
chmod +x "$DIR/bin/$SLUG"

cat > "$DIR/share/applications/$APP_ID.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$NAME
Exec=$SLUG
Icon=$APP_ID
Categories=Office;
Terminal=false
DESKTOP

step "Pruning"
KEEP_DEV=$KEEP_DEV "$HERE/prune.sh" "$DIR/lib"

step "Packaging"
tar -C "$OUT" -czf "$OUT/$SLUG-linux-$(uname -m).tar.gz" "$SLUG"
echo "  $OUT/$SLUG-linux-$(uname -m).tar.gz ($(du -sh "$OUT/$SLUG-linux-$(uname -m).tar.gz" | cut -f1))"

if [ "$APPIMAGE" = "1" ]; then
  if command -v appimagetool >/dev/null 2>&1; then
    step "AppImage"
    APPDIR="$OUT/$SLUG.AppDir"
    rm -rf "$APPDIR"; cp -R "$DIR" "$APPDIR"
    cp "$DIR/share/applications/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
    ln -sf "bin/$SLUG" "$APPDIR/AppRun"
    appimagetool "$APPDIR" "$OUT/$NAME-$(uname -m).AppImage"
  else
    echo "  appimagetool not installed; skipping (the tarball above is self-contained)"
  fi
fi

step "Result"
printf '  %s  (%s)\n' "$DIR" "$(du -sh "$DIR" | cut -f1)"
