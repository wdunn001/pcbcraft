# Localization checks

`cargo xtask i18n` is the CI job **Localization**. The catalog is `crates/text/strings/`:
`en.txt` is the base and `de.txt`, `es.txt` and `fr.txt` override it. Undefined keys and literals
inside converted functions are caught by `cargo xtask strings check` (job "UI strings are in the
catalog"); this check adds the rest. Job wiring: [ci.md](ci.md).

## What it enforces

| Rule | Counted per | Fix a failure by |
| --- | --- | --- |
| Key parity: every key in `en.txt` is in each locale | locale (`missing`) | Translate the key |
| Placeholder parity: each `{name}` and how often it appears matches `en.txt` | key, always fatal | Copy the placeholders from English |
| Unused keys: nothing in the code refers to the key | catalog (`unused`) | Delete the key or use it |
| Hard-coded text: a string literal passed to a label, button, heading, tooltip or `RichText` | file (`literals`) | Add a key and call `t!("key")` |
| Joined fragments: two lookups in one `format!` or added with `+` | file (`concatenation`) | Make one key with `{placeholders}` |
| Pseudo-locale `en-XA` (accented, 40% longer) at 1280 and 320 points: text clipped with no ellipsis, controls leaving the window | screen and width, `crates/ui-egui/src/e2e/pseudo-ratchet.txt` | Wrap or elide the text; let the row wrap |
| Every character in every locale file has a glyph in the loaded fonts | always fatal | Add a fallback font in `crates/ui-egui/src/fonts.rs` |

The first four kinds are counts in `crates/text/strings/i18n-ratchet.txt`. They can only shrink: a
count above the file fails, and a count below it fails until the file is lowered. `cargo xtask i18n
--write` prints the current lines. The pseudo-locale builder is `crates/text/src/pseudo.rs`.

## Reported only

Plural rules beyond one and other, and number, unit and date formatting per locale have no check.
Right-to-left layout is also unchecked. Each has an open issue.

Run it locally with `cargo xtask i18n`.
