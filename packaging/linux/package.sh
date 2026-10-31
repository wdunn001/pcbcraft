#!/usr/bin/env bash
# Build and package PCBCraft for Linux (x86_64 or aarch64):
#
#   $DIST/pcbcraft-<dist-tag>-linux-<arch>.AppImage
#   $DIST/pcbcraft-<dist-tag>-linux-<arch>.AppImage.zsync
#   $DIST/pcbcraft-<dist-tag>-linux-<arch>.deb
#   $DIST/pcbcraft-<dist-tag>-linux-<arch>.rpm
#   $DIST/pcbcraft-<dist-tag>-linux-<arch>.tar.gz   (optional FHS tree; CI keeps legacy tar via .github/scripts/package.sh)
#
# Usage: packaging/linux/package.sh [--skip-build] [--formats "appimage deb rpm tar"]
#
# Adapted from SolidCraft packaging (bherbruck/solidcraft; MIT OR Apache-2.0; see ATTRIBUTION.md).
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"

SKIP_BUILD=0
FORMATS="appimage deb rpm"
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --formats) FORMATS="$2"; shift 2 ;;
    -h | --help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="${CROSS_ARCH:-$(uname -m)}"
case "$ARCH" in
  x86_64) DEB_ARCH=amd64; LINUX_TARGET="${LINUX_TARGET:-x86_64-unknown-linux-gnu}" ;;
  aarch64 | arm64) ARCH=aarch64; DEB_ARCH=arm64; LINUX_TARGET="${LINUX_TARGET:-aarch64-unknown-linux-gnu}" ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
export PCBCRAFT_MAINTAINER="${PCBCRAFT_MAINTAINER:-PCBCraft contributors <https://github.com/wdunn001/pcbcraft>}"
export PCBCRAFT_BUILD_DATE
BASENAME="pcbcraft-$DIST_TAG-linux-$ARCH"

echo "==> PCBCraft $DIST_TAG (semver $VERSION) for Linux $ARCH ($FORMATS)"

if [ "$SKIP_BUILD" = 0 ]; then
  if [ -n "${CROSS_TARGET:-}" ]; then
    (cd "$ROOT" && cargo build --release --locked -p pcbcraft -p pcbcraft-cli --target "$CROSS_TARGET")
    BIN="$CARGO_TARGET_DIR/$CROSS_TARGET/release"
  else
    (cd "$ROOT" && cargo build --release --locked -p pcbcraft -p pcbcraft-cli --target "$LINUX_TARGET")
    BIN="$CARGO_TARGET_DIR/$LINUX_TARGET/release"
  fi
else
  if [ -n "${CROSS_TARGET:-}" ]; then
    BIN="$CARGO_TARGET_DIR/$CROSS_TARGET/release"
  else
    BIN="$CARGO_TARGET_DIR/$LINUX_TARGET/release"
  fi
fi
WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/root"
rm -rf "$WORK"

install -Dm755 "$BIN/pcbcraft" "$STAGE/usr/bin/pcbcraft"
install -Dm755 "$BIN/pcbcraft-cli" "$STAGE/usr/bin/pcbcraft-cli"
strip "$STAGE/usr/bin/pcbcraft" "$STAGE/usr/bin/pcbcraft-cli" 2>/dev/null || true
install -Dm644 "$ROOT/assets/platforms/linux/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
install -Dm644 "$ROOT/assets/platforms/linux/$APP_ID.mime.xml" "$STAGE/usr/share/mime/packages/$APP_ID.xml"
# Languages the built-in fonts cannot draw load their font file at
# runtime; the app finds it at ../share/pcbcraft/fonts from /usr/bin.
for font in NotoSansJP-Regular.otf NotoSansSC-Regular.otf OFL-NotoSansCJK.txt; do
  install -Dm644 "$ROOT/crates/ui-egui/assets/fonts/$font" "$STAGE/usr/share/pcbcraft/fonts/$font"
done
mkdir -p "$STAGE/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$PCBCRAFT_BUILD_DATE/g" \
  "$ROOT/assets/platforms/linux/$APP_ID.metainfo.xml.in" >"$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
make_icons "$STAGE/usr/bin/pcbcraft-cli" "$WORK/icons"
mkdir -p "$STAGE/usr/share/icons"
cp -R "$WORK/icons/hicolor" "$STAGE/usr/share/icons/"
mkdir -p "$STAGE/usr/share/doc/pcbcraft"
copy_docs "$STAGE/usr/share/doc/pcbcraft"

if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$STAGE/usr/share/applications/$APP_ID.desktop"
fi
if command -v appstreamcli >/dev/null; then
  appstreamcli validate --no-net --explain "$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
fi

has() { case " $FORMATS " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

if has tar; then
  mkdir -p "$WORK/tar"
  cp -R "$STAGE/usr" "$WORK/tar/$BASENAME"
  tar -C "$WORK/tar" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
  echo "wrote $DIST/$BASENAME.tar.gz"
fi

if has deb || has rpm; then
  command -v nfpm >/dev/null || { echo "error: nfpm not found (https://nfpm.goreleaser.com/install/)" >&2; exit 1; }
  export VERSION
  export NFPM_ARCH="$DEB_ARCH"
  sed "s|\${STAGE}|$STAGE|g" "$HERE/nfpm.yaml" >"$WORK/nfpm.yaml"
  for fmt in deb rpm; do
    if has "$fmt"; then (cd "$ROOT" && nfpm package -f "$WORK/nfpm.yaml" -p "$fmt" -t "$DIST/$BASENAME.$fmt"); fi
  done
fi

if has appimage; then
  APPDIR="$WORK/PCBCraft.AppDir"
  cp -R "$STAGE" "$APPDIR"
  mv "$APPDIR/usr/share/doc" "$WORK/doc-unused"
  install -Dm755 "$HERE/AppRun" "$APPDIR/AppRun"
  cp "$ROOT/assets/platforms/linux/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
  cp "$WORK/icons/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/$APP_ID.png"
  ln -s "$APP_ID.png" "$APPDIR/.DirIcon"

  TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
  if [ -z "$TOOL" ]; then
    TOOL="$CARGO_TARGET_DIR/appimagetool-$ARCH.AppImage"
    if [ ! -x "$TOOL" ]; then
      curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
      chmod +x "$TOOL"
    fi
  fi
  OUT="$DIST/$BASENAME.AppImage"
  APPDIR="$(cd "$APPDIR" && pwd)"
  TOOL="$(cd "$(dirname "$TOOL")" && pwd)/$(basename "$TOOL")"
  rm -f "$OUT.zsync"
  REPO="${GITHUB_REPOSITORY:-tangency-labs/pcbcraft}"
  UPDATE_INFO="gh-releases-zsync|${REPO%%/*}|${REPO#*/}|latest|pcbcraft-*-linux-$ARCH.AppImage.zsync"
  (cd "$DIST" && ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream -u "$UPDATE_INFO" "$APPDIR" "$OUT")
  echo "wrote $OUT"
  if [ -s "$OUT.zsync" ]; then
    echo "wrote $OUT.zsync"
  else
    warn "zsyncmake not found, so $OUT.zsync was not written; AppImage delta updates need it"
  fi
fi

"$STAGE/usr/bin/pcbcraft-cli" help >/dev/null
echo "==> done"
ls -lh "$DIST"
