#!/usr/bin/env bash
# Regenerate the brand masters and every platform icon (#687).
# Runs tools/build.py in a Python container with librsvg, fontTools and Pillow,
# so it behaves the same on a workstation, on the build host and in CI.
# Usage: assets/brand/build.sh   (needs docker; PYTHON_IMAGE overrides the image)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="${PYTHON_IMAGE:-python:3.11-slim}"
docker run --rm -v "$ROOT":/w -w /w "$IMAGE" bash -c '
  set -e
  apt-get update -qq >/dev/null
  apt-get install -y -qq librsvg2-bin >/dev/null
  pip install -q fonttools pillow 2>&1 | grep -v notice || true
  python3 assets/brand/tools/build.py
  chown -R '"$(id -u):$(id -g)"' assets/brand
'
