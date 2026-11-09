#!/usr/bin/env bash
# Sign and notarize macOS release artifacts. Secrets come from the environment only.
# usage:
#   sign-macos.sh prepare <stage-dir> <app-bundle> <cli-binary>
#   sign-macos.sh finish <dmg-path>
set -euo pipefail
set +x

if [[ -z "${APPLE_CERTIFICATE:-}" ]]; then
  echo "skipped, no certificate"
  exit 0
fi

keychain="$RUNNER_TEMP/pcbcraft-signing.keychain-db"
keychain_password="${KEYCHAIN_PASSWORD:-pcbcraft-ci-keychain}"

import_certificate() {
  if security find-identity -v -p codesigning "$keychain" 2>/dev/null | grep -q 'Developer ID Application'; then
    return 0
  fi
  cert_path="$RUNNER_TEMP/apple-codesign.p12"
  printf '%s' "$APPLE_CERTIFICATE" | base64 -D >"$cert_path"
  security create-keychain -p "$keychain_password" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$keychain_password" "$keychain"
  security import "$cert_path" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" \
    -T /usr/bin/codesign -T /usr/bin/security
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain"
  existing=$(security list-keychains -d user | sed 's/^[[:space:]]*"//;s/"$//')
  security list-keychains -d user -s "$keychain" $existing
  rm -f "$cert_path"
}

signing_identity() {
  if [[ -n "${APPLE_CODESIGN_IDENTITY:-}" ]]; then
    echo "$APPLE_CODESIGN_IDENTITY"
    return 0
  fi
  security find-identity -v -p codesigning "$keychain" | awk -F'"' '/Developer ID Application/{print $2; exit}'
}

sign_path() {
  identity=$(signing_identity)
  if [[ -z "$identity" ]]; then
    echo "no Developer ID Application identity in keychain" >&2
    exit 1
  fi
  codesign --force --options runtime --timestamp --sign "$identity" "$1"
  codesign --verify --deep --strict "$1"
}

notarize_dmg() {
  dmg=$1
  if [[ -n "${APPLE_NOTARY_KEY:-}" && -n "${APPLE_NOTARY_KEY_ID:-}" && -n "${APPLE_NOTARY_ISSUER_ID:-}" ]]; then
    key_path="$RUNNER_TEMP/notary-key.p8"
    printf '%s' "$APPLE_NOTARY_KEY" | base64 -D >"$key_path"
    xcrun notarytool submit "$dmg" --wait --key "$key_path" --key-id "$APPLE_NOTARY_KEY_ID" \
      --issuer "$APPLE_NOTARY_ISSUER_ID"
    rm -f "$key_path"
  elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    xcrun notarytool submit "$dmg" --wait --apple-id "$APPLE_ID" \
      --password "$APPLE_APP_SPECIFIC_PASSWORD" --team-id "$APPLE_TEAM_ID"
  else
    echo "notarization credentials missing (API key trio or Apple ID + app password + team id)" >&2
    exit 1
  fi
  xcrun stapler staple "$dmg"
  xcrun stapler validate "$dmg"
  spctl --assess --type open --context context:primary-signature -v "$dmg"
}

cmd=${1:?usage: sign-macos.sh prepare|finish ...}
shift

import_certificate

case "$cmd" in
  prepare)
    stage=$1
    app=$2
    cli=$3
    sign_path "$cli"
    sign_path "$app"
    ;;
  finish)
    dmg=$1
    sign_path "$dmg"
    notarize_dmg "$dmg"
    ;;
  *)
    echo "unknown command $cmd" >&2
    exit 1
    ;;
esac
