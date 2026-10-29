#!/usr/bin/env bash
# Idempotent Cloud Agent bootstrap for PCBCraft. Mirrors CI package and toolchain setup.
set -euo pipefail

LINUX_DEPS=(
  libxkbcommon-dev libwayland-dev libx11-dev libxi-dev libxrandr-dev
  libxcursor-dev libgl1-mesa-dev libxkbcommon-x11-0
)
RUNTIME_DEPS=(xvfb xauth mesa-vulkan-drivers libvulkan1 imagemagick)
BUILD_DEPS=(build-essential pkg-config curl ca-certificates git)

if ! dpkg -s "${LINUX_DEPS[0]}" >/dev/null 2>&1; then
  sudo apt-get update
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    "${BUILD_DEPS[@]}" "${LINUX_DEPS[@]}" "${RUNTIME_DEPS[@]}"
fi

if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
fi

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

for bin in cargo rustc rustup; do
  if [[ -x "$HOME/.cargo/bin/$bin" ]] && [[ ! -e "/usr/local/bin/$bin" ]]; then
    sudo ln -sf "$HOME/.cargo/bin/$bin" "/usr/local/bin/$bin"
  fi
done

if ! command -v rustup >/dev/null; then
  echo "rustup install failed" >&2
  exit 1
fi

MSRV="$(sed -n 's/^rust-version *= *"\(.*\)"/\1/p' Cargo.toml)"
if [[ -z "$MSRV" ]]; then
  echo "workspace Cargo.toml has no rust-version" >&2
  exit 1
fi

if ! rustup toolchain list -v | grep -q "^${MSRV}-"; then
  rustup toolchain install "${MSRV}" --profile minimal -c rustfmt -c clippy
fi
rustup default "${MSRV}"

if ! command -v cargo-nextest >/dev/null; then
  cargo install cargo-nextest --locked --force
  sudo ln -sf "$HOME/.cargo/bin/cargo-nextest" /usr/local/bin/cargo-nextest
fi

if ! command -v cargo-deny >/dev/null; then
  cargo install cargo-deny --locked --force
  sudo ln -sf "$HOME/.cargo/bin/cargo-deny" /usr/local/bin/cargo-deny
fi

cargo fetch --locked
cargo build --locked -p xtask
cargo build --locked -p pcbcraft -p pcbcraft-cli
