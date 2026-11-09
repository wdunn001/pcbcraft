# MCU pin-mux file (schema = 1)

File suffix: `.pcbcraft-mcu.toml` (TOML) or import JSON with the same fields under `McuImportBundle`.

## Required provenance

| Field | Meaning |
| --- | --- |
| `source_repository` | Upstream repository or document URL |
| `revision` | Upstream revision or document edition |
| `licence` | SPDX or plain licence name |
| `fetched` | ISO date `YYYY-MM-DD` when the table was captured |

## Record

- `mcu`: chip id, marketing name, vendor, family, optional `max_alternate_function` (default 15).
- `package`: one or more package variants; each lists every pin or ball name on that package.
- `pin`: mux row per signal pin; `alternate` lists `{ af, signal }` pairs where `signal` is `INSTANCE.SIGNAL`.
- `peripheral`: declared instances (`kind`, `instance`, `signals`, optional timer `channels`).
- `dma` (optional): `{ instance, signal, request }` rows referencing declared peripheral signals.

GPIO defaults use alternate function 0 and signals like `GPIOA.0`.

## Validation

The library crate refuses files when provenance is empty, a package pin has no mux row, a mux row names a pin absent from every package, alternate functions exceed the chip maximum, peripheral instances are unknown, or signals are not declared on their instance.

Import JSON bundles use the same fields without the top-level `schema` key; the importer sets `schema = 1`.
