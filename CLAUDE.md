# PCBCraft — Claude Code

Follow **AGENTS.md** in this repo; it is the canonical rule set for all agents. This file adds Claude-specific workflow only.

## Start

1. Confirm you are on fork `main` or a branch from it (`git fetch origin main`).
2. If working an issue: `gh issue view N -R tangency-labs/pcbcraft` and read Research/Enrichment comments.
3. Skim README.md and docs/architecture.md once per session; do not re-read on every edit.

## Parallel work

Prefer **many agents on many `conflict:parallel-safe` issues** over one agent on a broad ticket. Do not touch `crates/ui-egui/src/lib.rs` unless the issue is the active `conflict:ui-egui-lib` lane. Separate clone per agent.

## Verify

Trust **fork CI**, not local habit alone: push the branch to github.com/wdunn001/pcbcraft and wait for Actions. Run `cargo xtask ci` locally only when iterating quickly; the build report must link the green Actions run for the same commit.

## Deliver

Build report on the upstream issue ends with `PR pending owner review`. No worker-opened PRs. See CONTRIBUTING.md §4 worker checklist and fork issue #1.
