#!/bin/sh
# Put a plain 1100x720 @2x screenshot in a macOS-style window on a soft background.
# Usage: scripts/screenshots/frame.sh IN.png OUT.png
set -e
[ $# -eq 2 ] || { echo "usage: $0 IN.png OUT.png" >&2; exit 1; }
here=$(cd "$(dirname "$0")" && pwd)
in=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
out=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new \
  --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
  --window-size=1400,940 --virtual-time-budget=2000 \
  --screenshot="$out" "file://$here/frame.html#file://$in" 2>/dev/null
