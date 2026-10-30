# PCBCraft agent instructions

Rust EDA app, MIT OR Apache-2.0, MCP-first. **Codebase of record:** [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft) branch `main` (not upstream tangency-labs until integrator opens a PR). Onboarding: [docs/getting-started.md](docs/getting-started.md), fork issue [#1](https://github.com/wdunn001/pcbcraft/issues/1), full pipeline in CONTRIBUTING.md.

## Before coding

Read README.md (what works), ROADMAP.md, docs/architecture.md. For a claimed issue, read its body plus every `### Research` and `### Enrichment` comment.

## Before editing a file (docs/doc-standard.md)

- **Pre-edit:** read the file's `//!` header and the doc block of each function you change.
- **Pre-edit:** confirm every path under `# Dependencies` and every `Contract:` line exists. If one is missing, stop and report it on the issue. Do not guess a path.
- **Pre-edit:** before editing a trait or the engine command registry, open every file under its `# Implementations`. If the list is missing or stale, stop and report.
- Never shorten a `# Do not` section; only append to it.
- Each rule lives at the layer it governs (this file, `//!` header, trait or shared type, function). Point up to a rule; do not copy it down.
- `cargo xtask docs check` must pass (docs/ci.md).

## Parallel agents

- **One clone or worktree per agent.** Never share a checkout another agent is using.
- **One claimed issue** unless stacked children of one epic.
- **Workers** claim only upstream issues tagged `ready` and `status:open`. On a shared machine, also require `conflict:parallel-safe`.
- **Conflict lane:** only the active `conflict:ui-egui-lib` ticket may edit `crates/ui-egui/src/lib.rs`. Everyone else keeps UI shell changes out of that file; put `a_newcomer_*` tests in `crates/ui-egui/src/e2e/<area>.rs` (helpers in `src/e2e/support.rs`).
- **Branch** from fork `main`: `<kind>-<slug>` (see CONTRIBUTING.md §7). Merge latest fork `main` before final push. Push your branch only; never push to `main`; do not open pull requests (maintainer integrates).

## CI (source of truth)

Green **GitHub Actions on the fork** for the commit you report is required proof. Push early; link the run in your build report. Local `cargo xtask ci` is optional; job list in docs/ci.md. Done = merged into fork `main` with Actions green there.

## Architecture invariants

- One main window: tabs and docked panes only; shared project and selection.
- All mutations: typed engine commands (UI, CLI, MCP); programmatic paths never open dialogs.
- Validate inputs (bounded coords, counts, stable IDs, reference integrity). Invalid command/batch leaves state untouched.
- No `unsafe`; no production `unwrap`/`expect`/`panic`/`todo`/`unimplemented`/`unreachable`.
- Layering: geom → doc/library → rules/io → engine → mcp/ui-egui → apps. Lower crates never depend on egui, eframe, wgpu, or MCP.

## Content and honesty

- Adapted code must be MIT, Apache-2.0 or similarly permissive (never GPL, LGPL or AGPL): record source, revision, license in ATTRIBUTION.md. Pin craft-libs; no proprietary Craft trademarks/assets.
- Do not name other EDA products in code, docs, UI, commits, or issue comments.
- Do not oversell checks or routing; state limits in UI where users hit them.
- UI: every control gets a tooltip of at most two lines: what it does, why or when, its shortcut, and its limits. Disabled controls say how to enable them. Full rule: [docs/tooltips.md](docs/tooltips.md). Real margins; inspect rendered UI for visual changes (`track:ui-design` → screenshots in report).
- No AI attribution in commits. No releases or third-party pushes without authorization.

## Stuck

Same build/test failure twice with identical output → post output, set `status:blocked`, stop (CONTRIBUTING.md §9).

## Cloud Agent fleet

Workers push a branch only and never open a pull request; the integrator merges with `git merge`. Full text: docs/agents/README.md, section "Fleet rules".
