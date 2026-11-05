#!/usr/bin/env bash
# Build a Kite AppImage: packaging/appimage/build-appimage.sh [--no-build]
#
# Builds Kite in release mode and bundles it with Qt, QtWebEngine and the KDE
# QML modules it uses into dist/Kite-<version>-<arch>.AppImage, using
# linuxdeploy and linuxdeploy-plugin-qt (downloaded on first use into
# target/appimage-tools, or taken from $LINUXDEPLOY / $LINUXDEPLOY_PLUGIN_QT).
#
# Portability: an AppImage runs on systems whose glibc is at least as new as
# the build system's. Build on an old distribution (or in a container of
# one) to support more systems.
#
# Environment:
#   QMAKE           qmake of the Qt to bundle (default: qmake6)
#   ARCH            target architecture (default: uname -m)
#   CARGO           cargo binary (default: cargo, or ~/.cargo/bin/cargo)
#   KITE_TOOLS_DIR  where linuxdeploy is cached (default: target/appimage-tools)
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ARCH=${ARCH:-$(uname -m)}
QMAKE=${QMAKE:-$(command -v qmake6 || command -v qmake)}
CARGO=${CARGO:-$(command -v cargo || echo "$HOME/.cargo/bin/cargo")}
TOOLS=${KITE_TOOLS_DIR:-$ROOT/target/appimage-tools}
WORK=$ROOT/target/appimage
APPDIR=$WORK/AppDir
DIST=$ROOT/dist
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
BUILD=1
[[ ${1:-} == --no-build ]] && BUILD=0

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }

QT_PLUGINS=$("$QMAKE" -query QT_INSTALL_PLUGINS)
QT_QML=$("$QMAKE" -query QT_INSTALL_QML)

# ---- tools -------------------------------------------------------------------
fetch() { # name url
    local dest=$TOOLS/$1
    if [[ ! -x $dest ]]; then
        say "Downloading $1" >&2
        mkdir -p "$TOOLS"
        curl -fL --retry 3 -o "$dest.part" "$2"
        chmod +x "$dest.part"
        mv "$dest.part" "$dest"
    fi
    echo "$dest"
}
LD_URL=https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage
LDQT_URL=https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-$ARCH.AppImage
LINUXDEPLOY=${LINUXDEPLOY:-$(fetch "linuxdeploy-$ARCH.AppImage" "$LD_URL")}
LINUXDEPLOY_PLUGIN_QT=${LINUXDEPLOY_PLUGIN_QT:-$(fetch "linuxdeploy-plugin-qt-$ARCH.AppImage" "$LDQT_URL")}
# The AppImage runtime, fetched here rather than by appimagetool (whose own
# download can hang).
RUNTIME_URL=https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-$ARCH
export LDAI_RUNTIME_FILE=${LDAI_RUNTIME_FILE:-$(fetch "runtime-$ARCH" "$RUNTIME_URL")}
# linuxdeploy finds plugins named linuxdeploy-plugin-* next to itself or on PATH.
ln -sf "$LINUXDEPLOY_PLUGIN_QT" "$TOOLS/linuxdeploy-plugin-qt"
export PATH=$TOOLS:$PATH
# Work without FUSE (containers, CI).
export APPIMAGE_EXTRACT_AND_RUN=1

# ---- build -----------------------------------------------------------------------
if (( BUILD )); then
    say "Building Kite $VERSION (release)"
    (cd "$ROOT" && "$CARGO" build --release --locked -p kite)
fi
BIN=$ROOT/target/release/kite
[[ -x $BIN ]] || { echo "missing $BIN (build without --no-build first)" >&2; exit 1; }

# ---- AppDir ------------------------------------------------------------------------
say "Preparing AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/scalable/apps"
install -m755 "$BIN" "$APPDIR/usr/bin/kite"
install -m644 "$ROOT/data/org.kde.kite.desktop" "$APPDIR/usr/share/applications/"
install -m644 "$ROOT/data/org.kde.kite.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/"

# Things Kite loads at runtime that no QML file imports, so the QML scanner
# can't see them: the org.kde.desktop Qt Quick Controls style (set in
# cpp/bootstrap.cpp) and Kirigami's matching platform plugin; and the
# Wayland client plugins.
EXTRA_QML=$WORK/extra-qml
mkdir -p "$EXTRA_QML"
cat > "$EXTRA_QML/RuntimeImports.qml" <<'EOF'
import QtQuick
import org.kde.desktop
import org.kde.qqc2desktopstyle.private
Item {}
EOF
copy_plugins() { # dir under the Qt plugin path
    [[ -d $QT_PLUGINS/$1 ]] || return 0
    mkdir -p "$APPDIR/usr/plugins/$(dirname "$1")"
    cp -a "$QT_PLUGINS/$1" "$APPDIR/usr/plugins/$1"
}
copy_plugins kf6/kirigami/platform
copy_plugins printsupport
for d in wayland-decoration-client wayland-graphics-integration-client wayland-shell-integration; do copy_plugins "$d"; done

# ---- bundle -------------------------------------------------------------------------
say "Bundling with linuxdeploy"
export QMAKE
export QML_SOURCES_PATHS="$ROOT/crates/kite/qml:$EXTRA_QML"
export EXTRA_PLATFORM_PLUGINS="libqwayland.so"
export LDAI_OUTPUT="$WORK/Kite-$VERSION-$ARCH.AppImage"
export OUTPUT=$LDAI_OUTPUT
deps_only=()
for d in "$APPDIR"/usr/plugins/kf6/kirigami/platform "$APPDIR"/usr/plugins/wayland-*; do
    [[ -d $d ]] && deps_only+=(--deploy-deps-only "$d")
done
(cd "$WORK" && "$LINUXDEPLOY" --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/kite" \
    --desktop-file "$APPDIR/usr/share/applications/org.kde.kite.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/scalable/apps/org.kde.kite.svg" \
    "${deps_only[@]}" \
    --plugin qt --output appimage)

mkdir -p "$DIST"
mv -f "$LDAI_OUTPUT" "$DIST/"
say "Done: $DIST/$(basename "$LDAI_OUTPUT")"
