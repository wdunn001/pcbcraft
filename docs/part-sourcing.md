# Part sourcing fields

A component carries a typed sourcing record next to its free-form properties. Bill of
materials files, the part search and assembler packages read the record. Issues #227 and
#804.

## The record (`Component.sourcing`, `crates/doc/src/sourcing.rs`)

The record is saved only when something is set. A file without it loads as an empty record.

| Key | Type | Bounds | Meaning |
| --- | --- | --- | --- |
| `manufacturer` | text | 256 bytes, no control characters | Maker of the part |
| `mpn` | text | 256 bytes | The maker's part number |
| `description` | text | 256 bytes | What the part is, in words |
| `suppliers` | list of supplier parts | at most 16, names unique ignoring case | One number per supplier |
| `lifecycle` | `active`, `nrnd`, `last_time_buy`, `obsolete`, `unknown` | | Where the part is in its life |
| `moisture_level` | integer | 1 to 6 | Moisture sensitivity level |
| `rohs`, `reach`, `halogen_free` | yes or no | | Compliance flags |

A supplier part is `{ "supplier": text, "part_number": text, "url": text (optional, 512 bytes) }`.
Mount type is not stored; it comes from the pads.

## Commands

- `component.set_sourcing`: `{ id, manufacturer?, mpn?, description?, lifecycle?, moisture_level?, rohs?, reach?, halogen_free? }`.
  A key left out stays; `null` (or an empty string for text) clears it. One undo step.
- `component.set_supplier`: `{ id, supplier, part_number, url? }` adds or replaces the entry of that supplier
  (name compared ignoring case); a `null` part number removes it.
- `part.attach_source` (the online part search) also writes the record: manufacturer, MPN, description,
  lifecycle text and the distributor's number with its product link.

Both commands refuse a bad value and leave the record unchanged.

## Old files

On load (`pcbcraft_io::load` and `Session::replace`) free-form properties whose names match are copied into
the record: `Manufacturer`, `Mfr`, `MPN`, `Manufacturer Part Number`, `Description`, and a supplier number from
keys such as `Digi-Key PN` or `Mouser SKU`. Names match ignoring case, spaces and dashes. A value already set in
the record wins. The free-form properties stay in place and stay readable; the second load changes nothing.

## Bill of materials columns

`bom.export` reads these keys through `Component::bom_value`: `Manufacturer`, `MPN`, `Description`, `Lifecycle`,
`Moisture level`, `RoHS`, `REACH`, `Halogen free`, `Suppliers` (every supplier as `Name: number`, joined by `; `)
and `Supplier:<name>` (one supplier's number). Any other key reads the free-form property. The Export dialog's
generic CSV flavour still lists the usual columns; Description and supplier numbers are available as standard keys
in `bom.export`, project BOM columns and the UI lane that wires flavour tokens (issue #591).

## Parts table and BOM field keys

`Manufacturer`, `MPN` and `Description` are reserved standard keys (`pcbcraft_library::part_fields`).
Schematic parts tables and `bom.export` read them through `Component::bom_value`. Supplier numbers
use `Suppliers` or `Supplier:<name>`.
