# Worker brief

Getting started: [../getting-started.md](../getting-started.md) and [README.md](README.md).

- **Claim:** `ready`, `status:open`, no live lease; shared host → `conflict:parallel-safe`.
- **Branch:** from fork `main`, push to fork only; merge `origin/main` before final push.
- **Build:** green **GitHub Actions on wdunn001/pcbcraft** for your commit (required). Local `cargo xtask ci` optional.
- **Do not** edit `crates/ui-egui/src/lib.rs` unless your issue owns the `conflict:ui-egui-lib` lane.
- **Report:** Actions link, tests, files, limits; end `PR pending owner review`. No worker PRs to upstream.
- **One clone per agent.** One issue unless stacked epic children.
