# Getting started

Code of record is [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft)
branch `main` until the maintainer opens an upstream pull request.

## Install and run

Need Rust 1.95 or newer. Ubuntu window libraries:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libx11-dev libxi-dev libxrandr-dev libxcursor-dev libgl1-mesa-dev
```

```sh
git clone https://github.com/wdunn001/pcbcraft.git
cd pcbcraft
cargo run -p pcbcraft -- --sample
```

Open a saved project, or a specific tab:

```sh
cargo run -p pcbcraft -- --project led.pcbcraft --tab pcb
```

Headless capture without a desktop (Xvfb):

```sh
xvfb-run -a -s '-screen 0 1600x1100x24' \
  env -u WAYLAND_DISPLAY WGPU_BACKEND=vulkan \
  cargo run -p pcbcraft -- --sample --capture /tmp/pcbcraft.png
```

Windows uses the same `cargo run -p pcbcraft -- --sample` line natively.

## CLI and MCP

```sh
cargo run -p pcbcraft-cli -- commands
cargo run -p pcbcraft-cli -- sample --out led.pcbcraft
cargo run -p pcbcraft-cli -- run examples/led-indicator.json --save led.pcbcraft
cargo run -p pcbcraft-cli -- checks --project led.pcbcraft
cargo run -p pcbcraft-cli -- snapshot --sample --view pcb --out led.png
cargo run -p pcbcraft-cli -- mcp
cargo run -p pcbcraft-cli -- mcp --connect 127.0.0.1:7991
```

`commands` prints the engine command list. MCP is JSON-RPC over newline-delimited
stdio; logs go to stderr. Client setup: [mcp.md](mcp.md).

Bridge the CLI into a running desktop with `--control 127.0.0.1:7991` on the
app and `--connect` on `pcbcraft-cli mcp`. Bind loopback only.

## First circuit

1. Load **LED example**, or press **A**, pick a symbol, click to place.
2. Press **W**, click two pin circles. A net is created when neither pin has
   one; conflicting nets are refused.
3. **Sync to PCB** builds footprints for parts that have an assignment.
4. Press **X**, click two same-net pads. The track is orthogonal pad-to-pad.
5. Open **Checks**. Save `.pcbcraft`.

Limits you will hit on this path: the router does not avoid obstacles; inspect
clearance findings. Footprint moves do not drag copper. Wire crossings do not
join logical nets.

`examples/dual-amplifier.pcbcraft` is a larger native sample (multi-unit op-amp,
edited DIP8, front and back copper).

`examples/showcase.pcbcraft` is the finished showcase board: a battery powered
sensor node with 55 parts on three schematic pages, a four-layer board with
ground and power pours, stitching vias, a USB pair, two builds and no check
findings. Open it from the Start page (**Showcase board**). Regenerate it with
`cargo xtask showcase`; `examples/showcase-with-findings.pcbcraft` is the same
design with one error and two warnings, for reading the Checks page.

## What the checks cover

The Checks tab runs after every edit and lists what it finds, grouped by check.

- Pins with no connection (pins flagged no-connect are skipped), parts with no footprint, and power pins left floating.
- Copper clearance, using the strictest of the board default and the net classes on either side. A rule between two named nets can ask for a bigger gap.
- Track width, stub length and minimum arc radius against the net class limits when set, plus acute corners at track joints and shallow rect-pad joins.
- Pads, drills, part courtyards, board text and silkscreen that touch or overlap, and cut, perforation and mask figures against copper, pads and the board edge.
- Board outline edge and circular cutout clearance.

Limits: some findings can be false alarms, and there are no pin-type, annular-ring, zone or hierarchy checks yet. Each check can be set to warning or turned off in the project rules. Hide this hides one finding; Quiet this area hides a check inside a box around the finding.

## Libraries

Open **Libraries**. Search a symbol, preview it, **Place in schematic**. Select
the placed part, **Assign footprint**, pick a package whose pad numbers match
the pins. **Sync to PCB** keeps existing placements and reports symbols that
still need a package. Used geometry is embedded in the project file.

**Edit a copy**, **New symbol**, or **New footprint** writes into the project
library. `.pcbcraft-lib` collections are reusable. Placed definitions do not
change until you reassign. Formats, pad limits and fixture licenses:
[kicad-libraries.md](kicad-libraries.md).

## Keyboard

| Shortcut | Action |
| --- | --- |
| A | Add-symbol chooser (schematic) |
| W | Wire between pins (schematic) |
| X | Route between pads (PCB) |
| M | Move selection, then click destination |
| R | Rotate selection 90 degrees |
| E | Properties |
| Delete | Delete selection |
| Escape | Cancel tool |
| Ctrl+Z / Ctrl+Y | Undo / redo (Ctrl+Shift+Z also redoes) |
| Ctrl+S | Save |
| Ctrl+K | Command panel |
| Home | Fit the active canvas |

These are the shipping defaults, not a full user keymap.

## Screen readers

The desktop app hands its controls to the screen reader through AccessKit, so Narrator on Windows and VoiceOver on macOS can read the buttons, menus, tabs and settings by name. Linux support is not claimed yet. The canvas buttons Fit, Move and Rotate have names too. Nothing is sent until a screen reader asks, so the app costs nothing extra for everyone else.

## Contribute on the fork

Branch from fork `main`, merge latest `origin/main` before the last push, push
the feature branch only. Green GitHub Actions on that branch is the proof. Do
not run `cargo xtask ci` on a machine that freezes under a full workspace
build; push and read the Actions run. Rules: [CONTRIBUTING.md](../CONTRIBUTING.md),
[AGENTS.md](../AGENTS.md), [ci.md](ci.md). Onboarding:
[issue #1](https://github.com/wdunn001/pcbcraft/issues/1).

## What to read next

| Goal | Page |
| --- | --- |
| Feature inventory | [capabilities.md](capabilities.md) |
| Planned work | [ROADMAP.md](../ROADMAP.md) |
| Crate layers | [architecture.md](architecture.md) |
| How the first subset was checked | [validation.md](validation.md) |
| Adapted code and library licenses | [ATTRIBUTION.md](../ATTRIBUTION.md) |
| Doc map | [README.md](README.md) |
