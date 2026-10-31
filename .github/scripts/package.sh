#!/usr/bin/env bash
# Package release binaries that were already built with `cargo build --release`.
# Used by .github/workflows/build-dist.yml for nightly and tagged builds.
#
# usage: package.sh <platform> <version> <out-dir>
#   platform: linux-x86_64 | linux-aarch64 | windows-x86_64 | macos-universal
#
# Reads binaries from target/<triple>/release. Writes archives into <out-dir>.
set -euo pipefail

platform=$1
version=$2
out=$3
mkdir -p "$out"
out=$(cd "$out" && pwd)

name="pcbcraft-$version-$platform"
stage="$(mktemp -d)/$name"
mkdir -p "$stage"
docs=(LICENSE NOTICE ATTRIBUTION.md README.md)
cp "${docs[@]}" "$stage/"
mkdir -p "$stage/examples"
cp examples/* "$stage/examples/"
# Languages the built-in fonts cannot draw load their font file from the
# fonts/ folder next to the program; the license travels beside the file.
mkdir -p "$stage/fonts"
cp crates/ui-egui/assets/fonts/NotoSansJP-Regular.otf crates/ui-egui/assets/fonts/NotoSansSC-Regular.otf crates/ui-egui/assets/fonts/OFL-NotoSansCJK.txt "$stage/fonts/"

case "$platform" in
  linux-x86_64 | linux-aarch64)
    triple=$([ "$platform" = linux-x86_64 ] && echo x86_64-unknown-linux-gnu || echo aarch64-unknown-linux-gnu)
    install -m 755 "target/$triple/release/pcbcraft" "target/$triple/release/pcbcraft-cli" "$stage/"
    tar -C "$(dirname "$stage")" -czf "$out/$name.tar.gz" "$name"
    ;;
  windows-x86_64)
    triple=x86_64-pc-windows-msvc
    cp "target/$triple/release/pcbcraft.exe" "target/$triple/release/pcbcraft-cli.exe" "$stage/"
    (cd "$(dirname "$stage")" && 7z a -tzip -bso0 "$out/$name.zip" "$name")
    ;;
  macos-universal)
    # One binary for Apple silicon and Intel.
    bin="$stage/bin"
    mkdir -p "$bin"
    for exe in pcbcraft pcbcraft-cli; do
      lipo -create \
        "target/aarch64-apple-darwin/release/$exe" \
        "target/x86_64-apple-darwin/release/$exe" \
        -output "$bin/$exe"
    done
    app="$stage/PCBCraft.app"
    mkdir -p "$app/Contents/MacOS"
    cp "$bin/pcbcraft" "$app/Contents/MacOS/pcbcraft"
    # The app reads fonts from fonts/ next to its executable.
    cp -r "$stage/fonts" "$app/Contents/MacOS/fonts"
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>PCBCraft</string>
  <key>CFBundleDisplayName</key><string>PCBCraft</string>
  <key>CFBundleIdentifier</key><string>${MACOS_BUNDLE_ID:-io.github.tangency_labs.pcbcraft}</string>
  <key>CFBundleExecutable</key><string>pcbcraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version#v}</string>
  <key>CFBundleVersion</key><string>${version#v}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    if [[ "${PCBCRAFT_SIGN:-}" == 1 ]]; then
      bash .github/scripts/sign-macos.sh prepare "$stage" "$app" "$bin/pcbcraft-cli"
    fi
    rm "$bin/pcbcraft"
    tar -C "$(dirname "$stage")" -czf "$out/$name.tar.gz" "$name"
    hdiutil create -quiet -volname "PCBCraft $version" -srcfolder "$stage" -ov -format UDZO "$out/$name.dmg"
    if [[ "${PCBCRAFT_SIGN:-}" == 1 ]]; then
      bash .github/scripts/sign-macos.sh finish "$out/$name.dmg"
    fi
    ;;
  *)
    echo "unknown platform $platform" >&2
    exit 1
    ;;
esac

ls -l "$out"
