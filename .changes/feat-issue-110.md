## What
Published the actual hole spacing DRC codes (`hole_to_hole_via` and `hole_to_hole_pad`) and added a plated through-hole pad to copper spacing test.

## Why
The DRC emits `hole_to_hole_via` and `hole_to_hole_pad` findings, but only a single obsolete `hole_to_hole` code appeared in CHECK_CODES, so users could not override the severity of the real codes. This also adds the missing test for `hole_to_copper_pth`.

## Files changed
- crates/rules/src/lib.rs
- crates/rules/src/hole_copper.rs
- crates/text/strings/en.txt
