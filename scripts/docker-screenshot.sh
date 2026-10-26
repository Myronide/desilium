#!/usr/bin/env bash
# Render the Desilium compositor headlessly in a locked-down container and
# write a PNG screenshot to the host. No display or Wayland socket is needed.
#
# Usage:
#   scripts/docker-screenshot.sh [--no-build] [--out screenshots/name.png] [compositor args...]
#   scripts/docker-screenshot.sh --out screenshots/foot.png --command "foot -e sh -c 'echo hi; sleep 5'"
#
# Extra arguments go to the compositor (--size, --scale, --screenshot-delay,
# repeatable --command, ...). --headless and --screenshot are added here.

set -euo pipefail

die() {
    printf 'docker-screenshot: %s\n' "$*" >&2
    exit 1
}

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(dirname -- "$SCRIPT_DIR")"
IMAGE="${DESILIUM_IMAGE:-desilium-dev}"
CONTAINER_NAME="desilium-shot-$$"
CONTAINER_UID=1000
CONTAINER_GID=1000
CONTAINER_XDG="/tmp/xdg"
CONTAINER_OUT="/out"
OUT="screenshots/shot.png"

build=1
compositor_args=()
while (($# > 0)); do
    case "$1" in
        --no-build) build=0 ;;
        --out)
            (($# >= 2)) || die "--out needs a file path"
            OUT="$2"
            shift
            ;;
        *) compositor_args+=("$1") ;;
    esac
    shift
done

command -v docker >/dev/null 2>&1 || die "'docker' not found in PATH"

case "$OUT" in
    *.png) ;;
    *) die "--out must end in .png: $OUT" ;;
esac
OUT_DIR="$(dirname -- "$OUT")"
OUT_FILE="$(basename -- "$OUT")"
# Only the output directory is writable from inside; it must allow uid 1000.
mkdir -p -- "$OUT_DIR"
OUT_DIR="$(cd -- "$OUT_DIR" && pwd -P)"
chmod 0777 -- "$OUT_DIR"
rm -f -- "$OUT_DIR/$OUT_FILE"

shopt -s nullglob
render_nodes=(/dev/dri/renderD*)
shopt -u nullglob
((${#render_nodes[@]} > 0)) || die "no /dev/dri/renderD* node found (Mesa/GPU access needed)"
RENDER_NODE="${render_nodes[0]}"
RENDER_GID="$(stat -c %g "$RENDER_NODE")"

if ((build)); then
    DOCKER_BUILDKIT=1 docker build \
        -f "$REPO_ROOT/docker/Dockerfile" \
        -t "$IMAGE" \
        "$REPO_ROOT"
fi

# Same hardening as scripts/docker-run.sh, minus the host Wayland socket,
# plus a single writable bind mount for the screenshot directory.
run_args=(
    --rm
    --init
    --name "$CONTAINER_NAME"
    --user "${CONTAINER_UID}:${CONTAINER_GID}"
    --read-only
    --tmpfs /tmp
    --tmpfs "${CONTAINER_XDG}:mode=0700,uid=${CONTAINER_UID},gid=${CONTAINER_GID}"
    -v "${OUT_DIR}:${CONTAINER_OUT}"
    -e "XDG_RUNTIME_DIR=${CONTAINER_XDG}"
    -e "RUST_LOG=${RUST_LOG:-info}"
    -e "XDG_SESSION_TYPE=wayland"
    --device "${RENDER_NODE}"
    --group-add "${RENDER_GID}"
    --cap-drop ALL
    --security-opt no-new-privileges
    --network none
    --pids-limit 256
    --memory 2g
    --cpus 4
)

# Development overrides are forwarded only when set (see docs/headless.md).
for dev_var in DESILIUM_WALLPAPER DESILIUM_DEBUG_SNAP DESILIUM_DEBUG_POPUP DESILIUM_DEBUG_GLASS DESILIUM_DEBUG_LAYER_BLUR; do
    if [[ -n "${!dev_var:-}" ]]; then
        run_args+=(-e "${dev_var}=${!dev_var}")
    fi
done

# Arguments replace the image CMD; without --command the default CMD is lost,
# so default to foot like the interactive image does.
has_command=0
for arg in "${compositor_args[@]}"; do
    if [[ "$arg" == "--command" || "$arg" == "-c" ]]; then
        has_command=1
    fi
done
((has_command)) || compositor_args+=(--command foot)

status=0
docker run "${run_args[@]}" "$IMAGE" \
    --headless --screenshot "${CONTAINER_OUT}/${OUT_FILE}" "${compositor_args[@]}" || status=$?

if ((status != 0)); then
    die "compositor exited with status $status"
fi
[[ -s "$OUT_DIR/$OUT_FILE" ]] || die "no screenshot was written"
printf 'screenshot: %s\n' "$OUT_DIR/$OUT_FILE"
