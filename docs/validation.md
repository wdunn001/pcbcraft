# Initial implementation validation

## Getting started

This page records evidence for the first vertical slice, not full workflow
equivalence or manufacturing readiness for arbitrary boards. For day-to-day setup,
see [getting-started.md](getting-started.md). For what shipped since this pass,
compare fork `main` with [capabilities.md](capabilities.md).

## Details

Last full validation pass: **11 October 2026**. Fork `main` has since gained
vias, copper pours, drawn wires, fabrication output jobs, panel export, land-pattern
generate/check, interface rule profiles and fabrication-layer PNG rasters; those
features rely on the same automated gates below plus issue-scoped tests on each merge.

### Automated Rust checks

`cargo xtask ci` passes: formatting, dependency layers, clippy with warnings denied,
the workspace test suite (including headless `a_newcomer_*` stories in
crates/ui-egui), and WebAssembly compilation of the shared UI / browser entry point.

Tests cover failed-batch rollback, stale revisions, undo branch ID allocation,
automatic net creation in one undo step, preserved PCB placements, sample wire
paths avoiding foreign pins, counterclockwise pad coordinates, crossing shorts,
same-layer connectivity, atomic save replacement, invalid native files, MCP
negotiation/notifications/argument schemas, recoverable tool errors, actual tab
clicks and rotate/undo key events, and display refresh after an MCP edit.

### MCP client interoperability

The official Python MCP SDK 2.3.0 initialized the Rust server with protocol
2025-11-25, discovered the full tool registry and completed the same exercise in both
headless mode and via the loopback bridge into the running egui app:

- load the example and inspect stable IDs/revision;
- reject a failing batch without committing its preceding command;
- add eight routed track segments as one transaction;
- return zero findings in the implemented checks and zero airwires;
- return a PNG preview, save native data, export KiCad PCB;
- undo and redo the entire routing batch;
- change a component value through its direct command tool.

The SDK was used only as an external test client; the application/server is Rust.

### Independent KiCad validation

KiCad CLI 10.0.3 loaded the generated routed board and reported **zero violations
and zero unconnected items**. Reproduce:

```sh
pcbcraft-cli run examples/led-indicator.json --save led.pcbcraft
pcbcraft-cli run examples/route-led.json --project led.pcbcraft --save routed.pcbcraft --export routed.kicad_pcb
kicad-cli pcb drc routed.kicad_pcb --output drc.json --format json
```

The unrouted sample correctly reports three disconnected connections in both
PCBCraft and KiCad. Footprints are embedded original generic geometry, not parts
selected from a real component manufacturer's footprint specification.

### Builds and visual inspection

- Linux/WSL desktop and CLI build and run.
- Windows GNU desktop and CLI cross-build; both run natively on Windows.
- Shared UI, engine and browser entry compile for wasm32-unknown-unknown.
- Actual desktop screenshots were captured under Xvfb and visually inspected:
  dark schematic, light schematic and routed PCB with circular cutouts. All use
  a single root window. The packaged native Windows app also rendered and captured
  the actual dark schematic successfully with DirectX 12 and exited cleanly.
  Browser runtime behavior has not been tested yet.

The default routing mode has no obstacle avoidance; the `octilinear` and
`octilinear_shove` modes avoid pads, tracks and cutouts on one layer and push other
nets' tracks within fixed limits (no vias, no spring-back). Initial checks are not
comprehensive ERC/DRC. Full KiCad import, production libraries, inner copper
layers, simulation and real 3D rendering are tracked in ROADMAP.md.

### Parity suite

`crates/engine/tests/parity.rs` runs the reference projects (LED indicator built
from the example command scripts, dual amplifier, KiCad-library RC) and compares
project JSON, command results, normalised check findings, the KiCad export, SVG
views and PNG checksums with goldens in `crates/engine/tests/parity/`. It also
asserts writer determinism and the native save/load round trip.

After an intentional output change, review the diff and re-bless with
`cargo xtask bless-parity` (or `PCBCRAFT_BLESS=1 cargo test -p pcbcraft-engine
--test parity`). Never bless in CI; blessing is ignored when `CI` is set.

Gerber and drill output have no goldens yet: the writer is being fixed in #351.

## Limits

Validation dates do not auto-update on every merge. Treat green fork Actions on
your commit as the live gate for changes you land. This document does not certify
fabrication outputs beyond the KiCad board subset exercise above.

## See also

- [ci.md](ci.md) for the job list and fork branch workflow
- [architecture.md](architecture.md) for layering and honesty about checks
