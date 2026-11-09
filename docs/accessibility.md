# Accessibility checks

`cargo xtask a11y` is the CI job **Accessibility**. It runs headless (no GPU) and walks every
tab, the start page, the About dialog and the File, Edit, View and Help menus through the
AccessKit tree. The job summary shows one row per screen. The rules are tests in
`crates/ui-egui`; this page lists them and what to do when one fails. Job wiring: [ci.md](ci.md).

## What it enforces

| Rule | Where it lives | Fix a failure by |
| --- | --- | --- |
| Every interactive node has an accessible name and role (WCAG 4.1.2) | `crates/ui-egui/src/e2e/shell.rs` `every_interactive_control_has_an_accessible_name`; counts in `crates/ui-egui/src/e2e/accessible-name-exemptions.txt` | Label the control, or call `a11y::name_control` for a glyph-only one |
| No two controls with the same role, name and parent | `crates/ui-egui/src/e2e/a11y_gate.rs`; counts in `crates/ui-egui/src/e2e/a11y-ratchet.txt` | Put the row or object into the name ("Hide Copper" for one list, not two) |
| Targets are at least 24 by 24 points (WCAG 2.5.8) | `crates/ui-egui/src/target_size.rs`; list in `crates/ui-egui/target-size-exemptions.txt` | Size the control with `App::MIN_TARGET` |
| Every Tab press moves focus, and every enabled control is reached | `crates/ui-egui/src/e2e/a11y_gate.rs` (`unreached-by-tab`, `focus-trap`) | Make the control focusable; do not capture Tab |
| Contrast of every theme token pair, dark and light (4.5:1 text, 3:1 large text and parts) | `crates/ui-egui/src/contrast.rs`; list in `crates/ui-egui/contrast-exemptions.txt` | Change the colour in `crates/ui-egui/src/palette.rs` |
| Reduce motion sets the animation time to zero | `crates/ui-egui/src/e2e/a11y_gate.rs` | Read `Preferences::reduced_motion` ([`motion.rs`](../crates/ui-egui/src/motion.rs)) |

## Ratchets

Each list can only shrink. A count above its file fails; a count below it fails too, so the number
is lowered in the same change. The failure message prints the current lines to paste. Never raise a
count or add an exemption without an issue number and a reason on the line.

## Not yet checked

Colour-only status (a state dot needs a shape and a word) and a full focus-order comparison against
reading order are tracked in #922 and have no test yet.

Run it locally with `cargo xtask a11y`.
