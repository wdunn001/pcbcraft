#!/usr/bin/env bash
# Repackage a PCBCraft Linux tarball as a snap:
#
#   $DIST/pcbcraft-<dist-tag>-linux-<arch>.snap
#
# Usage: packaging/linux/snap-pack.sh [--destructive-mode] [TARBALL]
#
# TARBALL defaults to $DIST/pcbcraft-<dist-tag>-linux-<arch>.tar.gz from package.sh. The snap is
# built for the host architecture (x86_64 or aarch64), which must match the tarball's. Needs
# snapcraft (`sudo snap install snapcraft --classic`) and, without --destructive-mode, LXD.
# --destructive-mode builds on the host itself, which must be Ubuntu 24.04 (the core24 base),
# as in CI.
# Manifest: packaging/linux/snap/snapcraft.yaml.
# Adapted from SolidCraft packaging (bherbruck/solidcraft; MIT OR Apache-2.0; see ATTRIBUTION.md).
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"

MODE=(--use-lxd)
TARBALL=""
while [ $# -gt 0 ]; do
  case "$1" in
    --destructive-mode) MODE=(--destructive-mode); shift ;;
    -h | --help) sed -n '2,14p' "$0"; exit 0 ;;
    -*) echo "unknown argument: $1" >&2; exit 2 ;;
    *) TARBALL="$1"; shift ;;
  esac
done

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) SNAP_ARCH=amd64 ;;
  aarch64 | arm64) ARCH=aarch64; SNAP_ARCH=arm64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
BASENAME="pcbcraft-$DIST_TAG-linux-$ARCH"
TARBALL="${TARBALL:-$DIST/$BASENAME.tar.gz}"
[ -f "$TARBALL" ] || { echo "error: $TARBALL not found (run packaging/linux/package.sh --formats tar first)" >&2; exit 1; }
command -v snapcraft >/dev/null || { echo "error: snapcraft not found (sudo snap install snapcraft --classic)" >&2; exit 1; }
if ! printf '%s' "$VERSION" | grep -Eq '^[A-Za-z0-9][A-Za-z0-9.+~-]{0,31}$'; then
  echo "error: version $VERSION is not a valid snap version" >&2
  exit 1
fi

echo "==> PCBCraft $DIST_TAG snap for $SNAP_ARCH from $(basename "$TARBALL")"

WORK="$CARGO_TARGET_DIR/snap-pack"
rm -rf "$WORK"
mkdir -p "$WORK/snap" "$WORK/stage"
tar -xzf "$TARBALL" -C "$WORK/stage" --strip-components=1
[ -x "$WORK/stage/bin/pcbcraft" ] || { echo "error: $TARBALL has no bin/pcbcraft" >&2; exit 1; }
cp "$HERE/snap/snapcraft.yaml" "$WORK/snap/snapcraft.yaml"
printf '%s\n' "$VERSION" >"$WORK/version.txt"

(cd "$WORK" && snapcraft pack "${MODE[@]}" --build-for="$SNAP_ARCH" --output="$WORK/out.snap")

OUT="$DIST/$BASENAME.snap"
mv "$WORK/out.snap" "$OUT"
echo "wrote $OUT"
ls -lh "$OUT"
