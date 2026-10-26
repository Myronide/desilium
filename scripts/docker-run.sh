#!/usr/bin/env bash
# Build and run the Desilium compositor inside a locked-down container.
#
# Usage:
#   scripts/docker-run.sh [--no-build] [--arch] [--net] [compositor args...]
#     --arch  run on an Arch Linux image (pacman packages) instead of Debian
#     --net   attach the container to the bridge network instead of none
#     --sudo  (with --arch) allow passwordless sudo/pacman for the desilium user; this turns off
#             no-new-privileges and the read-only root and adds a few capabilities
#   scripts/docker-run.sh --command kitty
#
# The compositor is a Wayland client of the host session (winit backend), so
# the host Wayland socket is the only host resource it gets. Everything else
# is denied by the flags below.

set -euo pipefail

die() {
    printf 'docker-run: %s\n' "$*" >&2
    exit 1
}

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(dirname -- "$SCRIPT_DIR")"
IMAGE="desilium-dev"
CONTAINER_UID=1000
CONTAINER_GID=1000
CONTAINER_XDG="/tmp/xdg"

build=1
dockerfile="Dockerfile"
network="none"
sudo_mode=0
while (($#)); do
    case "$1" in
        --no-build) build=0 ;;
        # Arch Linux runtime (pacman, NetworkManager): the closest match for a real Arch desktop.
        --arch)
            dockerfile="Dockerfile.arch"
            IMAGE="desilium-dev-arch"
            ;;
        # Container network (bridge) instead of none: enables the Ethernet quick settings.
        --net) network="bridge" ;;
        # Let the container user run sudo/pacman (Arch image only): weakens the isolation below.
        --sudo) sudo_mode=1 ;;
        *) break ;;
    esac
    shift
done

command -v docker >/dev/null 2>&1 || die "'docker' not found in PATH"

# The compositor needs a running host Wayland session to open its window.
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

if ((build)); then
    DOCKER_BUILDKIT=1 docker build \
        -f "$REPO_ROOT/docker/$dockerfile" \
        -t "$IMAGE" \
        "$REPO_ROOT"
fi

# Flag rationale:
#   --rm                throwaway container; -it is added when a terminal is attached
#   --init              tiny init as PID 1: forwards signals and reaps client processes
#   --name              one instance at a time, easy to find or stop
#   --user              matches the unprivileged image user; never root
#   --read-only         no writes to the image filesystem
#   --tmpfs /tmp        the only writable scratch space, discarded on exit
#   --tmpfs /tmp/xdg    private XDG_RUNTIME_DIR where the compositor creates
#                       its own Wayland socket (not visible to the host)
#   -v (socket only)    the single host resource: the host Wayland socket,
#                       reachable at the container path below
#   --device            render node for EGL/GL; no other device nodes
#   --group-add         gives the uid access to the render node (0666 on
#                       the host is not guaranteed in every setup)
#   --cap-drop ALL      no Linux capabilities at all
#   no-new-privileges   setuid binaries cannot raise privileges
#   --network none      no network stack by default; Wayland uses a unix socket,
#                       which still works through the bind mount. --net switches
#                       to the default bridge so Ethernet status can be tested
#   --pids-limit        caps fork bombs and runaway child processes
#   --memory / --cpus   bounds the host impact of a misbehaving build
run_args=(
    --rm
    --init
    --name desilium-dev
    --user "${CONTAINER_UID}:${CONTAINER_GID}"
    --tmpfs /tmp
    --tmpfs "${CONTAINER_XDG}:mode=0700,uid=${CONTAINER_UID},gid=${CONTAINER_GID}"
    -v "${HOST_SOCKET}:${CONTAINER_XDG}/${WAYLAND_DISPLAY}"
    -e "XDG_RUNTIME_DIR=${CONTAINER_XDG}"
    -e "WAYLAND_DISPLAY=${WAYLAND_DISPLAY}"
    -e "RUST_LOG=${RUST_LOG:-info}"
    -e "XDG_SESSION_TYPE=wayland"
    --device "${RENDER_NODE}"
    --group-add "${RENDER_GID}"
    --cap-drop ALL
    --network "${network}"
    --pids-limit 256
    --memory 2g
    --cpus 4
)

if ((sudo_mode)); then
    [[ "$dockerfile" == "Dockerfile.arch" ]] || die "--sudo needs --arch (only that image has sudo)"
    # sudo needs setuid/setgid; pacman as root needs to own and chmod files. The root filesystem
    # stays writable so packages can be installed; it is discarded with the container (--rm).
    printf 'docker-run: --sudo: no-new-privileges and the read-only root are off for this run\n' >&2
    run_args+=(
        --cap-add SETUID --cap-add SETGID --cap-add AUDIT_WRITE
        --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER --cap-add FSETID --cap-add SETFCAP
    )
else
    run_args+=(--read-only --security-opt no-new-privileges)
fi

# Development overrides are forwarded only when set (see docs/headless.md).
for dev_var in DESILIUM_WALLPAPER DESILIUM_DEBUG_SNAP; do
    if [[ -n "${!dev_var:-}" ]]; then
        run_args+=(-e "${dev_var}=${!dev_var}")
    fi
done

# Interactive only when a terminal is attached, so the script also works from CI or tools.
if [[ -t 0 && -t 1 ]]; then
    run_args+=(-it)
fi

# Remaining arguments replace the image CMD (the ENTRYPOINT stays fixed).
exec docker run "${run_args[@]}" "$IMAGE" "$@"
