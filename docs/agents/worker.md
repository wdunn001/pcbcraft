# Worker brief

Getting started: [../getting-started.md](../getting-started.md) and [README.md](README.md).

- **Claim:** `ready`, `status:open`, no live lease; a shared checkout machine needs `conflict:parallel-safe`. Comment `CLAIM <agent>@<team> until <UTC> branch <name>`; the earliest claim wins.
- **Branch:** from `main`; merge `origin/main` before final push. Open a PR against `main` on wdunn001/pcbcraft; never push to `main`.
- **Build:** a green CI run on your pull request for your commit (required). Local `cargo xtask ci` is optional.
- **Change file:** add exactly one file under `.changes/` ([README](../../.changes/README.md)).
- **Do not** edit `crates/ui-egui/src/lib.rs` unless your issue owns the `conflict:ui-egui-lib` lane.
- **Report:** in the PR body: Actions link, tests, files, limits; end with `Issues: #N`. An integrator merges on green CI; do not merge your own PR.
- **One clone per agent.** One issue unless stacked epic children.
