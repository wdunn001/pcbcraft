# STEP reader comparison spike

Part of issue 724. Goal: compare the current pure-Rust truck reader with two newer pure-Rust STEP readers on 30 real component models, recording load time, triangle count, colour support and assembly handling.

## Current reader

- Crate path: `crates/io/src/step_read.rs`
- Tessellation tolerance: `STEP_TOLERANCE_MM` (0.05 mm)
- Output: triangles in millimetres, single diffuse colour when present

## Candidates to benchmark

1. **truck** (in tree today): `truck-stepio` + `truck-meshalgo`
2. **Candidate B**: a second pure-Rust STEP crate (to be pinned in `Cargo.toml` when the spike branch runs)
3. **Candidate C**: a third pure-Rust STEP crate (same)

Record crate names and versions in the benchmark output when they are chosen; do not add them to `main` until the spike report lands.

## Corpus

Thirty STEP files from the open component model library already used in library tests (mixed QFP, SOT, passives, connectors). Store only paths in git; fetch on the CI host.

## Metrics per file

| Field | Meaning |
| --- | --- |
| `load_ms` | Wall time to produce triangles |
| `triangles` | Index count divided by three |
| `colours` | Whether more than one material colour appears |
| `assemblies` | Whether multiple named solids are preserved vs merged |
| `error` | Plain sentence when the reader refuses the file |

## How to run (CI host)

```text
cargo xtask step-reader-spike --corpus fixtures/step-spike-list.txt --out target/step-spike.json
```

The xtask target is not wired yet; this document fixes the procedure before automation lands.

## Decision

After the table is filled, pick one reader (or keep truck) based on colour and assembly needs versus median load time on the corpus. Bump `LOADER_VERSION` and invalidate the import cache when the chosen reader changes output.
