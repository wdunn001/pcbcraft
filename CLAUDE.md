# PCBCraft — Claude Code

Follow **AGENTS.md** in this repo; it is the canonical rule set for all agents. This file adds Claude-specific workflow only. Issue [#1](https://github.com/wdunn001/pcbcraft/issues/1) ("Start here") is the authoritative short version of the process.

## Start

1. Confirm you are on `main` of wdunn001/pcbcraft or on a branch from it (`git fetch origin main`).
2. If working an issue: `gh issue view N -R wdunn001/pcbcraft` and read every Research and Enrichment comment. Issues, pull requests and CI all live on wdunn001/pcbcraft; the old tracker on tangency-labs/pcbcraft is frozen, so never post there.
3. Before claiming, read the comments from newest to oldest. A claim is live if its `until` time is in the future and no `RELEASE` follows it. Otherwise claim with one `CLAIM <agent>@<team> until <UTC time> branch <name>` comment; the earliest claim wins. Renew with `CLAIM-RENEW <agent>`, stop with `RELEASE <agent>`.
4. Skim README.md and docs/architecture.md once per session; do not re-read on every edit.

## Parallel work

Prefer **many agents on many `conflict:parallel-safe` issues** over one agent on a broad ticket. Do not touch `crates/ui-egui/src/lib.rs` unless the issue is the active `conflict:ui-egui-lib` lane. Separate clone per agent.

## Verify

Trust **CI on the pull request**, not local habit alone. Run `cargo xtask ci` locally only when iterating quickly. The pull request must link the green Actions run for the same commit.

## Deliver

Branch from `main`, merge the latest `main` into it before your final push, then open a pull request against `main` on wdunn001/pcbcraft. The pull request body is the build report (what was built, files touched, tests, limits) and ends with an `Issues: #N` line. Never push to `main`. Do not merge your own pull request: an integrator merges it when CI is green. See CONTRIBUTING.md sections 2 and 7.
