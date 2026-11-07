#!/usr/bin/env bash
# Install or update Kite from an AppImage, for the current user, without root:
#   packaging/appimage/install-appimage.sh <Kite.AppImage>
#
# Without an installed Kite, the AppImage goes to ~/.local/bin/kite, with a desktop
# file and an icon, which is what puts Kite in the application menu and among the
# choices for default browser.
#
# With one, each Kite AppImage found is replaced where it is, so whatever points at it
# keeps working. A file is only replaced after it's been shown to be a Kite AppImage:
# looking for them is in find_installed. Kite can keep running during an update, and
# gets the new version when it's next started.
#
# build-appimage.sh sources this file, and asks what to do before building.
#
# Environment:
#   KITE_INSTALL_PATH  where a first install goes (default: ~/.local/bin/kite)
#   XDG_DATA_HOME      where the desktop file and icon go (default: ~/.local/share)

KITE_SOURCE_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
KITE_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}
KITE_APPS_DIR=$KITE_DATA_HOME/applications
KITE_ICON_THEME=$KITE_DATA_HOME/icons/hicolor
KITE_OWN_ENTRY=$KITE_APPS_DIR/org.kde.kite.desktop
KITE_NEW_PATH=$(realpath -m "${KITE_INSTALL_PATH:-$HOME/.local/bin/kite}")

# What find_installed finds: the AppImages, as real paths, and the desktop files that
# start them, with what each one starts at the same index.
INSTALLED=()
ENTRIES=()
ENTRY_TARGETS=()

# An AppImage whose contents include Kite's desktop file. The runtime lists that
# without running Kite and without FUSE.
is_kite_appimage() { # absolute path
    [[ -f $1 && -x $1 ]] || return 1
    [[ $(od -An -tx1 -j8 -N3 "$1" 2>/dev/null) == ' 41 49 02' ]] || return 1
    local dir status=1
    dir=$(mktemp -d)
    (cd "$dir" && timeout 30 "$1" --appimage-extract org.kde.kite.desktop) </dev/null >/dev/null 2>&1 || true
    [[ -L $dir/squashfs-root/org.kde.kite.desktop || -e $dir/squashfs-root/org.kde.kite.desktop ]] && status=0
    rm -rf "$dir"
    return $status
}

# The program a desktop file starts.
desktop_program() { # desktop file
    local line
    line=$(grep -m1 '^Exec=' "$1") || return 1
    line=${line#Exec=}
    if [[ $line == \"* ]]; then
        line=${line#\"}
        printf '%s\n' "${line%%\"*}"
    else
        printf '%s\n' "${line%% *}"
    fi
}

add_installed() { # path
    local real known
    real=$(readlink -f "$1") || return 1
    for known in "${INSTALLED[@]}"; do
        [[ $known == "$real" ]] && { printf '%s\n' "$real"; return 0; }
    done
    is_kite_appimage "$real" || return 1
    INSTALLED+=("$real")
    printf '%s\n' "$real"
}

# Looks where people and tools like AppImageLauncher keep AppImages, and at what the
# desktop files of this user start. A copy somewhere else, as in Downloads or dist,
# isn't considered installed.
find_installed() {
    INSTALLED=() ENTRIES=() ENTRY_TARGETS=()
    local restore_nullglob dir file program real
    restore_nullglob=$(shopt -p nullglob || true)
    shopt -s nullglob
    local candidates=("$KITE_NEW_PATH")
    for dir in "$HOME/.local/bin" "$HOME/bin" "$HOME/Applications" "$HOME/AppImages" "$HOME/.appimages"; do
        candidates+=("$dir"/[Kk]ite "$dir"/[Kk]ite.[Aa]pp[Ii]mage "$dir"/[Kk]ite[-_]*)
    done
    local entries=("$KITE_APPS_DIR"/*.desktop)
    eval "$restore_nullglob"

    for file in "${candidates[@]}"; do
        [[ -e $file ]] || continue
        # A subshell can't add to the list, so this isn't $(add_installed ...).
        add_installed "$file" >/dev/null || true
    done
    for file in "${entries[@]}"; do
        grep -qi kite "$file" 2>/dev/null || continue
        program=$(desktop_program "$file") || continue
        [[ $program == /* && -e $program ]] || continue
        add_installed "$program" >/dev/null || continue
        real=$(readlink -f "$program")
        ENTRIES+=("$file")
        ENTRY_TARGETS+=("$real")
    done
}

# The version in an AppImage, from Kite itself. It takes FUSE, so it can be unknown.
appimage_version() { # path
    local out
    out=$(env -u APPIMAGE_EXTRACT_AND_RUN timeout 10 "$1" --version </dev/null 2>/dev/null) || true
    [[ $out == "Kite "* ]] && printf '%s\n' "${out#Kite }" || printf 'unknown version\n'
}

# Refuses what would fail at the end of a long build, or replace something else.
check_can_install() {
    local path
    if (( ${#INSTALLED[@]} == 0 )); then
        case $KITE_NEW_PATH in
            *[\"\`\$\\%]*)
                echo "kite: can't name $KITE_NEW_PATH in a desktop file; choose another KITE_INSTALL_PATH" >&2
                return 1 ;;
        esac
        if [[ -e $KITE_NEW_PATH || -L $KITE_NEW_PATH ]]; then
            echo "kite: $KITE_NEW_PATH exists and isn't a Kite AppImage, so it won't be replaced;" >&2
            echo "      move it, or choose another path with KITE_INSTALL_PATH" >&2
            return 1
        fi
        path=$(dirname "$KITE_NEW_PATH")
        mkdir -p "$path" || return 1
        [[ -w $path ]] || { echo "kite: can't write to $path" >&2; return 1; }
    fi
    for path in "${INSTALLED[@]}"; do
        [[ -w $(dirname "$path") ]] || { echo "kite: can't write to $(dirname "$path"), so $path can't be updated" >&2; return 1; }
    done
}

# Through a new file, so nothing is left half written, and a running Kite isn't in the way.
put_appimage() { # new, destination
    local part=$2.part.$$
    if ! cp "$1" "$part" || ! chmod 755 "$part" || ! mv -f "$part" "$2"; then
        rm -f "$part"
        echo "kite: couldn't write $2" >&2
        return 1
    fi
}

write_desktop_entry() { # program
    local exec=$1 part=$KITE_OWN_ENTRY.part.$$ line
    [[ $exec == *' '* ]] && exec="\"$exec\""
    mkdir -p "$KITE_APPS_DIR" "$KITE_ICON_THEME/scalable/apps"
    while IFS= read -r line; do
        case $line in
            Exec=kite*) printf 'Exec=%s%s\n' "$exec" "${line#Exec=kite}" ;;
            *) printf '%s\n' "$line" ;;
        esac
    done <"$KITE_SOURCE_ROOT/data/org.kde.kite.desktop" >"$part"
    chmod 644 "$part"
    mv -f "$part" "$KITE_OWN_ENTRY"
    install -m644 "$KITE_SOURCE_ROOT/data/org.kde.kite.svg" "$KITE_ICON_THEME/scalable/apps/"
}

# Tells the desktop about the new files. Each of these is only on some systems, and
# none of them is required: the desktop would find out by itself, only later.
refresh_desktop() {
    {
        command -v update-desktop-database && update-desktop-database -q "$KITE_APPS_DIR"
        [[ -f $KITE_ICON_THEME/icon-theme.cache ]] && gtk-update-icon-cache -q -t -f "$KITE_ICON_THEME"
        command -v kbuildsycoca6 && kbuildsycoca6
    } </dev/null >/dev/null 2>&1 || true
}

# Needs find_installed to have run.
install_kite() { # new AppImage
    local new=$1 path own_target='' i
    check_can_install || return 1

    if (( ${#INSTALLED[@]} == 0 )); then
        put_appimage "$new" "$KITE_NEW_PATH" || return 1
        INSTALLED=("$KITE_NEW_PATH")
        printf 'Installed %s\n' "$KITE_NEW_PATH"
    else
        for path in "${INSTALLED[@]}"; do
            put_appimage "$new" "$path" || return 1
            printf 'Replaced %s\n' "$path"
        done
    fi

    # A desktop file from another tool, like AppImageLauncher, still starts the right
    # file, and a second one of ours would only list Kite twice.
    for i in "${!ENTRIES[@]}"; do
        [[ ${ENTRIES[$i]} == "$KITE_OWN_ENTRY" ]] && own_target=${ENTRY_TARGETS[$i]}
    done
    if [[ -n $own_target || ${#ENTRIES[@]} == 0 ]]; then
        write_desktop_entry "${own_target:-${INSTALLED[0]}}"
        printf 'Wrote %s\n' "$KITE_OWN_ENTRY"
    else
        printf 'Left the existing desktop file alone: %s\n' "${ENTRIES[0]}"
    fi
    refresh_desktop
    printf 'Kite can now be chosen as the default browser in the system settings. A running Kite updates when restarted.\n'
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    set -euo pipefail
    if [[ $# != 1 || ! -f $1 ]]; then
        echo "usage: $0 <Kite.AppImage>" >&2
        exit 2
    fi
    new=$(readlink -f "$1")
    is_kite_appimage "$new" || { echo "kite: $1 isn't a Kite AppImage" >&2; exit 1; }
    find_installed
    install_kite "$new"
fi
