#!/usr/bin/env bash
# Usage: ./shoot.sh [page ...]   (default: all pages). Writes screenshots/<page>-<theme>.png
set -e
cd "$(dirname "$0")"
python3 build.py >/dev/null
mkdir -p screenshots
pages=("$@"); [ ${#pages[@]} -eq 0 ] && pages=(desktop launcher quick-settings notifications lockscreen settings switcher overview window-chrome)
for p in "${pages[@]}"; do
  for t in dark light; do
    timeout 90 firefox --headless --screenshot "$PWD/screenshots/$p-$t.png" --window-size=1920,1080 "file://$PWD/$p.html?theme=$t&shot" >/dev/null 2>&1
  done
done
