# Feature: Datasheet review command

Adds the `datasheet.review` engine command to apply accept, edit or reject
decisions to individual rows in a previously extracted datasheet bundle.
Edits copy the original source record into the edited value, and every
text field runs through bounded validation. The bundle is rewritten to
the project attachment after each review operation.

Tests: `crates/engine/tests/datasheet_extract.rs` (review decision recording,
edit without value, index out of range).
