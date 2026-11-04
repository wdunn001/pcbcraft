# Issue #980: Firmware target export plugin

Move flight-controller firmware target export out of the engine core and behind the plugin interface.

## Changes

- Removed the core `firmware.config.compare` command handler and its registration.
- Removed the core `firmware_target` export kind handler.
- Removed the core `firmware_config_compare` function.
- The test file `crates/engine/tests/firmware_target.rs` is now gated behind `plugin-firmware-target` and keeps only the peripheral conflict test; the firmware export test lives in the plugin crate.
- Core builds and tests pass with and without the feature.
