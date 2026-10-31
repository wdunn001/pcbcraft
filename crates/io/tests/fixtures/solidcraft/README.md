# SolidCraft export fixture

`board_two_parts.solidcraft` is the output of the `solidcraft.write` tool for a board with two resistors (R1, R2) and the copper layer on.

`reader_check.rs.txt` is a throwaway integration test for the SolidCraft workspace (`crates/io/tests/pcbcraft_fixture.rs` there). It opens this file with SolidCraft's own `read_design`, evaluates the model and checks the bodies Board, R1 and R2 exist. Last run against SolidCraft main: format upgraded from 22 to the current number, 2 components, 2 occurrences, bodies Board, Copper top, R1, R2, test passed. Run it by hand when the writer changes; it is not part of the default CI because it needs the other workspace.

Files here exercise the PCBCraft SolidCraft reader. Most were saved from SolidCraft
(main branch, format 22, millimetre units) using small native mesh bodies.

| File | Origin |
|------|--------|
| `part_two_bodies.solidcraft` | Part with Red and Blue mesh bodies |
| `assembly_two_occurrences.solidcraft` | Assembly with BrickA and BrickB occurrences |
| `container_mesh.solidcraft` | Same triangle mesh stored in a ZIP container blob |
| `board_mini.solidcraft` | PCBCraft export sample for the SolidCraft reader check |

Regenerate the SolidCraft-native fixtures by opening SolidCraft, creating the matching
geometry, and using Save. Regenerate `container_mesh.solidcraft` by saving the part as a container design in SolidCraft.

## SolidCraft reader check (not default CI)
