# Firmware target export plugin

Issue #980. Flight-controller board targets live outside the core in
`pcbcraft-plugin-firmware-target`, registered when the engine is built with
the `plugin-firmware-target` feature.

## Project files

- `firmware-design.toml` (or the name in project meta `firmware_design_file`)
  lists the MCU reference, pin roles, gyro, flash, serial ports and unmapped parts.
- An MCU pin-mux record TOML named in the design (see issue #844 for the shared library format).

## Commands (plugin feature enabled)

- `firmware.target.export` writes `configs/<manufacturer>/<board>/config.h` and
  `board-manifest.json`.
- `firmware.config.compare` compares an existing config header with the design.

## Install manifest

`plugins/firmware-target-betaflight/plugin.toml` describes the plugin id,
export kind and commands for future discovery (issue #978).
