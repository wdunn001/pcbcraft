# Library source harvest (#326)

## Manifest columns

Each stored PDF adds one tab-separated row to `manifest.tsv`:

| column | meaning |
| --- | --- |
| topic | always `library` |
| kind | `datasheet` |
| path | repo-relative path under `research/` |
| url | manufacturer download URL |
| title | document title |
| retrieved | UTC date `YYYY-MM-DD` |
| license | short license note |
| used_for | `#218 (harvest #326)` |
| stored | `true` when the PDF bytes are present |

Append only; do not rewrite prior rows.

## Part TOML

One file per part in `parts/`. Fields map to #218 records:

- `[part]`: identity and datasheet link
- top-level `package`, `pin_count`, `land_pattern_ref`
- `[[pin]]`: PinSpec candidates (`kind` uses the closed PinKind set)
- `[limits]`: absolute and recommended operating limits
- `[[requirement]]`: decoupling and similar rules with `page` and `confirmed`

Unverified extractions set `confirmed = false`.
