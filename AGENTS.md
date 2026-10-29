# PCBCraft agent instructions

Rust EDA app, GPL-3.0-or-later, MCP-first. **Codebase of record:** [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft) branch `main` (not upstream tangency-labs until integrator opens a PR). Onboarding: [docs/getting-started.md](docs/getting-started.md), fork issue [#1](https://github.com/wdunn001/pcbcraft/issues/1), full pipeline in CONTRIBUTING.md.

## Before coding

Read docs/getting-started.md, docs/capabilities.md, ROADMAP.md, docs/architecture.md. For a claimed issue, read its body plus every `### Research` and `### Enrichment` comment.

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

- GPL adaptations: record source, revision, license in ATTRIBUTION.md. Pin craft-libs; no proprietary Craft trademarks/assets.
- Do not name other EDA products in code, docs, UI, commits, or issue comments.
- Do not oversell checks or routing; state limits in UI where users hit them.
- UI: tooltips on controls; real margins; inspect rendered UI for visual changes (`track:ui-design` → screenshots in report).
- No AI attribution in commits. No releases or third-party pushes without authorization.

## Stuck

Same build/test failure twice with identical output → post output, set `status:blocked`, stop (CONTRIBUTING.md §9).

## Cloud Agent fleet (fork wdunn001/pcbcraft)

**Workers:** push a `<kind>-<slug>` or `feature/<slug>-db36` branch only. Green **GitHub Actions on that branch** is the proof. Post the upstream build report ending with `PR pending owner review`. **Do not** open a pull request on the fork and **do not** use forge PR tools to create one—maintainer/integrator lands work on fork `main` with a merge commit after review.

**Integrator (same fleet):** merge contributor branches into fork `main` with `git merge` (not `gh pr merge`). Close any stray worker-opened fork PRs with `closed` status so the branch + Actions run remain the record. Upstream PR to tangency-labs waits until fork `main` is green and the owner has run the packaged test gate—never ask the owner to close PRs or merge manually.

