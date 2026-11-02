### feature: drive align and array from the Edit menu (e2e)

A new end-to-end story, `a_newcomer_aligns_two_parts_from_the_edit_menu`,
sets up two footprints, box-selects them, opens the real Edit menu,
chooses "Align and distribute" → "Align top", and then opens the
"Array..." dialog and clicks "Make array". The test asserts that both
footprints share the same Y coordinate after aligning and that the array
creates the expected number of new parts. Production code was not
changed; the menu entries already existed in `crates/ui-egui/src/arrange.rs`.
