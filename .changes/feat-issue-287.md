## Board interchange import

Import KiCad PCB interchange files (`.kicad_pcb`) with full round-trip fidelity.
The `project.import` engine command with format `board` replaces the current
project and supports undo.

- Import footprints, pads, tracks, arcs, vias, zones, and design rules
- Preserve unknown source nodes for lossless round-trips
- Support multi-layer boards with blind and buried vias
- Malformed files produce errors that name the line number
