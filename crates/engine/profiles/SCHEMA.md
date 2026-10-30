# Interface rule profile schema

Profiles are TOML files with `schema = 1` (legacy) or `schema = 2` (protocol bus template). One profile per file.

## Header

| Field | Type | Notes |
| --- | --- | --- |
| `schema` | integer | `1` or `2`. |
| `[profile].id` | string | Slug, 1 to 64 bytes, lowercase letters, digits, hyphen. |
| `[profile].name` | string | Display name, 1 to 128 bytes. |
| `[profile].category` | string | 1 to 64 bytes. |
| `[profile].summary` | string | 1 to 512 bytes. |

## Library record (schema 2, optional)

| Field | Type | Notes |
| --- | --- | --- |
| `[record].kind` | string | Must be `interface_profile`. |
| `[record].license` | string | Up to 128 bytes. |
| `[record].source` | string | Provenance note, 1 to 512 bytes. |
| `[record].content_hash` | string | Optional 64 hex digits; usually computed on import. |

Precedence when applying by id: project library entry, then `{config}/pcbcraft/interface-profiles/{id}.toml`, then built-in.

## Protocol bus (schema 2, required)

| Field | Type | Notes |
| --- | --- | --- |
| `[bus].name` | string | Bus template name, 1 to 128 bytes. |
| `[bus].schematic_type` | string | Schematic bus or port type, 1 to 128 bytes. |
| `[bus.limits].edge_rate_band` | string | Optional label, up to 64 bytes. |
| `[bus.limits].impedance_ohm` | number | 20 to 200. |
| `[bus.limits].impedance_tolerance_percent` | number | 0 to 100. |
| `[bus.limits].max_routed_length_mm` | number | 0 to 10000. |
| `[bus.limits].max_stub_mm` | number | 0 to 10000. |
| `[bus.limits].skew_budget_ps` | number | 0 to 1e6. |
| `[bus.limits].reference_plane` | string | `required`, `recommended`, or `optional`. |

## Signals (schema 2)

Array `[[signal]]` (at most 64):

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | 1 to 128 bytes. |
| `net` | string | Fixed net name, or use `pattern`. |
| `pattern` | string | Glob pattern for net names. |
| `electrical_class` | string | `single-ended`, `open-drain`, or `differential`. |

## Net classes, pairs, and areas (schema 1 and 2)

Same as phase 1: `[[net_class]]`, `[[pair]]`, `[[pair_rule]]`, `[[rule_area]]`, `[[value_source]]`.

Known `value_source.field` keys include net class and pair fields, plus `bus.limits.*` keys listed in `crates/doc/src/interface_profile.rs`.

## Companions and checks (schema 2)

`[[companion]]`: `role`, optional `value_min`, `value_max`, `unit`, `placement_distance_max_mm`, required `source` note, optional `nets` list.

`[[check]]`: `name`, optional `[check.params]` table (at most 32 keys).

Until companion checks land, apply reports companions in the plan and lists `companion` under `unsupported`.

## Unsupported reporting

Fields the engine cannot enforce yet are returned in order under `unsupported` (for example `companion`, `check`, `bus`, `signal.*`, `net_class.*.limits.*`).

Unknown keys anywhere in the file are refused at parse time.
