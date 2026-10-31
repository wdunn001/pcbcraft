# SolidCraft export fixture

`board_two_parts.solidcraft` is the output of the `solidcraft.write` tool for a board with two resistors (R1, R2) and the copper layer on.

`reader_check.rs.txt` is a throwaway integration test for the SolidCraft workspace (`crates/io/tests/pcbcraft_fixture.rs` there). It opens this file with SolidCraft's own `read_design`, evaluates the model and checks the bodies Board, R1 and R2 exist. Last run against SolidCraft main: format upgraded from 22 to the current number, 2 components, 2 occurrences, bodies Board, Copper top, R1, R2, test passed. Run it by hand when the writer changes; it is not part of the default CI because it needs the other workspace.
