#!/usr/bin/env bash
# Print SolidCraft component and body counts for a PCBCraft export fixture.
# Run on a machine that has the SolidCraft repo checked out; not part of default CI.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="${1:-$ROOT/crates/io/tests/fixtures/solidcraft/board_mini.solidcraft}"
COUNTS="$ROOT/crates/io/tests/fixtures/solidcraft/export_counts.json"
export PCBCRAFT_FIXTURE="$FIXTURE"
if [[ -z "${SOLIDCRAFT_ROOT:-}" ]]; then
  echo "Set SOLIDCRAFT_ROOT to a SolidCraft checkout with solidcraft_io available." >&2
  exit 1
fi
cd "$SOLIDCRAFT_ROOT"
cargo test -p solidcraft-io --test pcbcraft_fixture opens_pcbcraft_export -- --nocapture
python3 - "$COUNTS" <<'PY'
import json, sys
expected = json.load(open(sys.argv[1]))
print("expected", expected)
PY
