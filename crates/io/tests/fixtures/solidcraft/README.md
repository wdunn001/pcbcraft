# SolidCraft interchange fixtures

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

`reader_check.rs.txt` is a throwaway test for the SolidCraft workspace. Copy it into
SolidCraft's tree, point `PCBCRAFT_FIXTURE` at `board_mini.solidcraft`, and run it with
SolidCraft's test runner. Expected counts live in `export_counts.json`.
