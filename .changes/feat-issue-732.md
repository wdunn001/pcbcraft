feat: export board to SolidCraft project

Add SolidCraft as an export format. The board exports as a SolidCraft project
with its real outline, cutouts, holes and thickness, and each placed part as
its own named component with a 3D model. Optional layers add copper, mask
and silk as separate bodies.

- Format registry entry in crates/io/src/formats.rs
- Export dialog entry under Exchange group
- Newcomer story test: a_newcomer_exports_the_board_to_solidcraft
