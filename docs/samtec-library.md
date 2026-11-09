# Samtec connectors in the library

PCBCraft does not ship Samtec CAD files. Their public terms forbid copying and
redistributing drawings, models, and bulk downloads without written consent.
The library instead offers a configurable catalog: the user picks a series, sets
options, and receives a generated footprint, symbol, and parametric body that
must be checked against the manufacturer drawing.

## First slice

The embedded catalog (`crates/library/data/connectors/catalog.toml`) registers
three series for milestone 2:

| Prefix | Role |
|--------|------|
| TSW | 0.100 inch through-hole pin header strip |
| SSW | Matching socket strip |
| BTH | 0.5 mm surface-mount board-to-board header |

Each series appears in `library.list` as a **generated** listing under
`Samtec connectors`. `library.catalog` also returns a
`configurable_connectors` object with the same records.

## Related work

- Part-number build and parse: issue #779
- Footprint, symbol, and 3D generators: issue #780
- Library tab configurator and search: issue #782
- User-download import and manufacturer page: issue #785

Generated parts carry the verify note from the catalog and link to the public
series page on samtec.com. No vendor file is committed, bundled, or fetched in
bulk.

## Owner decision

Written permission from Samtec for automated fetch or redistribution has not
been requested yet. The catalog records `permission_status = not_requested`
until the owner updates it.
