#!/usr/bin/env bash
# Build pcbcraft.ico from the Linux SVG source (Vista+ PNG-in-ICO).
# Usage: packaging/windows/make-icon.sh [output-dir]
# Adapted from SolidCraft packaging (bherbruck/solidcraft; MIT OR Apache-2.0; see ATTRIBUTION.md).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$ROOT/assets/icons/app/windows/exports}"
SRC="$ROOT/assets/brand/pcbcraft-icon.svg"
if [ ! -f "$SRC" ]; then
  echo "error: missing icon source $SRC" >&2
  exit 1
fi
if ! command -v rsvg-convert >/dev/null; then
  echo "error: rsvg-convert required (librsvg2-bin)" >&2
  exit 1
fi
if ! command -v magick >/dev/null; then
  echo "error: magick required (imagemagick)" >&2
  exit 1
fi
mkdir -p "$OUT"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
for s in 16 32 48 256; do
  rsvg-convert -w "$s" -h "$s" "$SRC" -o "$TMP/$s.png"
done
magick "$TMP/16.png" "$TMP/32.png" "$TMP/48.png" "$TMP/256.png" "$OUT/pcbcraft.ico"
cp "$OUT/pcbcraft.ico" "$OUT/pcbcraft-executable.ico"
echo "wrote $OUT/pcbcraft.ico"
