# shellcheck shell=bash
# Shared setup for packaging scripts. Source: `. "$(dirname "$0")/../env.sh"`.
# Adapted from SolidCraft packaging (bherbruck/solidcraft; MIT OR Apache-2.0; see ATTRIBUTION.md).
#
# Exports:
#   ROOT, VERSION (semver for nfpm), DIST_TAG (archive names, often with a leading v),
#   DIST, PCBCRAFT_BUILD_DATE, CARGO_TARGET_DIR, APP_ID

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT

workspace_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") ; next }
    in_pkg && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }
  ' "$ROOT/Cargo.toml"
}

DIST_TAG="${PCBCRAFT_DIST_TAG:-${PCBCRAFT_VERSION:-}}"
if [ -z "$DIST_TAG" ]; then
  DIST_TAG="v$(workspace_version)"
fi
export DIST_TAG

VERSION="${PCBCRAFT_VERSION:-$(workspace_version)}"
VERSION="${VERSION#v}"
if [ -z "$VERSION" ]; then
  echo "error: could not read version from Cargo.toml or PCBCRAFT_VERSION" >&2
  exit 1
fi
export VERSION

DIST="${DIST:-$ROOT/dist/release}"
mkdir -p "$DIST"
DIST="$(cd "$DIST" && pwd)"
export DIST

export PCBCRAFT_BUILD_DATE="${PCBCRAFT_BUILD_DATE:-$(date -u +%Y-%m-%d)}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
APP_ID=io.github.tangency-labs.pcbcraft
export APP_ID

warn() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
}

copy_docs() {
  local dest="$1" f
  for f in README.md LICENSE NOTICE ATTRIBUTION.md; do
    if [ -f "$ROOT/$f" ]; then cp "$ROOT/$f" "$dest/"; fi
  done
}

# Copy the generated hicolor icon tree (PNG sizes and the scalable SVG) into
# "$2". The tree is built from the masters in assets/brand by assets/brand/build.sh
# and committed, so packaging needs no rasteriser.
make_icons() {
  local out="$2" src="$ROOT/assets/brand/generated/linux/hicolor"
  if [ ! -d "$src" ]; then
    echo "error: missing generated icons $src (run assets/brand/build.sh)" >&2
    return 1
  fi
  mkdir -p "$out"
  cp -R "$src" "$out/"
}
