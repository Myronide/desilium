#!/usr/bin/env bash
# End-to-end check of the live effect against the real compositor (needs Docker and a render node).
#
# Two headless compositor runs share the same recipe: the compositor watches --config-dir, the
# settings app is open on the same directory. In the second run a second `desilium-settings`
# (no window) changes the accent after two seconds, exactly like clicking a swatch would.
# The compositor must reload the file by itself (inotify, no IPC): its title bar border and the
# open settings window change colour, so the two screenshots must differ.
#
# Usage: crates/desilium-settings/tests/e2e/live-accent.sh [outdir]   (default: docs/screenshots)

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../.." && pwd -P)"
OUT_DIR="${1:-$REPO_ROOT/docs/screenshots}"
export DESILIUM_IMAGE="${DESILIUM_IMAGE:-desilium-settings}"
CFG=/tmp/cfg
APP="desilium-settings --config-dir $CFG --size 1100x760"
DELAY=4500
cd "$REPO_ROOT"
mkdir -p "$OUT_DIR"

echo "== before: accent untouched"
scripts/docker-screenshot.sh --out "$OUT_DIR/settings-live-before.png" --size 1280x800 \
    --config-dir "$CFG" --screenshot-delay "$DELAY" --command "$APP" 2>&1 | tee "$OUT_DIR/.live-before.log" >/dev/null

echo "== after: accent changed from a second process while everything runs"
scripts/docker-screenshot.sh --no-build --out "$OUT_DIR/settings-live-after.png" --size 1280x800 \
    --config-dir "$CFG" --screenshot-delay "$DELAY" --command "$APP" \
    --command "sh -c 'sleep 2.5; desilium-settings --config-dir $CFG --set appearance.accent=#E8348A --no-window'" \
    2>&1 | tee "$OUT_DIR/.live-after.log" >/dev/null

if grep -qi "rejected\|ignoring shortcut\|cannot" "$OUT_DIR/.live-after.log"; then
    echo "compositor reported a problem with the configuration:" >&2
    grep -i "rejected\|ignoring shortcut\|cannot" "$OUT_DIR/.live-after.log" >&2
    exit 1
fi
if cmp -s "$OUT_DIR/settings-live-before.png" "$OUT_DIR/settings-live-after.png"; then
    echo "FAIL: the screenshots are identical, the change did not reach the compositor" >&2
    exit 1
fi
rm -f "$OUT_DIR/.live-before.log" "$OUT_DIR/.live-after.log"
echo "OK: the compositor and the open settings window picked up the new accent"
