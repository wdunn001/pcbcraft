# Agent role briefs

## Getting started

1. Read [getting-started.md](../getting-started.md) and [issue #1](https://github.com/wdunn001/pcbcraft/issues/1).
2. Read [AGENTS.md](../../AGENTS.md) and [CONTRIBUTING.md](../../CONTRIBUTING.md) before claiming work.
3. Find issues on wdunn001/pcbcraft (the old tracker on tangency-labs/pcbcraft is frozen); claim with `CLAIM <agent>@<team> until <UTC> branch <name>`. The earliest claim wins; renew with `CLAIM-RENEW <agent>`, stop with `RELEASE <agent>`.
4. Branch from `main`, merge latest `origin/main` before your final push, push the branch, and open a pull request against `main` on wdunn001/pcbcraft. The build report goes in the pull request body, and green **GitHub Actions on the pull request** is the proof.

## Details

Token-sized summaries for parallel headless workers.

| Role | Find work | Output | Lease |
| --- | --- | --- | --- |
| Researcher | `stage:research` | `### Research` comment, `research:done` | 2 h |
| Enricher | `stage:enrich` | `### Enrichment`, full Tags line, `ready` | 2 h |
| Worker | `ready` + `status:open` (+ `conflict:parallel-safe` if shared host) | pull request against `main` with the build report in the body, green Actions | 6 h |
| Reviewer (optional) | open pull request | `### Review` (read/run only, no edits) | 2 h |
| Integrator | pull requests with green CI | merge commit into `main`, green Actions there | none |

**Parallelism:** prefer many workers on `conflict:parallel-safe` issues; one agent per clone; never share checkouts.

**CI:** Actions on the pull request is the source of truth; local `cargo xtask ci` is optional. Doc-only lanes still push and wait for the green **CI result** check.

Role files (optional detail): [researcher.md](researcher.md), [enricher.md](enricher.md), [worker.md](worker.md), [reviewer.md](reviewer.md).

## Limits

Workers open pull requests against `main` on wdunn001/pcbcraft and never push to `main` or merge their own. Never post to the frozen tangency-labs tracker. Only the active
`conflict:ui-egui-lib` ticket may edit `crates/ui-egui/src/lib.rs`. Do not name
other EDA products in comments or docs.

## See also

- [../ci.md](../ci.md) for job names
- [../mcp.md](../mcp.md) if your issue touches automation
