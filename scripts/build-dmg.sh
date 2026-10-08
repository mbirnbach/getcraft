#!/usr/bin/env bash
# Packs GetCraft.app into a styled DMG (background, icon layout, volume icon).
#   scripts/build-dmg.sh <path/to/GetCraft.app> <output.dmg>
# Needs `dmgbuild` (pip install dmgbuild); runs on macOS only.
set -euo pipefail
cd "$(dirname "$0")/.."

APP="${1:?usage: $0 <GetCraft.app> <output.dmg>}"
OUT="${2:?usage: $0 <GetCraft.app> <output.dmg>}"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# One TIFF holding both resolutions lets Finder pick the sharp one on Retina screens.
tiffutil -cathidpicheck assets/dmg/background.png assets/dmg/background@2x.png -out "$WORK/background.tiff" >/dev/null 2>&1

rm -f "$OUT"
dmgbuild -s scripts/dmg-settings.py \
  -D app="$APP" -D icon=assets/getcraft.icns -D background="$WORK/background.tiff" \
  GetCraft "$OUT"
echo "$OUT"
