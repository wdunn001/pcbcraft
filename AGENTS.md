# PCBCraft agent instructions

Rust EDA app, GPL-3.0-or-later, MCP-first. Home: [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft). Start here: issue [#1](https://github.com/wdunn001/pcbcraft/issues/1) (authoritative), then [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/getting-started.md](docs/getting-started.md).

The agent rules live in `.cursor/rules/*.mdc`, one topic per file. This file only points to them.

| File | Topic |
| --- | --- |
| `pcbcraft-pipeline.mdc` | claims, branches, pull requests, CI, changesets, integrators |
| `pcbcraft-core.mdc` | code: typed commands, no panics, layering |
| `pcbcraft-ui.mdc` | `crates/ui-egui`: design docs, widgets, tabs |
| `pcbcraft-tests.mdc` | stories, tooltips, golden files, flaky list |
| `pcbcraft-docs.mdc` | documentation layers and edit procedure |
| `pcbcraft-public-repo.mdc` | public repo hygiene, licences, names |

Before editing a file (`docs/doc-standard.md`):

- **Pre-edit:** read the file's `//!` header and the doc block of each function you change.
- **Pre-edit:** confirm every path under `# Dependencies` and every `Contract:` line exists. If one is missing, stop and report it on the issue.
- **Pre-edit:** before editing a trait or the engine command registry, open every file under its `# Implementations`. If the list is missing or stale, stop and report.

`cargo xtask docs check` must pass.
