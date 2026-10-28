#!/usr/bin/env bash
# Build and run a Desilium session (KWin, shell, bridge, notifications) inside a locked-down
# container, and manage the persistent development user space of that container.
#
# Usage:
#   scripts/docker-run.sh [options] [desilium-session args...]
#
# Distro (one image, one home volume and one pseudo folder per distro; the last flag wins):
#   (default)  Debian   docker/Dockerfile          image desilium-dev          pseudo-debian/
#   --arch     Arch     docker/Dockerfile.arch     image desilium-dev-arch     pseudo-arch/
#   --ubu      Ubuntu   docker/Dockerfile.ubuntu   image desilium-dev-ubuntu   pseudo-ubuntu/
#   --fed      Fedora   docker/Dockerfile.fedora   image desilium-dev-fedora   pseudo-fedora/
#
# Options:
#   --width N      width of the nested KWin window (default 1600), with --height N (default 900)
#   --no-build     reuse the existing image instead of running `docker build` first
#   --net          attach the container to the bridge network instead of none
#   --sudo         (Arch only) turn on passwordless sudo/pacman for the dev user; this turns off
#                  no-new-privileges and the read-only root and adds a few capabilities
#   --no-sudo      with --arch: keep the fully locked-down run (sudo then cannot work)
#   --fresh-home   use a throw-away home: an anonymous Docker volume at the home directory,
#                  initialised from the image (seeded skeleton included) and removed with the
#                  container (--rm); nothing persists
#   --reset-home   delete the home volume of the chosen distro (refused while a container uses it)
#                  and exit; with desilium-session arguments after it, continue and start a
#                  session on a fresh volume
#   --save         keep the WHOLE system, the root filesystem "/" including the home and everything
#                  installed into it, in pseudo-<distro>/system.tar in the repository: if that file
#                  exists the session starts from it, and when the session ends the container is
#                  written back to it (docker commit + docker save; no bind mount, the file belongs
#                  to you). The root filesystem is writable for such a run and the home is part of
#                  the saved system. Without --save the file is neither read nor written
#   --check        smoke test, no display needed: builds the image (unless --no-build) and checks
#                  inside the locked-down container that desilium-session --dry-run works, that
#                  kwin_wayland, kwriteconfig6 and kglobalacceld exist, that the home skeleton and
#                  the icon themes are there; PASS/FAIL per check, non-zero exit on any FAIL
#
# Examples:
#   scripts/docker-run.sh --allow-any-client -- --width 1280 --height 720
#   DESILIUM_WIDTH=1920 DESILIUM_HEIGHT=1080 scripts/docker-run.sh
#   scripts/docker-run.sh --ubu --check
#   scripts/docker-run.sh --ubu --save --width 2650 --height 1440
#
# Environment:
#   DESILIUM_USER          name of the container user, default dev (home /home/<name>; passed to
#                          docker build as USER_NAME, so build and run agree)
#   DESILIUM_HOME_VOLUME   name of the home volume, default desilium-dev-home-<distro>
#   DESILIUM_WIDTH / DESILIUM_HEIGHT   size of the nested KWin window, default 1600x900
#
# Without arguments the session runs with --allow-any-client and a 1600x900 KWin window (see
# DESILIUM_WIDTH/DESILIUM_HEIGHT). Arguments replace that default and go to desilium-session;
# after a literal `--` they go to kwin_wayland. KWin runs as a client of the host Wayland session (its
# nested Wayland backend, chosen because WAYLAND_DISPLAY points at the host), so the host Wayland
# socket is the only host resource it gets. Everything else is denied by the flags below.
#
# The user space: the container user `dev` has the home /home/dev with the usual folders
# (Applications, Desktop, Documents, ...), seeded from docker/home-skel. The home is a NAMED DOCKER
# VOLUME. Docker fills a new volume from the image's /home/dev (content and ownership) on first
# use, and everything done afterwards persists between runs. The images have not been built in the
# authoring environment; none of this has been run.

set -euo pipefail

die() {
    printf 'docker-run: %s\n' "$*" >&2
    exit 1
}

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(dirname -- "$SCRIPT_DIR")"
CONTAINER_UID=1000
CONTAINER_GID=1000
CONTAINER_USER="${DESILIUM_USER:-dev}"
CONTAINER_HOME="/home/${CONTAINER_USER}"
CONTAINER_XDG="/tmp/xdg"

[[ "$CONTAINER_USER" =~ ^[a-z_][a-z0-9_-]*$ ]] || die "invalid DESILIUM_USER: $CONTAINER_USER"

# distro name, Dockerfile and image per flag
distro="debian"
dockerfile="Dockerfile"
IMAGE="desilium-dev"
set_distro() {
    distro="$1"
    case "$1" in
        debian) dockerfile="Dockerfile" IMAGE="desilium-dev" ;;
        arch) dockerfile="Dockerfile.arch" IMAGE="desilium-dev-arch" ;;
        ubuntu) dockerfile="Dockerfile.ubuntu" IMAGE="desilium-dev-ubuntu" ;;
        fedora) dockerfile="Dockerfile.fedora" IMAGE="desilium-dev-fedora" ;;
        *) die "unknown distro: $1" ;;
    esac
}

build=1
network="none"
sudo_mode=""
fresh_home=0
reset_home=0
save=0
action=""
set_action() {
    action="$1"
}
while (($#)); do
    case "$1" in
        --no-build) build=0 ;;
        # Arch Linux runtime (pacman, NetworkManager): the closest match for a real Arch desktop.
        --arch) set_distro arch ;;
        --ubu) set_distro ubuntu ;;
        --fed) set_distro fedora ;;
        # Container network (bridge) instead of none: enables the Ethernet quick settings.
        --net) network="bridge" ;;
        # sudo/pacman for the container user (Arch image only): weakens the isolation below.
        # On by default with --arch; --no-sudo restores the fully locked-down run.
        --sudo) sudo_mode=1 ;;
        --no-sudo) sudo_mode=0 ;;
        --fresh-home) fresh_home=1 ;;
        --reset-home) reset_home=1 ;;
        --save) save=1 ;;
        --check) set_action check ;;
        # Size of the nested KWin window (same as DESILIUM_WIDTH / DESILIUM_HEIGHT).
        --width) [[ $# -ge 2 && "$2" =~ ^[0-9]+$ ]] || die "--width needs a number"
            DESILIUM_WIDTH="$2"; shift ;;
        --height) [[ $# -ge 2 && "$2" =~ ^[0-9]+$ ]] || die "--height needs a number"
            DESILIUM_HEIGHT="$2"; shift ;;
        *) break ;;
    esac
    shift
done

# sudo defaults to on for Arch (the image has sudo), off for everything else.
if [[ -z "$sudo_mode" ]]; then
    if [[ "$distro" == "arch" ]]; then sudo_mode=1; else sudo_mode=0; fi
fi

# The home volume: the ONLY writable, host-persistent location of the container. It is a
# Docker-managed volume (under /var/lib/docker/volumes), not a host path; --save syncs its content
# with the repository folder pseudo-<distro>/.
HOME_VOLUME="${DESILIUM_HOME_VOLUME:-desilium-dev-home-${distro}}"
PSEUDO_DIR="$REPO_ROOT/pseudo-${distro}"

command -v docker >/dev/null 2>&1 || die "'docker' not found in PATH"

build_image() {
    DOCKER_BUILDKIT=1 docker build \
        -f "$REPO_ROOT/docker/$dockerfile" \
        --build-arg "USER_NAME=${CONTAINER_USER}" \
        --build-arg "USER_UID=${CONTAINER_UID}" \
        --build-arg "USER_GID=${CONTAINER_GID}" \
        -t "$IMAGE" \
        "$REPO_ROOT"
}

# $1 = always: build unless --no-build. $1 = if-missing: build only when the image is absent
# (kept for helpers that only need `tar` from the image).
ensure_image() {
    if [[ "$1" == "always" ]]; then
        if ((build)); then build_image; fi
    elif ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
        ((build)) || die "image $IMAGE does not exist (and --no-build is set)"
        build_image
    fi
    docker image inspect "$IMAGE" >/dev/null 2>&1 || die "image $IMAGE does not exist"
}

volume_exists() {
    docker volume inspect "$HOME_VOLUME" >/dev/null 2>&1
}

# Containers (running or stopped) that still reference the home volume.
volume_users() {
    docker ps -a --filter "volume=${HOME_VOLUME}" --format '{{.Names}}'
}

require_volume_unused() {
    local users
    users="$(volume_users)"
    [[ -z "$users" ]] || die "volume $HOME_VOLUME is in use by: $(printf '%s' "$users" | tr '\n' ' ')(stop/remove them first)"
}

# Flags shared by the helper containers (the --save sync and --check): same lockdown as the session.
helper_args=(
    --rm
    --user "${CONTAINER_UID}:${CONTAINER_GID}"
    --read-only
    --cap-drop ALL
    --security-opt no-new-privileges
    --network none
)

if [[ -n "$action" ]] && (($# > 0)); then
    die "--$action takes no session arguments (got: $*)"
fi

if ((save && fresh_home)); then
    die "--save and --fresh-home exclude each other (a throw-away home has nothing to sync)"
fi
if ((save)) && [[ -n "$action" ]]; then
    die "--save belongs to a session run, not to --$action"
fi

# --reset-home: drop the persistent home of this distro.
if ((reset_home)); then
    if volume_exists; then
        require_volume_unused
        docker volume rm "$HOME_VOLUME" >/dev/null
        printf 'docker-run: reset: removed volume %s (distro %s); the next start re-seeds it from the image\n' \
            "$HOME_VOLUME" "$distro"
    else
        printf 'docker-run: reset: volume %s does not exist, nothing to remove\n' "$HOME_VOLUME"
    fi
    if [[ -z "$action" ]] && ((!save)) && (($# == 0)); then
        exit 0
    fi
fi

# --save keeps the WHOLE system of this distro, the complete root filesystem "/" with the home
# and everything installed into it, in pseudo-<distro>/system.tar. The session runs in a container
# that is not thrown away; when it ends, the container is committed to an image and that image is
# written to the tar (`docker save`, so the file belongs to you and no host directory is mounted
# into the container). The next --save run loads the tar and continues from that state.
# Without --save the file is neither read nor written.
SAVED_IMAGE="${IMAGE}-saved"
SAVE_FILE="$PSEUDO_DIR/system.tar"
CONTAINER_NAME="desilium-dev"
# Docker images are limited to about 125 layers and every save adds one.
LAYER_WARNING=100

# Container -> image -> pseudo-<distro>/system.tar; then the container is removed.
save_system() {
    docker container inspect "$CONTAINER_NAME" >/dev/null 2>&1 || return 0
    printf 'docker-run: saving the whole system of the container to %s ...\n' "$SAVE_FILE" >&2
    if ! docker commit "$CONTAINER_NAME" "$SAVED_IMAGE" >/dev/null; then
        printf 'docker-run: committing the container failed; it is kept as %s\n' "$CONTAINER_NAME" >&2
        return 1
    fi
    mkdir -p -- "$PSEUDO_DIR"
    local tmp_file="$PSEUDO_DIR/.system.$$.tar"
    if ! docker save -o "$tmp_file" "$SAVED_IMAGE"; then
        rm -f -- "$tmp_file"
        printf 'docker-run: writing %s failed; the container is kept as %s\n' "$SAVE_FILE" "$CONTAINER_NAME" >&2
        return 1
    fi
    mv -f -- "$tmp_file" "$SAVE_FILE"
    docker rm "$CONTAINER_NAME" >/dev/null 2>&1 || true
    local layers
    layers="$(docker image inspect --format '{{len .RootFS.Layers}}' "$SAVED_IMAGE" 2>/dev/null || echo 0)"
    printf 'docker-run: saved %s (%s, %s layers)\n' "$SAVE_FILE" "$(du -sh -- "$SAVE_FILE" | cut -f1)" "$layers" >&2
    if ((layers >= LAYER_WARNING)); then
        printf 'docker-run: warning: %s layers; Docker allows about 125. Delete %s to start again from the image\n' \
            "$layers" "$PSEUDO_DIR" >&2
    fi
}

# pseudo-<distro>/system.tar -> image $SAVED_IMAGE, before the session starts.
load_system() {
    docker load -i "$SAVE_FILE" >/dev/null || die "loading $SAVE_FILE failed"
    docker image inspect "$SAVED_IMAGE" >/dev/null 2>&1 \
        || die "$SAVE_FILE does not contain the image $SAVED_IMAGE"
}

# --check: smoke test of the image without a display. The script below runs under `sh` inside the
# image (it must not contain single quotes). kwin_wayland carries a file capability, hence SYS_NICE.
if [[ "$action" == "check" ]]; then
    ensure_image always
    check_script='
fail=0
indent() { while IFS= read -r l; do printf "        %s\n" "$l"; done; }
check() {
    desc=$1; shift
    out=$("$@" 2>&1); rc=$?
    if [ "$rc" -eq 0 ]; then printf "PASS  %s\n" "$desc"; else printf "FAIL  %s\n" "$desc"; fail=1; fi
    if [ -n "$out" ]; then printf "%s\n" "$out" | indent; fi
}
locate() {
    p=$(command -v "$1" 2>/dev/null)
    if [ -z "$p" ]; then
        p=$(find /usr/bin /usr/lib /usr/libexec /usr/local -name "$1" -print -quit 2>/dev/null)
    fi
    [ -n "$p" ] && printf "%s\n" "$p"
}
plan_ok() {
    plan=$(desilium-session --dry-run) || { printf "%s\n" "$plan"; return 1; }
    printf "%s\n" "$plan"
    for b in kwin_wayland desilium-shell desilium-kwin; do
        printf "%s\n" "$plan" | grep -q "$b" || { echo "not in the plan: $b"; return 1; }
    done
}
kwin_version() { k=$(locate kwin_wayland) || return 1; "$k" --version; }
kwrite_version() { k=$(locate kwriteconfig6) || return 1; echo "$k"; "$k" --version; }
skeleton_ok() {
    for d in Applications Desktop Documents Downloads Music Pictures Public Templates Videos Pictures/Wallpapers; do
        [ -d "$HOME/$d" ] || { echo "missing folder: $HOME/$d"; return 1; }
    done
    for f in Documents/Welcome.txt .config/user-dirs.locale; do
        [ -f "$HOME/$f" ] || { echo "missing file: $HOME/$f"; return 1; }
    done
    for v in DESKTOP DOWNLOAD TEMPLATES PUBLICSHARE DOCUMENTS MUSIC PICTURES VIDEOS; do
        grep -q "^XDG_${v}_DIR=" "$HOME/.config/user-dirs.dirs" || { echo "user-dirs.dirs lacks XDG_${v}_DIR"; return 1; }
    done
    echo "home: $HOME ($(id -un), uid $(id -u))"
}
launchers_ok() {
    for f in org.desilium.Doria.desktop desilium-settings.desktop; do
        [ -x "$HOME/Desktop/$f" ] || { echo "not executable: $HOME/Desktop/$f"; return 1; }
    done
}
cursors_ok() {
    d=$(find /usr/share/icons -type d -name cursors -print -quit 2>/dev/null)
    [ -n "$d" ] || { echo "no cursor theme below /usr/share/icons: KWin would show no pointer"; return 1; }
    echo "cursor theme: $d"
}
not_root() { [ "$(id -u)" -ne 0 ]; }
icons_ok() {
    test -f /usr/share/icons/breeze/index.theme || { echo "missing /usr/share/icons/breeze/index.theme"; return 1; }
    test -f /usr/share/icons/hicolor/index.theme || { echo "missing /usr/share/icons/hicolor/index.theme"; return 1; }
}

check "runs as an unprivileged user" not_root
check "desilium-session --dry-run lists kwin_wayland, desilium-shell, desilium-kwin" plan_ok
check "kwin_wayland --version" kwin_version
check "kwriteconfig6 --version" kwrite_version
check "kglobalacceld is installed" locate kglobalacceld
check "home skeleton, user-dirs.dirs" skeleton_ok
check "desktop launchers are executable" launchers_ok
check "icon themes breeze and hicolor" icons_ok
check "a cursor theme is installed" cursors_ok

echo
if [ "$fail" -eq 0 ]; then echo "ALL CHECKS PASSED"; else echo "SOME CHECKS FAILED"; fi
exit "$fail"
'
    printf 'docker-run: --check: distro %s, image %s\n' "$distro" "$IMAGE"
    exec docker run "${helper_args[@]}" \
        --init \
        --tmpfs /tmp \
        --cap-add SYS_NICE \
        -e "XDG_RUNTIME_DIR=/tmp" \
        -e "QT_QPA_PLATFORM=offscreen" \
        --entrypoint sh "$IMAGE" -c "$check_script"
fi

# ---------------------------------------------------------------------------
# Session run
# ---------------------------------------------------------------------------

# KWin needs a running host Wayland session to open its window.
[[ -n "${WAYLAND_DISPLAY:-}" ]] || die "WAYLAND_DISPLAY is not set (no host Wayland session?)"
[[ -n "${XDG_RUNTIME_DIR:-}" ]] || die "XDG_RUNTIME_DIR is not set (no host Wayland session?)"

HOST_SOCKET="$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"
[[ -S "$HOST_SOCKET" ]] || die "host Wayland socket not found: $HOST_SOCKET"

# Pick the first render node. Only the render node is passed through; card*
# nodes would give the container modesetting rights, which it does not need.
shopt -s nullglob
render_nodes=(/dev/dri/renderD*)
shopt -u nullglob
((${#render_nodes[@]} > 0)) || die "no /dev/dri/renderD* node found (Mesa/GPU access needed)"
RENDER_NODE="${render_nodes[0]}"
RENDER_GID="$(stat -c %g "$RENDER_NODE")"

run_image="$IMAGE"
if ((save)); then
    if docker container inspect "$CONTAINER_NAME" >/dev/null 2>&1; then
        die "the container $CONTAINER_NAME still exists (an earlier --save run did not finish). Its state is not lost: \`docker commit $CONTAINER_NAME $SAVED_IMAGE && docker save -o $SAVE_FILE $SAVED_IMAGE\`, or \`docker rm -f $CONTAINER_NAME\` to drop it"
    fi
    if [[ -f "$SAVE_FILE" ]]; then
        printf 'docker-run: loading the saved system %s\n' "$SAVE_FILE" >&2
        load_system
        run_image="$SAVED_IMAGE"
    else
        printf 'docker-run: no saved system yet (%s), starting from the image; it is saved when the session ends\n' \
            "$SAVE_FILE" >&2
        ensure_image always
    fi
else
    ensure_image always
fi

# Flag rationale:
#   --rm                throwaway container; -it is added when a terminal is attached
#   --init              tiny init as PID 1: forwards signals and reaps client processes
#   --name              one instance at a time, easy to find or stop
#   --user              matches the unprivileged image user; never root
#   --read-only         no writes to the image filesystem
#   --tmpfs /tmp        scratch space, discarded on exit
#   --tmpfs /tmp/xdg    private XDG_RUNTIME_DIR where the compositor creates
#                       its own Wayland socket (not visible to the host)
#   -v VOLUME:HOME      the home of the dev user: a named Docker volume, the only writable
#                       location that outlives the container. It is Docker-managed storage, NOT
#                       a host path; a new volume is initialised from the image's home (the
#                       seeded skeleton, with ownership). --fresh-home uses an anonymous volume
#                       (-v HOME) instead, which --rm deletes with the container.
#   -v (socket only)    the single host resource: the host Wayland socket,
#                       reachable at the container path below
#   --device            render node for EGL/GL; no other device nodes
#   --group-add         gives the uid access to the render node (0666 on
#                       the host is not guaranteed in every setup)
#   --cap-drop ALL      no Linux capabilities, except SYS_NICE for kwin_wayland (see below)
#   no-new-privileges   setuid binaries cannot raise privileges
#   --network none      no network stack by default; Wayland uses a unix socket,
#                       which still works through the bind mount. --net switches
#                       to the default bridge so Ethernet status can be tested
#   (no --pids-limit, --memory or --cpus: the container runs without resource limits)
run_args=(
    --init
    --name "${CONTAINER_NAME}"
    --user "${CONTAINER_UID}:${CONTAINER_GID}"
    --tmpfs /tmp
    --tmpfs "${CONTAINER_XDG}:mode=0700,uid=${CONTAINER_UID},gid=${CONTAINER_GID}"
    # The host socket gets a name that is not wayland-N and is addressed by absolute path: KWin
    # creates its own wayland-0 in the same directory and fails with "Address already in use"
    # when the host's wayland-0 is mounted there.
    -v "${HOST_SOCKET}:${CONTAINER_XDG}/host-compositor"
    -e "XDG_RUNTIME_DIR=${CONTAINER_XDG}"
    -e "WAYLAND_DISPLAY=${CONTAINER_XDG}/host-compositor"
    -e "RUST_LOG=${RUST_LOG:-info}"
    -e "XDG_SESSION_TYPE=wayland"
    --device "${RENDER_NODE}"
    --group-add "${RENDER_GID}"
    --cap-drop ALL
    # kwin_wayland carries the file capability cap_sys_nice+ep (realtime scheduling). A file
    # capability outside the container's bounding set makes execve fail with EPERM, so this one
    # capability has to stay.
    --cap-add SYS_NICE
    --network "${network}"
    # Docker's default /dev/shm is 64 MB, too small for browsers (Firefox exits at startup).
    --shm-size 1g
    -e "MOZ_ENABLE_WAYLAND=1"
)

# --save keeps the container (no --rm) so that it can be committed afterwards; every other run is
# thrown away when it ends.
((save)) || run_args+=(--rm)

# The home: with --save it is part of the container's own filesystem and so of the saved system.
# Otherwise a persistent named volume, or a throw-away anonymous one (--fresh-home). Both are
# initialised by Docker from the image's home directory on first use.
if ((save)); then
    printf 'docker-run: home: inside the container (saved with the whole system, distro %s)\n' \
        "$distro" >&2
elif ((fresh_home)); then
    run_args+=(-v "${CONTAINER_HOME}")
    printf 'docker-run: home: fresh anonymous volume at %s (initialised from the image, deleted on exit)\n' \
        "$CONTAINER_HOME" >&2
else
    run_args+=(-v "${HOME_VOLUME}:${CONTAINER_HOME}")
    printf 'docker-run: home: volume %s at %s (distro %s; persistent, reset with --reset-home)\n' \
        "$HOME_VOLUME" "$CONTAINER_HOME" "$distro" >&2
fi

if ((sudo_mode)); then
    [[ "$distro" == "arch" ]] || die "--sudo needs --arch (only that image has sudo)"
    # sudo needs setuid/setgid; pacman as root needs to own and chmod files. The root filesystem
    # stays writable so packages can be installed; it is discarded with the container (--rm).
    # The home volume is mounted the same way as without sudo.
    printf 'docker-run: --sudo: no-new-privileges and the read-only root are off for this run\n' >&2
    run_args+=(
        --cap-add SETUID --cap-add SETGID --cap-add AUDIT_WRITE
        --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER --cap-add FSETID --cap-add SETFCAP
    )
elif ((save)); then
    # The root filesystem has to be writable or there would be nothing to save besides /tmp.
    printf 'docker-run: --save: the root filesystem is writable for this run\n' >&2
    run_args+=(--security-opt no-new-privileges)
else
    run_args+=(--read-only --security-opt no-new-privileges)
fi

# Interactive only when a terminal is attached, so the script also works from CI or tools.
if [[ -t 0 && -t 1 ]]; then
    run_args+=(-it)
fi

# Without arguments the session starts with a fixed window size. A host compositor that draws no
# decorations for Wayland clients (GNOME's Mutter does not implement xdg-decoration) shows the
# nested KWin window without a title bar, and KWin's nested backend has no fallback of its own, so
# the window cannot be resized with the mouse. The size is DESILIUM_WIDTH x DESILIUM_HEIGHT.
if (($# == 0)); then
    set -- --allow-any-client -- \
        --width "${DESILIUM_WIDTH:-1600}" --height "${DESILIUM_HEIGHT:-900}"
fi

# Remaining arguments replace the image CMD (the ENTRYPOINT stays fixed).
if ((!save)); then
    exec docker run "${run_args[@]}" "$run_image" "$@"
fi

# --save: the session must not be exec'ed, the whole system is saved once it has ended.
rc=0
docker run "${run_args[@]}" "$run_image" "$@" || rc=$?
save_system || rc=$?
exit "$rc"
