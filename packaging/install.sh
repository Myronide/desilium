#!/usr/bin/env bash
# Installs Desilium's binaries, session file, portal preferences, desktop entries and wallpapers.
#
# Usage:
#   packaging/install.sh [--prefix DIR] [--target-dir DIR] [--dry-run] [--uninstall]
#   packaging/install.sh --help
#
# Environment:
#   PREFIX        install prefix (default /usr/local); same as --prefix
#   DESTDIR       staging root prepended to every destination, for packagers
#   CARGO_TARGET_DIR  where `cargo build --release` wrote its output (default: <repo>/target)
#
# This script is never run by the build or by the session. Packagers run it from the
# PKGBUILD or debian rules; users run it by hand. Use --dry-run first.

set -euo pipefail

die() {
    printf 'install.sh: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'USAGE'
usage: packaging/install.sh [--prefix DIR] [--target-dir DIR] [--dry-run] [--uninstall]

  --prefix DIR       install under DIR (default: $PREFIX or /usr/local)
  --target-dir DIR   cargo target directory holding release/ (default: $CARGO_TARGET_DIR or <repo>/target)
  --dry-run          print every operation, change nothing
  --uninstall        remove the files this script installs
  -h, --help         show this help

DESTDIR is honoured for staged installs, e.g. DESTDIR=/tmp/stage packaging/install.sh --prefix /usr
USAGE
}

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(dirname -- "$SCRIPT_DIR")"

PREFIX="${PREFIX:-/usr/local}"
DESTDIR="${DESTDIR:-}"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
DRY_RUN=0
ACTION=install

BINARIES=(
    desilium-session
    desilium-kwin
    desilium-shell
    desilium-settings
    desilium-notifications
    doria
)

while (($#)); do
    case "$1" in
        --prefix)
            (($# >= 2)) || die "--prefix requires a directory"
            PREFIX="$2"
            shift 2
            ;;
        --target-dir)
            (($# >= 2)) || die "--target-dir requires a directory"
            TARGET_DIR="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --uninstall)
            ACTION=uninstall
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            die "unknown argument: $1 (see --help)"
            ;;
    esac
done

[[ "$PREFIX" == /* ]] || die "prefix must be an absolute path: $PREFIX"
command -v install >/dev/null 2>&1 || die "'install' (coreutils) not found in PATH"

# Each entry is "MODE|SOURCE|DESTINATION"; MODE "dir" creates a directory instead.
# The separator is not whitespace on purpose: an empty SOURCE must not collapse into the next field.
ENTRIES=()
ENTRY_SEPARATOR='|'

add_entry() {
    ENTRIES+=("$1${ENTRY_SEPARATOR}$2${ENTRY_SEPARATOR}$3")
}

plan_entries() {
    local bin_dir="$DESTDIR$PREFIX/bin"
    local share="$DESTDIR$PREFIX/share"
    local name file

    for name in "${BINARIES[@]}"; do
        add_entry 0755 "$TARGET_DIR/release/$name" "$bin_dir/$name"
    done
    add_entry 0644 "$SCRIPT_DIR/wayland-sessions/desilium.desktop" "$share/wayland-sessions/desilium.desktop"
    add_entry 0644 "$SCRIPT_DIR/xdg-desktop-portal/desilium-portals.conf" "$share/xdg-desktop-portal/desilium-portals.conf"
    add_entry 0644 "$SCRIPT_DIR/applications/org.desilium.Shell.desktop" "$share/applications/org.desilium.Shell.desktop"
    add_entry 0644 "$SCRIPT_DIR/applications/desilium-settings.desktop" "$share/applications/desilium-settings.desktop"
    add_entry 0644 "$SCRIPT_DIR/applications/org.desilium.Doria.desktop" "$share/applications/org.desilium.Doria.desktop"

    shopt -s nullglob
    for file in "$REPO_ROOT"/assets/wallpapers/*; do
        [[ -f "$file" && "$(basename -- "$file")" != README* ]] || continue
        add_entry 0644 "$file" "$share/desilium/wallpapers/$(basename -- "$file")"
    done
    for file in "$SCRIPT_DIR"/icons/*.svg "$SCRIPT_DIR"/icons/*.png; do
        add_entry 0644 "$file" "$share/icons/hicolor/scalable/apps/$(basename -- "$file")"
    done
    shopt -u nullglob
    add_entry dir "" "$share/icons/hicolor/scalable/apps"
}

# Refuses to install when a binary is missing; a dry run only reports it.
check_sources() {
    local mode src dest missing=0
    for entry in "${ENTRIES[@]}"; do
        IFS="$ENTRY_SEPARATOR" read -r mode src dest <<<"$entry"
        [[ "$mode" == dir || -f "$src" ]] && continue
        missing=$((missing + 1))
        printf 'install.sh: missing source: %s\n' "$src" >&2
    done
    if ((missing > 0)); then
        if ((DRY_RUN)); then
            printf 'install.sh: %d source(s) missing; a real install would stop here (build first)\n' "$missing" >&2
            return 0
        fi
        die "$missing source file(s) missing; run: cargo build --release --locked -p desilium-session -p desilium-kwin -p desilium-shell -p desilium-settings -p desilium-notifications -p desilium-files"
    fi
}

run() {
    if ((DRY_RUN)); then
        printf '+'
        printf ' %q' "$@"
        printf '\n'
        return 0
    fi
    "$@"
}

do_install() {
    local mode src dest
    for entry in "${ENTRIES[@]}"; do
        IFS="$ENTRY_SEPARATOR" read -r mode src dest <<<"$entry"
        if [[ "$mode" == dir ]]; then
            run install -d -m 0755 "$dest"
        else
            run install -D -m "$mode" "$src" "$dest"
        fi
    done
}

do_uninstall() {
    local mode src dest
    for entry in "${ENTRIES[@]}"; do
        IFS="$ENTRY_SEPARATOR" read -r mode src dest <<<"$entry"
        [[ "$mode" == dir ]] && continue
        run rm -f -- "$dest"
    done
    # Directories are removed only when empty; nothing else is ever deleted.
    local dir
    for dir in "$DESTDIR$PREFIX/share/desilium/wallpapers" "$DESTDIR$PREFIX/share/desilium"; do
        if ((DRY_RUN)); then
            printf '+ rmdir --ignore-fail-on-non-empty %q\n' "$dir"
        else
            rmdir --ignore-fail-on-non-empty -- "$dir" 2>/dev/null || true
        fi
    done
}

main() {
    plan_entries
    if ((DRY_RUN)); then
        printf 'install.sh: dry run, nothing is written (%s)\n' "$ACTION"
    fi
    printf 'install.sh: prefix=%s destdir=%s target=%s\n' "$PREFIX" "${DESTDIR:-/}" "$TARGET_DIR"
    if [[ "$ACTION" == install ]]; then
        check_sources
        do_install
    else
        do_uninstall
    fi
}

main
