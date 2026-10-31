# Samtec first-slice drawing spot-check

Generated footprints for TSW, SSW, and BTH are compared to the public manufacturer footprint drawings (not copied into the repository). Measure pad centres against the drawing; pass when every delta is within 0.01 mm.

Drawing pages (public):

- TSW: https://www.samtec.com/products/tsw/
- SSW: https://www.samtec.com/products/ssw/
- BTH: https://www.samtec.com/products/bth/

| Series | Sample part number | Positions | Drawing checked (UTC) | Max pad centre delta (mm) | Pass |
| --- | --- | --- | --- | --- | --- |
| TSW | TSW-104-07-G-S | 4 x 1 | pending owner | | |
| TSW | TSW-110-07-G-S | 10 x 1 | pending owner | | |
| TSW | TSW-110-07-G-D | 10 x 2 | pending owner | | |
| TSW | TSW-120-07-G-S | 20 x 1 | pending owner | | |
| TSW | TSW-108-07-G-S | 8 x 1 | pending owner | | |
| TSW | TSW-106-07-G-S | 6 x 1 | pending owner | | |
| SSW | SSW-104-07-G-S | 4 x 1 | pending owner | | |
| SSW | SSW-110-07-G-S | 10 x 1 | pending owner | | |
| SSW | SSW-110-07-G-D | 10 x 2 | pending owner | | |
| SSW | SSW-108-07-G-S | 8 x 1 | pending owner | | |
| SSW | SSW-106-07-G-S | 6 x 1 | pending owner | | |
| SSW | SSW-120-07-G-S | 20 x 1 | pending owner | | |
| BTH | BTH-060-01-L-D-A | 30 x 2 | pending owner | | |
| BTH | BTH-030-01-L-D-A | 15 x 2 | pending owner | | |
| BTH | BTH-090-01-L-D-A | 45 x 2 | pending owner | | |
| BTH | BTH-120-01-L-D-A | 60 x 2 | pending owner | | |
| BTH | BTH-030-01-L-D-A-K | 15 x 2 + posts | pending owner | | |
| BTH | BTH-060-01-L-D-A-K | 30 x 2 + posts | pending owner | | |

Automated checks in `crates/library/tests/connector_series_generate.rs` cover pitch, pad count, pin 1 corner, courtyard enclosure, housing length, and BTH stack height from the embedded series tables.
