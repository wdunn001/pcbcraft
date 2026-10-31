#!/usr/bin/env bash
# Installs the window-system packages the desktop app links against, plus any
# extra packages named as arguments. Does nothing when everything is present, so
# a runner image that already carries them needs no root access.
set -euo pipefail

packages="libxkbcommon-dev libwayland-dev libx11-dev libxi-dev libxrandr-dev libxcursor-dev libgl1-mesa-dev $*"
missing=""
for package in $packages; do
  if ! dpkg -s "$package" >/dev/null 2>&1; then
    missing="$missing $package"
  fi
done
if [ -z "$missing" ]; then
  echo "All packages present."
  exit 0
fi
echo "Installing:$missing"
if [ "$(id -u)" = 0 ]; then
  sudo=""
else
  sudo="sudo"
fi
# shellcheck disable=SC2086 # the package list is a word list
$sudo apt-get update
# shellcheck disable=SC2086
$sudo apt-get install -y --no-install-recommends $missing
