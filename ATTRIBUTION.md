# Attribution

| Component | Source | Revision / license | Use |
|---|---|---|---|
| craft-ui | https://github.com/storytold/craft-libs | 1aa81a773aa9198a0e4c0daeda954d622475af38; MIT OR Apache-2.0 | Shared egui tab and widget components, Cargo dependency |
| SolidCraft architecture | https://github.com/bherbruck/solidcraft | README / AGENTS consulted; MIT OR Apache-2.0 | Architectural reference; no implementation copied |
| SolidCraft agent chat backend | https://github.com/bherbruck/solidcraft | 32558d0a; MIT OR Apache-2.0 | Adapted into `crates/assistant/src/backend.rs` |
| SolidCraft agent chat transcript | https://github.com/bherbruck/solidcraft | 32558d0a; MIT OR Apache-2.0 | Adapted into `crates/assistant/src/transcript.rs` and `turn.rs` |
| SolidCraft agent chat test script backend | https://github.com/bherbruck/solidcraft | 32558d0a; MIT OR Apache-2.0 | Adapted into `crates/assistant/src/script.rs` |
| SolidCraft agent chat API backend | https://github.com/bherbruck/solidcraft | 32558d0a; MIT OR Apache-2.0 | Adapted into `crates/assistant/src/api.rs` |
| SolidCraft agent chat gate | https://github.com/bherbruck/solidcraft | 32558d0a; MIT OR Apache-2.0 | Adapted into `crates/assistant/src/gate.rs` |
| SolidCraft agent chat connections and presets | https://github.com/bherbruck/solidcraft | 32558d0a; MIT OR Apache-2.0 | Adapted into `crates/assistant/src/connections.rs` |
| SolidCraft agent chat settings page | https://github.com/bherbruck/solidcraft | 32558d0a; MIT OR Apache-2.0 | Adapted in shape into `crates/ui-egui/src/assistant/settings.rs` |
| SolidCraft Linux packaging | https://github.com/bherbruck/solidcraft | packaging/linux and packaging/env.sh at 2026-03; MIT OR Apache-2.0 | Adapted AppImage, deb, and rpm scripts, nfpm config, AppRun, and postinst for PCBCraft |
| IBM Plex Sans (SemiBold) | https://github.com/IBM/plex | Copyright IBM Corp., Reserved Font Name "Plex"; SIL Open Font License 1.1, text in `assets/brand/fonts/OFL.txt` | Outlined to paths in the wordmark masters under `assets/brand/`; the font file is kept there only so `assets/brand/build.sh` can regenerate them |
| KiCad board format | https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/ | Public format documentation | Original limited board writer |
| KiCad library formats | https://dev-docs.kicad.org/en/file-formats/ | Public format documentation | Original Rust symbol/footprint readers |
| KiCad library fixtures | https://gitlab.com/kicad/libraries/kicad-symbols and https://gitlab.com/kicad/libraries/kicad-footprints | Ubuntu packages 10.0.6~ubuntu24.04.1; CC-BY-SA 4.0 + KiCad design-output exception | Extracted R/LED/LM2904/LM358 symbols and unmodified R_0805 footprint; community contributors; exact provenance/checksums and modifications in docs/kicad-libraries.md; original license in crates/library/tests/fixtures/LICENSE-kicad-libraries.md |
| IBM Plex Sans | https://github.com/IBM/plex | Variable font 2.x, cut to static weights 400, 500 and 600; SIL OFL 1.1 (crates/ui-egui/assets/fonts/OFL-IBMPlex.txt) | Interface font, embedded in `crates/ui-egui/src/fonts.rs` |
| JetBrains Mono | https://github.com/JetBrains/JetBrainsMono | Regular; SIL OFL 1.1 (crates/ui-egui/assets/fonts/OFL-JetBrainsMono.txt) | Numbers, references and net names font, embedded in `crates/ui-egui/src/fonts.rs` |
| Open library recorded answers | https://gitlab.com/kicad/libraries/kicad-symbols, https://gitlab.com/kicad/libraries/kicad-footprints and https://gitlab.com/kicad/libraries/kicad-packages3D | Repository revisions 45efbd93, a337146e and 89757151; CC-BY-SA 4.0 + KiCad design-output exception | Unmodified R and C symbols, R_0603 footprint and STEP model, folder listings reduced to a few entries, revision answers reduced to id and date; used only as test fixtures in crates/library/tests/fixtures/open_library; recording script and licence beside them (LICENSE-kicad-libraries.md) |
| Showcase sensor node | `cargo xtask showcase` (`xtask/src/showcase/`) | Original design by the PCBCraft authors; CC BY-SA 4.0 | `examples/showcase.pcbcraft` and `examples/showcase-with-findings.pcbcraft`; every symbol, footprint and placement is defined in code, no third-party library data |

KiCad's official PCB_POINT API/source documentation was consulted to distinguish
non-manufactured reference-point metadata from copper/outline geometry; exact links
and fixture checksums are recorded in docs/kicad-libraries.md. No implementation
code was copied or ported. The DIP8 fixture is unmodified; the original dual-amplifier
example/library uses edited LM358 pin metadata and a DIP8 offset oval drill, with
the KiCad data license and source attribution embedded in the native collection.

No KiCad implementation code has been copied or ported in this implementation.
PCBCraft sample circuits are original; imported symbol/footprint artwork retains
its KiCad data license and design-output exception. Dependencies retain their own licenses.

The octagonal-hull walkaround and shove router in crates/engine/src/route and
crates/geom/src/hull.rs is an original implementation from published algorithm
descriptions. No third-party router source was copied or ported.

## UI icons (assets/icons/ui)

Generic icons are copied from Lucide (lucide-static 0.468.0), ISC license with MIT for parts derived from Feather; the license text is in assets/icons/ui/LICENSE-lucide.txt. Each file names its source icon in a leading comment. Domain icons (via, pads, zone, keepout, net, track, diff pair, footprint, courtyard, drill, stackup, layers, board outline, cutout, mounting hole, wire, bus, label, port, power, no-connect, junction, part, sheet, drill file) are drawn for PCBCraft and carry the project license.

## Project licence

The project licence changed from GPL-3.0-or-later to MIT OR Apache-2.0 (see NOTICE). Material listed above under its own licence keeps it. Code adapted from other projects must be under a compatible permissive licence.
