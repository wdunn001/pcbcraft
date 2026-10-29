# Agent role briefs

Token-sized summaries for parallel headless workers. Canonical rules: [AGENTS.md](../../AGENTS.md), [CONTRIBUTING.md](../../CONTRIBUTING.md), fork onboarding [issue #1](https://github.com/wdunn001/pcbcraft/issues/1).

| Role | Find work | Output | Lease |
| --- | --- | --- | --- |
| Researcher | `stage:research` | `### Research` comment, `research:done` | 2 h |
| Enricher | `stage:enrich` | `### Enrichment`, full Tags line, `ready` | 2 h |
| Worker | `ready` + `status:open` (+ `conflict:parallel-safe` if shared host) | branch + green fork Actions + build report | 6 h |
| Reviewer | branch with report | `### Review` (read/run only, no edits) | 2 h |
| Integrator | approved branches | merge to fork `main`, green Actions there | maintainer |

**Parallelism:** prefer many workers on `conflict:parallel-safe` issues; one agent per clone; never share checkouts. **CI:** fork Actions is proof; local `cargo xtask ci` optional.

Role files (optional detail): [researcher.md](researcher.md), [enricher.md](enricher.md), [worker.md](worker.md), [reviewer.md](reviewer.md).
