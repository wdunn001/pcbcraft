# Library part records

Parts are stored in three layers:

1. **Component** (`component`): generic symbol, footprint(s), 3D model, pin-to-pad map, parameters, stable id and content hash.
2. **Orderable part** (`orderable`): one manufacturer part number bound to exactly one component, with supplier SKUs, lifecycle, compliance flags, dated stock and price snapshots, and a datasheet link.
3. **Provenance** (`provenance`): source, licence, fetch time, revision or hash.

Project files copy these records under `library.part_components` and `library.orderable_parts`. Placed parts link through `part_links`.

See `records.schema.json` for the JSON shape. Runtime types live in `crates/library/src/records.rs`.
