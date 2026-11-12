#!/usr/bin/env bash
# Build a Kite AppImage:
#   packaging/appimage/build-appimage.sh [--no-build] [--standalone | --install]
#
# Builds Kite in release mode and bundles it with Qt, QtWebEngine and the KDE
# QML modules it uses into dist/Kite-<version>-<arch>.AppImage, using
# linuxdeploy and linuxdeploy-plugin-qt (downloaded on first use into
# target/appimage-tools, or taken from $LINUXDEPLOY / $LINUXDEPLOY_PLUGIN_QT).
#
# On a terminal, it first asks what to do with the AppImage. --standalone and
# --install answer in advance, and without a terminal it's standalone:
#   standalone  it stays in dist/, for you to place and run
#   install     it's also installed for this user, or replaces the Kite already
#               installed: see install-appimage.sh
#
# Portability: it runs on systems with an older glibc than the build system's by
# carrying its own: see add_own_glibc.
#
# Environment:
#   QMAKE           qmake of the Qt to bundle (default: qmake6)
#   ARCH            target architecture (default: uname -m)
#   CARGO           cargo binary (default: cargo, or ~/.cargo/bin/cargo)
#   KITE_TOOLS_DIR  where linuxdeploy is cached (default: target/appimage-tools)
#   KITE_DIST_DIR   where the AppImage goes (default: dist)
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ARCH=${ARCH:-$(uname -m)}
QMAKE=${QMAKE:-$(command -v qmake6 || command -v qmake)}
CARGO=${CARGO:-$(command -v cargo || echo "$HOME/.cargo/bin/cargo")}
TOOLS=${KITE_TOOLS_DIR:-$ROOT/target/appimage-tools}
WORK=$ROOT/target/appimage
APPDIR=$WORK/AppDir
DIST=${KITE_DIST_DIR:-$ROOT/dist}
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
BUILD=1
ACTION=ask
for arg in "$@"; do
    case $arg in
        --no-build) BUILD=0 ;;
        --standalone) ACTION=standalone ;;
        --install) ACTION=install ;;
        *) echo "usage: $0 [--no-build] [--standalone | --install]" >&2; exit 2 ;;
    esac
done

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }

# ---- what to do with it --------------------------------------------------------
# Before anything else, as the build takes a while.
source "$ROOT/packaging/appimage/install-appimage.sh"
ask_action() {
    local path choice first_action second_action
    if (( ${#INSTALLED[@]} > 0 )); then
        echo "Kite is already installed:"
        for path in "${INSTALLED[@]}"; do
            printf '  %s (%s)\n' "$path" "$(appimage_version "$path")"
        done
        echo "What should happen to the new AppImage?"
        echo "  1) Update: replace what's listed above with $VERSION"
        echo "  2) Standalone: leave it in $DIST, and leave the installed Kite as it is"
        first_action=install second_action=standalone
    else
        echo "What should happen to the new AppImage?"
        echo "  1) Standalone: leave it in $DIST, for you to place and run"
        echo "  2) Install: copy it to $KITE_NEW_PATH, with a desktop file and icon"
        first_action=standalone second_action=install
    fi
    while :; do
        read -r -p "Choice [1]: " choice || { echo; exit 1; }
        case ${choice:-1} in
            1) ACTION=$first_action; return ;;
            2) ACTION=$second_action; return ;;
        esac
    done
}
if [[ $ACTION != standalone ]]; then
    find_installed
    if [[ $ACTION == ask ]]; then
        if [[ -t 0 ]]; then ask_action; else ACTION=standalone; fi
    fi
    [[ $ACTION == install ]] && check_can_install
fi

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
install -m644 "$ROOT/data/io.github.n4n0gh.kite.desktop" "$APPDIR/usr/share/applications/"
install -m644 "$ROOT/data/io.github.n4n0gh.kite.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/"

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
import org.kde.purpose
Item {}
EOF
copy_plugins() { # dir under the Qt plugin path
    [[ -d $QT_PLUGINS/$1 ]] || return 0
    mkdir -p "$APPDIR/usr/plugins/$(dirname "$1")"
    cp -a "$QT_PLUGINS/$1" "$APPDIR/usr/plugins/$1"
}
copy_plugins kf6/kirigami/platform
copy_plugins printsupport
# KDE's look, icons and file dialogs. The system's can't load against the bundled Qt.
copy_plugins platformthemes
copy_plugins styles
# What KDE's file dialog lists folders with: without it, the list stays empty.
if [[ -f $QT_PLUGINS/kf6/kio/kio_file.so ]]; then
    mkdir -p "$APPDIR/usr/plugins/kf6/kio"
    cp -a "$QT_PLUGINS/kf6/kio/kio_file.so" "$APPDIR/usr/plugins/kf6/kio/"
fi
# How KDE's libraries talk to Wayland and X11, the file dialog's among them.
copy_plugins kf6/kwindowsystem
# Sharing (ShareDialog.qml, a plugin's, which the scanner doesn't look at): the ways
# that don't need accounts or other applications' libraries. Running one takes the
# system's purposeprocess, so without KDE on the system sharing just reports failing.
for p in clipboardplugin emailplugin kdeconnectplugin saveasplugin barcodeplugin; do
    [[ -f $QT_PLUGINS/kf6/purpose/$p.so ]] || continue
    mkdir -p "$APPDIR/usr/plugins/kf6/purpose"
    cp -a "$QT_PLUGINS/kf6/purpose/$p.so" "$APPDIR/usr/plugins/kf6/purpose/"
done
[[ -d /usr/share/kf6/purpose ]] && { mkdir -p "$APPDIR/usr/share/kf6"; cp -a /usr/share/kf6/purpose "$APPDIR/usr/share/kf6/"; }
for d in wayland-decoration-client wayland-graphics-integration-client wayland-shell-integration; do copy_plugins "$d"; done

# Started through a symlink, as linuxdeploy would have it, Qt doesn't find qt.conf beside
# the executable, and takes its plugins and QML from where Qt was on the build machine:
# on other distributions, a different Qt's, which it can't load.
# Some distributions (openSUSE) keep ICU's data in a file of its own, and libicudata is
# then a stub. Without that file, pages crash as soon as they format a date, reported
# as "V8 process OOM (DateTimePatternGeneratorCache::CreateGenerator)".
if [[ -d /usr/share/icu ]]; then
    mkdir -p "$APPDIR/usr/share"
    cp -a /usr/share/icu "$APPDIR/usr/share/"
fi
# On a system with an older glibc than this one's, which couldn't load what's bundled,
# Kite runs on a copy of this one, through its loader: see add_own_glibc below. Everywhere
# else it's left out of it, and the system's drivers can need a glibc newer than this.
GLIBC_VERSION=$(getconf GNU_LIBC_VERSION | cut -d' ' -f2)
LOADER=$(basename "$(readelf -l "$BIN" | sed -n 's/.*interpreter: \(.*\)]/\1/p')")
APPRUN=$WORK/AppRun
cat > "$APPRUN" <<EOF
#!/bin/sh
HERE="\$(dirname "\$(readlink -f "\$0")")"
for d in "\$HERE"/usr/share/icu/*; do [ -d "\$d" ] && export ICU_DATA="\$d"; done
# Older than $GLIBC_VERSION, or not telling? KITE_OWN_GLIBC=1 or 0 decides instead, for testing.
own=\${KITE_OWN_GLIBC:-}
if [ -z "\$own" ]; then
    have=\$(getconf GNU_LIBC_VERSION 2>/dev/null | cut -d' ' -f2)
    major=\${have%%.*}; minor=\${have#*.}; minor=\${minor%%.*}
    case "\$major\$minor" in
        ''|*[!0-9]*) own=1 ;;
        *) if [ "\$major" -lt ${GLIBC_VERSION%%.*} ] || { [ "\$major" -eq ${GLIBC_VERSION%%.*} ] && [ "\$minor" -lt ${GLIBC_VERSION#*.} ]; }; then own=1; else own=0; fi ;;
    esac
fi
if [ "\$own" = 1 ]; then
    # (Read by QtWebEngineProcess's own wrapper.)
    export KITE_OWN_GLIBC=1
    exec "\$HERE/usr/glibc/$LOADER" --library-path "\$HERE/usr/glibc:\$HERE/usr/lib" --argv0 "\$HERE/usr/bin/kite" "\$HERE/usr/bin/kite" "\$@"
fi
export KITE_OWN_GLIBC=0
exec "\$HERE/usr/bin/kite" "\$@"
EOF
chmod +x "$APPRUN"

# Some distributions (openSUSE) keep builds of libraries for newer CPUs too, and the system's
# own loader, which the scan asks, picks those on this CPU. Bundled, they'd crash on any CPU
# without what they were built for, so the plain builds go in instead.
use_plain_cpu_builds() {
    local libdir file name variant
    libdir=$(dirname "$(ldd "$BIN" | awk '$1 == "libc.so.6" {print $3}')")
    for file in "$APPDIR"/usr/lib/*; do
        [[ -f $file && ! -L $file ]] || continue
        name=$(basename "$file")
        for variant in "$libdir"/glibc-hwcaps/*/"$name"; do
            [[ -e $variant && -e $libdir/$name ]] || continue
            cp -L --remove-destination "$libdir/$name" "$file"
            break
        done
    done
}

# What Kite runs on with a system too old for it. Beside Kite's other libraries it would be
# found on every system, so it has a folder of its own, only ever named by the loader.
# The loader is then what the system sees as running, and Qt looks for qt.conf beside that.
# It isn't inherited: programs Kite starts use the system's libraries, as they should.
add_own_glibc() {
    local own=$APPDIR/usr/glibc lib libdir
    libdir=$(dirname "$(ldd "$BIN" | awk '$1 == "libc.so.6" {print $3}')")
    mkdir -p "$own"
    cp -L "$libdir/$LOADER" "$own/"
    for lib in "${OWN_GLIBC_LIBS[@]}"; do
        [[ -e $libdir/$lib ]] && cp -L "$libdir/$lib" "$own/"
    done
    # An old system's versions of these are also too old for what's bundled. What's tied
    # to its hardware and session stays the system's, with what those depend on.
    local path name
    for lib in "${OWN_SYSTEM_LIBS[@]}"; do
        [[ -e $libdir/$lib ]] || continue
        cp -L "$libdir/$lib" "$own/"
        while read -r name _ path _; do
            [[ $path == /* && -e $path ]] || continue
            [[ -e $APPDIR/usr/lib/$name || -e $own/$name ]] && continue
            [[ -e $libdir/$name ]] && path=$libdir/$name
            [[ $name =~ ^lib(GL|EGL|GLX|OpenGL|gbm|drm|xcb|X11|Xau|Xdmcp|asound|ffi|bsd|md) ]] && continue
            cp -L "$path" "$own/"
        done < <(ldd "$libdir/$lib")
    done
    cp "$APPDIR/usr/bin/qt.conf" "$own/"
    local process=$APPDIR/usr/libexec/QtWebEngineProcess
    mv "$process" "$process.bin"
    cat > "$process" <<EOF
#!/bin/sh
HERE="\$(dirname "\$(readlink -f "\$0")")"
if [ "\$KITE_OWN_GLIBC" = 1 ]; then
    exec "\$HERE/../glibc/$LOADER" --library-path "\$HERE/../glibc:\$HERE/../lib" --argv0 "\$0" "\$HERE/QtWebEngineProcess.bin" "\$@"
fi
exec "\$HERE/QtWebEngineProcess.bin" "\$@"
EOF
    chmod +x "$process"
}
OWN_GLIBC_LIBS=(libc.so.6 libm.so.6 libmvec.so.1 libdl.so.2 libpthread.so.0 librt.so.1 libutil.so.1 libresolv.so.2 libanl.so.1 libnss_compat.so.2 libgcc_s.so.1 libstdc++.so.6)
OWN_SYSTEM_LIBS=(libharfbuzz.so.0 libfreetype.so.6 libfontconfig.so.1 libfribidi.so.0 libexpat.so.1 libz.so.1 libgmp.so.10 libcom_err.so.2 libICE.so.6 libSM.so.6 libwayland-client.so.0 libwayland-cursor.so.0 libwayland-egl.so.1)

# ---- bundle -------------------------------------------------------------------------
say "Bundling with linuxdeploy"
export QMAKE
export QML_SOURCES_PATHS="$ROOT/crates/kite/qml:$EXTRA_QML"
export EXTRA_PLATFORM_PLUGINS="libqwayland.so"
export LDAI_OUTPUT="$WORK/Kite-$VERSION-$ARCH.AppImage"
export OUTPUT=$LDAI_OUTPUT
# What NSS loads at runtime from beside itself, which the scan can't see. Without it, a
# system that doesn't happen to have them ends Kite at the first HTTPS connection.
nss_modules=()
libdir=$(dirname "$(ldd "$BIN" | awk '$1 == "libnss3.so" {print $3}')")
for lib in libsoftokn3.so libfreebl3.so libfreeblpriv3.so libnssdbm3.so libnssckbi.so; do
    [[ -e $libdir/$lib ]] && nss_modules+=(--library "$libdir/$lib")
done
deps_only=()
for d in "$APPDIR"/usr/plugins/kf6/kirigami/platform "$APPDIR"/usr/plugins/kf6/purpose "$APPDIR"/usr/plugins/kf6/kio "$APPDIR"/usr/plugins/kf6/kwindowsystem "$APPDIR"/usr/plugins/platformthemes "$APPDIR"/usr/plugins/styles "$APPDIR"/usr/plugins/wayland-*; do
    [[ -d $d ]] && deps_only+=(--deploy-deps-only "$d")
done
(cd "$WORK" && "$LINUXDEPLOY" --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/kite" \
    --custom-apprun "$APPRUN" \
    --desktop-file "$APPDIR/usr/share/applications/io.github.n4n0gh.kite.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/scalable/apps/io.github.n4n0gh.kite.svg" \
    "${deps_only[@]}" "${nss_modules[@]}" \
    --plugin qt)
use_plain_cpu_builds
add_own_glibc
(cd "$WORK" && "$LINUXDEPLOY" --appdir "$APPDIR" --output appimage)

mkdir -p "$DIST"
mv -f "$LDAI_OUTPUT" "$DIST/"
say "Done: $DIST/$(basename "$LDAI_OUTPUT")"

if [[ $ACTION == install ]]; then
    say "Installing"
    install_kite "$DIST/$(basename "$LDAI_OUTPUT")"
fi
