# Issue tagging standard

Every issue starts with a Tags line. The generator reads only that line. Tags are backticked words after `**Tags:**`.

```
**Tags:** `track:ai-handoff` `kind:feature` `area:layout` `status:open` `ready`
```

## Tag namespaces

| Namespace | Values |
|-----------|--------|
| track | `track:ai-handoff` an agent can take it without asking; `track:ui-design` it changes what the user sees |
| kind | `kind:feature` `kind:epic` `kind:bug` `kind:research` `kind:chore` `kind:refactor` `kind:docs` |
| area | `area:schematic` `area:layout` `area:library` `area:rules` `area:io` `area:artifacts` `area:3d` `area:sim` `area:ui-shell` `area:mcp` `area:ci` `area:docs` `area:packaging` `area:kb` |
| milestone | `milestone:m1` to `milestone:m4` from ROADMAP.md, `milestone:m5` Inner copper layers (epic #316), `milestone:m6` Manufacturing artifacts (epic #322) |
| crate | `crate:engine` `crate:doc` `crate:rules` `crate:io` `crate:geom` `crate:library` `crate:mcp` `crate:ui-egui` `crate:xtask` `crate:sim` `crate:apps` |
| conflict | `conflict:ui-egui-lib` edits crates/ui-egui/src/lib.rs; `conflict:parallel-safe` stays out of that file |
| size | `size:s` one PR, one crate, under a day; `size:m` one PR, several crates; `size:l` several PRs or an epic |
| machine | `machine:neoden-yy1` `machine:eufymake-e1` `machine:snapmaker-cnc` `machine:commarker-b6` |
| priority | `priority:p0` `priority:p1` `priority:p2` `priority:p3` |
| status | `status:open` `status:claimed` `status:in-progress` `status:pr-open` `status:needs-owner-test` `status:blocked` `status:review` |
| stage | `stage:research` `stage:enrich` `ready` (bare; an issue never carries both `ready` and a `stage:` tag) |
| research | `research:needed` `research:done` `research:n/a` |
| enrich | `enrich:done` (an `### Enrichment` comment exists) |
| flag | `owner-decision` the maintainer must choose something; say what in the body or a comment |

## Status values

- `status:open` the issue is available to claim
- `status:claimed` an agent has claimed it (see claiming rules in CONTRIBUTING.md)
- `status:in-progress` code is being written or pushed
- `status:pr-open` an upstream pull request is open
- `status:needs-owner-test` the branch is pushed and the maintainer must test
- `status:blocked` waiting on an external decision or dependency
- `status:review` a reviewer holds the branch; the reviewer moves it to `needs-owner-test` on pass or back to `in-progress` on fail

## Legacy status values

These values appear on older issues. They map to current values:

- `status:split` maps to `status:blocked` with the children named in the body
- `status:built` maps to `status:needs-owner-test`
- `status:ready` maps to the bare `ready` tag (not a `status:` value)

Issues carrying these legacy values should be retagged to the current vocabulary.

## Stage derivation

No enrichment and research needed gives `stage:research`. No enrichment and research done or n/a gives `stage:enrich`. Enrichment done and research still needed gives `stage:research`. Enrichment done and research done or n/a gives `ready`.

## Who sets which tag

- Reporter: the first line, with at least `track:`, `kind:`, `area:`, `status:open` and the stage.
- Researcher: `research:done`, and the move from `stage:research` to `stage:enrich` (or to `ready` if enrichment already happened).
- Enricher: the whole line, including `enrich:done` and `ready`.
- Worker: `status:claimed` on `CLAIM`, `status:in-progress` on first push, `status:needs-owner-test` with the build report.
- Maintainer: `status:pr-open` when the upstream pull request opens.
- Anyone: correct a wrong `area:`, `crate:` or `conflict:` tag; set `owner-decision` on hitting a question; clear it when recording the answer.
