//! Acceptance story: a_newcomer_rounds_every_corner_of_a_board_in_one_step
//!
//! A newcomer draws a track with sharp right-angle corners, opens the Edit
//! menu and looks for a "Round corners" command. They pick a radius and
//! watch every corner of the selected tracks turn into circular fillets
//! in one step, the way a factory would produce the copper.
//!
//! BLOCKED: no UI control exists for track.round_corners. The Edit menu
//! has "Clean up tracks" (track.cleanup) but no "Round corners" entry.
//! No button, menu item, or context menu entry for track.round_corners
//! was found in the UI. The engine command works (unit tests pass), but
//! there is no UI affordance to invoke it.
//!
//! Missing control: Edit menu "Round corners" item (or equivalent button).
//!
//! To unblock: add an Edit menu item "Round corners" that opens a dialog
//! for radius input, or adds a track inspector button like "Straighten",
//! "Chamfer", "Split" already have.

#[test]
#[ignore = "BLOCKED: Edit menu item \"Round corners\" is missing"]
fn a_newcomer_rounds_every_corner_of_a_board_in_one_step() {
    // This test is BLOCKED on the missing UI control.
    // See doc comment above.
}
