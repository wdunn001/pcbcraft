# Agent role briefs

## Getting started

1. Read [getting-started.md](../getting-started.md) and fork [issue #1](https://github.com/wdunn001/pcbcraft/issues/1).
2. Read [AGENTS.md](../../AGENTS.md) and [CONTRIBUTING.md](../../CONTRIBUTING.md) before claiming work.
3. Find upstream issues on tangency-labs/pcbcraft; claim with `CLAIM <agent>@<host> until <UTC> clone <path> branch <name>`.
4. Branch from fork `main`, merge latest `origin/main` before your final push, push the branch only, and use green **GitHub Actions on wdunn001/pcbcraft** as proof (link the run in your build report).

## Details

Token-sized summaries for parallel headless workers.

| Role | Find work | Output | Lease |
| --- | --- | --- | --- |
| Researcher | `stage:research` | `### Research` comment, `research:done` | 2 h |
| Enricher | `stage:enrich` | `### Enrichment`, full Tags line, `ready` | 2 h |
| Worker | `ready` + `status:open` (+ `conflict:parallel-safe` if shared host) | branch + green fork Actions + build report | 6 h |
| Reviewer | branch with report | `### Review` (read/run only, no edits) | 2 h |
| Integrator | approved branches | merge to fork `main`, green Actions there | maintainer |

**Parallelism:** prefer many workers on `conflict:parallel-safe` issues; one agent per clone; never share checkouts.

**CI:** fork Actions on the pushed branch is the source of truth; local `cargo xtask ci` is optional. Doc-only lanes still push and wait for the green **CI result** check.

Role files (optional detail): [researcher.md](researcher.md), [enricher.md](enricher.md), [worker.md](worker.md), [reviewer.md](reviewer.md).

## Limits

Workers do not open pull requests on the fork or upstream. Only the active
`conflict:ui-egui-lib` ticket may edit `crates/ui-egui/src/lib.rs`. Do not name
other EDA products in comments or docs.

## See also

- [../ci.md](../ci.md) for job names and the screenshot sheet
- [../mcp.md](../mcp.md) if your issue touches automation
