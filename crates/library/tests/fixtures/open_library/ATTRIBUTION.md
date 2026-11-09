# Recorded library answers

The files in this folder are verbatim copies of answers from the official open
symbol, footprint and 3D model libraries (the project paths are in
`OpenLibraryConfig::default`), recorded by `record.sh` at the pinned revisions
named there. They are test data only; nothing here ships in the app.

| File | What it is |
| --- | --- |
| `Device-R.kicad_sym`, `Device-C.kicad_sym` | symbol files; `Device-R` has its Footprint property set to `Resistor_SMD:R_0603_1608Metric` (the upstream value is empty) so the import tests follow a symbol to its footprint |
| `MCU_ST_STM32F1-STM32F103R8Tx.kicad_sym`, `MCU_ST_STM32F1-STM32F103R_8-B_Tx.kicad_sym` | a microcontroller symbol that inherits its pins from a parent symbol held in a second file, with its footprint, datasheet link and description set; `symbols-MCU_ST_STM32F1.json` is that folder listing trimmed to four entries |
| `R_0603_1608Metric.kicad_mod`, `R_0603_1608Metric.step` | a footprint and its 3D model |
| `LQFP-64_10x10mm_P0.5mm.kicad_mod` | the 64-pin package footprint, `Package_QFP.pretty` |
| `footprints-Package_QFP.json` | the `Package_QFP.pretty` folder listing, trimmed to the 64-pin quad flat packages and their relatives |
| `*-root-*.json`, `*-commit.json`, other `*.json` | folder listings and revision answers, as served |

## Licence

Library data: the open library community, licensed CC-BY-SA 4.0
(https://creativecommons.org/licenses/by-sa/4.0/) with the library
design-output exception. The licence text and the exception are in
`LICENSE-kicad-libraries.md` in the parent folder. Redistributing these files
as a collection keeps that licence and this credit; a board that uses a part
carries no such duty.
