# Contributing to PCBCraft

This file explains how work moves through the PCBCraft tracker, from a request to a merged change. It is written for people and for agents, including outside agents that claim issues on their own. If you only want to fix something small, read "Worker" and "Definition of Done" and skip the rest.

**This repository, github.com/wdunn001/pcbcraft, is the project's home. Code, issues, pull requests and CI all live here.** The old tracker on tangency-labs/pcbcraft is frozen and kept only as history: do not post, comment, claim or open pull requests there. Issues moved from it keep their title, tags and comments, and their body says `Migrated from tangency-labs/pcbcraft#N`; search that text to find an old number. The start-here issue, [#1](https://github.com/wdunn001/pcbcraft/issues/1), is the short version of this file and wins if the two ever disagree.

Read code on `main`, branch from it, merge the latest `main` into your branch before your final push, and resolve conflicts on your side. CI is GitHub Actions on this repository and runs on every pull request. In this file "main" means `main` on wdunn001/pcbcraft, and `Base: main at <sha>` is the first line of every `### Research`, `### Enrichment` and `### Review` comment, so everyone names the same code.

**Definition of done:** a change is done when it is merged into `main` and the GitHub Actions build on `main` is green. Everything else in this file serves that.

Most of the work on the board is done by AI agents, and the board is the coordination channel. There is no chat or side channel to join. Everything that matters happens in issue comments, the `**Tags:**` line, labels and pull requests, so any team of people or agents can take part by following the same rules.

**Start here.** Get the latest code from `main`. Find work: `gh issue list -R wdunn001/pcbcraft --state open --label ready --label status:open`, then claim one with a `CLAIM` comment (section 3). Deliver by opening a pull request against `main` with the build report in its body, and wait for green CI. An integrator verifies and merges it. Issues come first: nothing is built that is not an enriched, tagged issue.

## 1. The pipeline at a glance

Every piece of work becomes an issue before anyone touches code. The issue then moves through fixed stages. Tags on the first line of the issue body say which stage it is in.

```
 request                 raw ticket               researched                enriched
 (owner, user, agent) -> [reporter] ------------> [researcher] -----------> [enricher] ---------> ready
                         stage:research           research:done             enrich:done           `ready`
                         or stage:enrich          stage:enrich              full Tags line
                         research:n/a

 ready --> [worker claims] --> [worker builds] ---> pull request ----------> [integrator] ---------> closed
           CLAIM <agent>       own clone off          against main, CI         verifies, merges      by the tracker
           status:claimed      main, merges main      runs, build report       on green CI,          manager when the
                               before the final       in the PR body           merge commit          PR is merged and
                               push                   status:in-progress                             CI on main is green
```

Stages in words:

1. **Raw ticket.** A reporter files a short issue: the request in the requester's words, the goal, links. Tags: `stage:research`, or `stage:enrich` plus `research:n/a` when there is no open standards or design question.
2. **Research.** A researcher answers the standards, format, machine or reference-rule questions, posts a `### Research` comment, sets `research:done` and moves the issue to `stage:enrich`.
3. **Enrichment.** An enricher verifies file and function names, lists every place to change, dependencies and order, the conflict lane, the test list, tooltips, text catalog, accessibility and logging needs, and splits the work into one-PR children. It posts `### Enrichment`, writes the full Tags line, sets `enrich:done` and replaces `stage:enrich` with `ready`.
4. **Ready.** A worker may claim it. Workers claim only `ready` issues. Workers who share a build machine with others also need `conflict:parallel-safe`.
5. **Work.** The worker claims, branches off `main`, builds, tests, merges the latest `main` back in, opens a pull request against `main` with the build report in the body, and waits for the Actions run on the pull request.
6. **Integration.** The integrator verifies the pull request and merges it with a merge commit when CI is green. When the Actions build on `main` is green, the change is done.
7. **Closing.** The tracker manager closes the issue when the pull request that finishes it has merged and CI on `main` is green. Partial work leaves the issue open with the pull request linked.

A reviewer may add a `### Review` comment on the issue or the pull request. It is optional and never replaces CI.
5. **Work.** The worker claims, branches off fork `main`, builds, tests, merges the latest fork `main` back in, pushes the branch, waits for the Actions run, and posts a build report ending in "PR pending owner review".
6. **Review.** A reviewer checks the pushed branch against the acceptance criteria and posts `### Review`.
7. **Integration.** The integrator merges the branch into fork `main` with a merge commit. The merge message ends with a line `Issues: #N, #M` listing every issue the branch delivers, with no other text on that line. When the Actions build on fork `main` is green, the change is done.
8. **Owner test.** The maintainer runs fork `main`. Only then does a pull request to upstream open.

## 2. Roles, step by step

Replace `N` with the issue number and `<agent>` with your handle (a GitHub login or an agent name such as `claude-agent-docs`). All commands use the GitHub CLI, `gh`, authenticated as you.

### Reporter

Anyone who turns a request into an issue. A coordinator does this for the maintainer's requests; outside contributors may file their own.

1. Search first so the request is not already tracked:
   ```sh
   gh issue list -R wdunn001/pcbcraft --search "<keywords>" --state all
   ```
2. Create the issue with the body template below. The first line is the Tags line. Keep the How section short or write "To be completed by the enrichment stage"; the enricher fills it in.
   ```sh
   gh issue create -R wdunn001/pcbcraft --title "<plain title>" --body-file body.md
   ```
3. Choose the stage. If the work depends on an open standards, file format, machine or reference-rule question, tag `stage:research` and `research:needed`. Otherwise tag `stage:enrich` and `research:n/a`.
4. Add the new issue to the program status epic #95 (comment on it, or edit if you have access).

Body template (the sections after Acceptance are copied verbatim into every issue):

```markdown
**Tags:** `track:ai-handoff` `kind:feature` `area:layout` `status:open` `stage:enrich` `research:n/a`
## What
One paragraph: what changes for the user or the code.

## Why
Who asked, what it unblocks, which issues depend on it.

## How
To be completed by the enrichment stage.

## Acceptance
- Observable results, each one testable. Name the `a_newcomer_*` test for UI work.

### Rules for whoever picks this up
- Branch off `main` on wdunn001/pcbcraft and merge the latest `main` into your branch before your final push. Open a pull request against `main`; never push to `main`.
- New code and PR text may not name other EDA tools.
- Typed engine commands with bounded validation; no unwrap/expect/panic anywhere, tests included.
- The GitHub Actions run on your pull request must be green; it is the proof. The pull request body is the build report.
- Every new control gets a tooltip; panels keep real margins and padding.
- End-to-end tests in crates/ui-egui are named like `a_newcomer_*` and must read like a story for someone who has never seen the app.

### Claiming
Comment `CLAIM <agent>@<team> until <UTC time> branch <name>` to take this. A coder's claim lasts 6 hours and any other stage's claim lasts 2 hours; renew with `CLAIM-RENEW <agent>`, release with `RELEASE <agent>`. The earliest claim wins. An expired claim may be taken over.
```

### Researcher

Takes `stage:research` issues. The deliverable is a comment, not code.

1. Find work:
   ```sh
   gh issue list -R wdunn001/pcbcraft --state open --label stage:research
   ```
2. Check what is already known before reading anything new: the `### Enrichment` and `### Research` comments on this issue and on every issue it links, and the research notes (ask on the issue). Open the comment with what was already covered and where, so the same defect is not researched twice.
3. Read the published specifications, standards and reference material the question needs. Read the code paths the issue names, so the findings say what the code does today and what the specification says it must do.
4. Post one `### Research` comment. Format:
   ```markdown
   ### Research

   Base: main at <sha>
   Already covered: #111 enrichment items 1, 2 and 6. New here: items 3 to 5.
   One line on what was read (specification name and revision, code paths).

   1. **Finding title.** What the code does, what the standard requires, file and line anchors, the change this implies.
   2. **Next finding.** ...

   Owner decision: the question the maintainer must answer, if any, stated as a choice.
   ```
   Research comment on #176 is the model: numbered findings, each tied to a section of the specification and a place in the code, ending with one owner decision.
5. Update the Tags line: add `research:done`, replace `stage:research` with `stage:enrich`. If `enrich:done` is already present, set `ready` instead of `stage:enrich`. If the maintainer must decide something, add `owner-decision`, say what the choice is, and add one line to the "Open decisions for the owner" list in #95 (section 3, "Owner decisions").
6. Captured sources (specification pages, datasheets, application notes) go into the project's knowledge base. Ask on the issue if you want a source captured; the maintainer's research agents maintain the manifest.

### Enricher

Takes `stage:enrich` issues. The enricher owns the Tags line and makes the issue buildable without further questions.

1. Find work:
   ```sh
   gh issue list -R wdunn001/pcbcraft --state open --label stage:enrich
   ```
2. Read the code on `main` and record the commit: the first line of the enrichment is `Base: main at <sha>`. If the work must build on a branch not yet merged into `main`, say so on the next line (`Stacks on: feat-<slug> at <sha>`) and prefer to have that branch integrated first. Anchors are file names plus function, type or constant names (`crates/doc/src/lib.rs: Pad::outline_points, Pad::bounding_radius`), never bare line numbers; line numbers go stale within a day on this repository, names can be grepped. Verify every anchor exists at the recorded commit before posting.
3. Post one `### Enrichment` comment. Paste the Definition of Ready checklist (section 4) at the top and tick it, with "n/a" and one word of reason where an item does not apply, then cover, in this order:
   - Corrections to the issue body (for example "there is no pad shape enum; shape is a free string in two places").
   - Every place that must change, grouped by crate, with anchors.
   - Free wins: code paths that start working with no change once the core change lands.
   - Behaviour details the worker must honour (validation bounds, closed rings, convexity assumptions, error codes).
   - Existing bugs in the same code that this issue should fix while it is there.
   - Tests to add, named, per crate, including the `a_newcomer_*` story test for any UI work.
   - Tooltips, text catalog entries, accessibility and logging needs.
   - Ordering and dependencies against other issues, and the conflict lane.
   - Out of scope: what this issue does not do, with the follow-up issue number.
   - Split into one-PR children when the work is `size:l`; file the children and list them.
   The enrichment on #105 is the model.
4. Rewrite the whole Tags line in the same pass: track, kind, area, milestone, crate, conflict, size, machine, status, then `ready` (when research is done or n/a) or `stage:research` (when research is still needed), then `research:*` and `enrich:done`. How to edit the line safely is in section 3.
5. If an owner decision is needed before work can start, add `owner-decision` and `status:blocked`, say what the choice is, and add it to the decision list in #95.
6. Scope added later goes through the enricher again. Anyone who adds a requirement to an issue after the `### Enrichment` comment posts it as `### Enrichment (update)` and refreshes the test list and the Tags line in the same comment. A loose "scope note" comment with no update does not count; a worker who finds one asks for the update before claiming.

### Worker

Takes `ready` issues. A worker is a human or an agent that writes the code.

**Find work**

```sh
# everything a worker may claim
gh issue list -R wdunn001/pcbcraft --state open --label ready --label status:open
# only what can run alongside other workers (required when you share a build machine)
gh issue list -R wdunn001/pcbcraft --state open --label ready --label status:open --label conflict:parallel-safe
# the maintainer's top priorities
gh issue list -R wdunn001/pcbcraft --state open --label priority:owner-top
```

Read the issue body, every `### Research` and `### Enrichment` comment, and the linked issues. Check the program status epic #95 and the open pull requests (`gh pr list -R wdunn001/pcbcraft`) for who is working nearby.

**Claim**

Read the issue comments from newest to oldest first. A claim is live if its `until` time is in the future and no `RELEASE` follows it. If a live claim exists, pick another issue. Otherwise post one comment:

```sh
gh issue comment N -R wdunn001/pcbcraft --body "CLAIM <agent>@<team> until <UTC time> branch <name>"
```

Optionally add one or two lines of plan under the claim. Then set `status:claimed` on the Tags line (section 3). Do not claim more than one issue at a time unless they are stacked children of one epic. Your lease is 6 hours; renew it with `CLAIM-RENEW <agent> until <UTC time>` before it runs out, and post `RELEASE <agent>` if you stop. If two claims land close together, the earliest comment wins and the later claimant posts `RELEASE <agent>` and moves on. An expired claim may be taken over.

**Branch**

One clone or worktree per agent. Never work in a checkout another agent is using. Clone the repository and branch off its `main`:

```sh
git clone https://github.com/wdunn001/pcbcraft.git pcbcraft-<slug>
cd pcbcraft-<slug>
git checkout -b feat-<slug> origin/main
```

Branch from the base the enrichment recorded (`Base: main at <sha>`, or the `Stacks on:` branch when there is one). If your work depends on an open pull request, branch from that pull request's branch and say so in your pull request. Branch names follow section 7. If you do not have push access to wdunn001/pcbcraft, work on your own copy of it and open the pull request from there.

Before your final push, bring the latest `main` into your branch and resolve conflicts on your side, so the integrator receives a branch that merges cleanly:

```sh
git fetch origin main
git merge origin/main
# resolve, build, test, commit the merge
```

A pull request that does not merge cleanly into `main` goes back to the worker.

**Build and test**

CI is GitHub Actions on wdunn001/pcbcraft, and it runs on every pull request. The Actions run is the proof of a green build; link it in your pull request. Running the gate yourself before pushing is optional and saves round trips:
CI is GitHub Actions on the fork, and it runs on pull requests and on `main`, not on plain branch pushes. Open a draft pull request to get the cheap checks (format, policy, clippy, docs); mark it ready for review to run the full gate. The green run is the proof of a build; link it in your report. Running the gate yourself before pushing is optional and saves round trips:

```sh
cargo xtask ci
```

It runs formatting, dependency layer checks, clippy with warnings denied, the workspace tests (including the `a_newcomer_*` end-to-end tests) and the WebAssembly check. Once the CI pipeline branch (#164) is merged, the Actions gate also runs policy, docs, MSRV and license checks, the full e2e suite and the screenshot sheet; `docs/ci.md` lists every job and its local command. If you build on a shared machine, cap parallelism (`CARGO_BUILD_JOBS=4`) and keep your own target directory; do not run full builds on a laptop when a shared build host or Actions can do it.
It runs formatting, dependency layer checks, clippy with warnings denied, the workspace tests (including the `a_newcomer_*` end-to-end tests) and the WebAssembly check. `docs/ci.md` lists every job and its local command. CI does not capture screenshots. If you build on a shared machine, cap parallelism (`CARGO_BUILD_JOBS=4`) and keep your own target directory; do not run full builds on a laptop when a build machine or Actions can do it.

For UI changes, look at the rendered result (section 6).

**Push and open the pull request**

```sh
git push -u origin feat-<slug>
gh pr create -R wdunn001/pcbcraft --base main --head feat-<slug> --title "<plain title> (#N)" --body-file report.md
```

Push only your feature branch. Never push to `main`. Open the pull request against `main` on wdunn001/pcbcraft; the body is the build report. Set `status:in-progress` on the issue on the first push. Do not merge your own pull request: an integrator verifies and merges it.

**Report**

The pull request body is the build report. Do not repeat it in an issue comment. Format:

```markdown
Implemented on branch feat-<slug>, commit <sha>, based on main at <sha>, 7 commits above the base.

Built:
- What landed, per crate, in plain sentences.

Bugs fixed in passing:
- ...

Acceptance:
- criterion 1: test `clearance_to_a_chamfered_corner_measures_the_cut_edge`
- criterion 2: not done, reason, follow-up #NNN

Tests: first lines of the `cargo xtask ci` output, not the word green. CI run: <link>.

Files touched: list, and "crates/ui-egui/src/lib.rs untouched" or the lane ticket number.

Screenshots: before and after, in this body (UI changes only).

Limitations: what the change does not do, stated plainly, with follow-up issue numbers.

Definition of Done:
- [x] ... (the list from section 4, ticked)

Issues: #N
```

The last line names the issues the pull request moves, and says which ROADMAP.md lines it advances. Nothing else is required of the worker; the integrator picks the pull request up once CI is green.

### Reviewer

Optional. Checks a pull request against its issue before or while the integrator verifies it.

1. Find work:
   ```sh
   gh pr list -R wdunn001/pcbcraft --state open
   ```
2. Fetch the branch into your own read-only clone and read the diff against its recorded base. Start the `### Review` with `Base: main at <sha>`. Confirm the branch merges cleanly into current `main`; one that does not goes back to the worker. Check that the Actions run on the pull request is green for the branch head; if it is red, fail it.
3. Walk the Acceptance list and the test list from the enrichment, running each named test. Check the rules in section 5 (no unwrap or panic, typed commands, validation bounds, no other tools named in the diff or in the comments, tooltips, margins, attribution). Confirm `crates/ui-egui/src/lib.rs` is untouched when the issue is `conflict:parallel-safe`. For UI work, open the screenshots in the pull request body. The reviewer reads and runs; it never edits the branch.
4. Post one `### Review` comment on the pull request:
   ```markdown
   ### Review

   Base: main at <sha>

   Branch feat-<slug> at <sha>.

   Acceptance:
   - [x] criterion, evidence (test name, file, screenshot)
   - [ ] criterion, what is missing

   Rules: pass, or the rule and the place it is broken.

   Verdict: approved for integration
   ```
   or `Verdict: changes requested` followed by the list. The worker fixes and pushes; the reviewer reviews again.
5. The reviewer does not merge and does not change the status tag.

### Integrator

Integrators verify pull requests and merge them into `main`, so `main` is always the integrated build.

1. Watch `gh pr list -R wdunn001/pcbcraft --state open` for pull requests with green CI. Workers do not need to ask.
2. Verify the pull request: it merges cleanly, the report is complete, the Definition of Done is ticked and the rules in section 5 hold. Wait until the pull request's "CI result" check is green (`main` has no required checks, so do not use `--auto`: it merges at once). Then merge it with a merge commit, never a squash or a rebase of `main`:
   ```sh
Verdict: approved for integration
   ```
   or `Verdict: changes requested` followed by the list. The worker fixes and pushes; the reviewer reviews again.
5. The reviewer does not merge and does not change the status tag.

### Integrator

Integrators verify pull requests and merge them into `main`, so `main` is always the integrated build.

1. Watch `gh pr list -R wdunn001/pcbcraft --state open` for pull requests with green CI. Workers do not need to ask.
2. Verify the pull request: it merges cleanly, the report is complete, the Definition of Done is ticked and the rules in section 5 hold. Wait until the pull request's "CI result" check is green (`main` has no required checks, so do not use `--auto`: it merges at once). Then merge it with a merge commit, never a squash or a rebase of `main`:
   ```sh
   git checkout main && git pull --ff-only origin main
   git merge --no-ff feat-<slug>
   gh pr merge N -R wdunn001/pcbcraft --merge
   ```
   End the merge commit message with a line `Issues: #N, #M` listing every issue the branch delivers, with no other text on that line.
3. Resolve conflicts in the merge commit, keeping both sides' intent; if the resolution is not obvious, ask on both issues before merging/pushing.
4. Push the merge, then watch the GitHub Actions run on `main`. The change is done when that full build is green; if it is red, fix forward on a branch with its own pull request or report on the issue, and never rewrite `main`. Where a local run is cheap, run `cargo xtask ci` on the merged tree before the push:
   ```sh
   git push origin main
   ```
5. Never rewrite `main` and never push to it directly. All changes arrive by pull request.
6. Keep a ledger in `integration-ledger.md`: date, branch, branch head, issue, fork main before and after, Actions run, conflict files and how each was resolved. Comment on each issue with the merge commit, and post any resolution that changes logic so the worker and reviewer see it.

### Owner test

The maintainer tests on fork `main`. Screenshots are local (`cargo xtask screenshots --out target/screenshots`). Only after the maintainer has tested:

- The maintainer opens one pull request per feature from the feature branch to upstream `main`, with a body that starts with the Tags line, and sets `status:pr-open` on the issue.
- Upstream pull requests merge only with green CI.
- When upstream merges, the issue closes, and the status epic #352 is updated.

If the maintainer finds a problem, they comment on the issue; the status goes back to `status:in-progress` and the worker continues on the same branch.

## 3. Tags and claiming

Tags live on the first line of every issue and pull request body, as one `**Tags:**` line of backticked tags, and the same tags are applied as GitHub labels, so `gh issue list --label` works. The body line is the source: when the two differ, fix the label to match. The order is: track, kind, area, milestone, crate, conflict, size, machine, status, stage (or `ready`), research, enrich, owner-decision.

```
**Tags:** `track:ai-handoff` `kind:feature` `area:layout` `milestone:m2` `crate:engine` `crate:ui-egui` `conflict:ui-egui-lib` `size:m` `status:open` `ready` `research:n/a` `enrich:done`
```

Vocabulary, one line each. "One" means exactly one value.

| Namespace | Values |
| --- | --- |
| track (one or more) | `track:ai-handoff` an agent can take it without asking; `track:ui-design` it changes what the user sees |
| kind (one) | `kind:feature` `kind:epic` `kind:bug` `kind:research` `kind:chore` `kind:refactor` `kind:docs` |
| area (one or two) | `area:schematic` `area:layout` `area:library` `area:rules` `area:io` `area:artifacts` `area:3d` `area:sim` `area:ui-shell` `area:mcp` `area:ci` `area:docs` `area:packaging` `area:kb` |
| milestone (zero to two) | `milestone:m1` to `milestone:m4` from ROADMAP.md, `milestone:m5` Inner copper layers (epic #140), `milestone:m6` Manufacturing artifacts (epic #146) |
| crate (one or more) | `crate:engine` `crate:doc` `crate:rules` `crate:io` `crate:geom` `crate:library` `crate:mcp` `crate:ui-egui` `crate:xtask` `crate:sim` `crate:apps` |
| conflict (one) | `conflict:ui-egui-lib` edits crates/ui-egui/src/lib.rs; `conflict:parallel-safe` stays out of that file |
| size (one) | `size:s` one PR, one crate, under a day; `size:m` one PR, several crates; `size:l` several PRs or an epic |
| machine (zero or more) | `machine:neoden-yy1` `machine:eufymake-e1` `machine:snapmaker-cnc` `machine:commarker-b6` |
| status (one) | `status:open` `status:claimed` `status:in-progress` `status:pr-open` `status:blocked` |
| status (one) | `status:open` `status:claimed` `status:in-progress` `status:pr-open` `status:needs-owner-test` `status:blocked` `status:review` |
| stage (one) | `stage:research` `stage:enrich` `ready` (bare; an issue never carries both `ready` and a `stage:` tag) |
| research | `research:needed` `research:done` `research:n/a` |
| enrich | `enrich:done` (an `### Enrichment` comment exists) |
| flag | `owner-decision` the maintainer must choose something; say what in the body or a comment |

Deriving the stage: no enrichment and research needed gives `stage:research`; no enrichment and research done or n/a gives `stage:enrich`; enrichment done and research still needed gives `stage:research`; enrichment done and research done or n/a gives `ready`.

**Who sets which tag**

- Reporter: the first line, with at least `track:`, `kind:`, `area:`, `status:open` and the stage.
- Researcher: `research:done`, and the move from `stage:research` to `stage:enrich` (or to `ready` if enrichment already happened).
- Enricher: the whole line, including `enrich:done` and `ready`.
- Worker: `status:claimed` on `CLAIM`, `status:in-progress` on first push, `status:pr-open` when the pull request opens.
- Tracker manager: keeps tags and labels in sync, links duplicates, and closes the issue when its pull request has merged and CI on `main` is green.
- Anyone: correct a wrong `area:`, `crate:` or `conflict:` tag; set `owner-decision` on hitting a question; clear it when recording the answer.

**The conflict lane.** `crates/ui-egui/src/lib.rs` holds the tabs, docks, menus and inspector sections, and nearly every UI change lands there. It is edited serially: one `conflict:ui-egui-lib` ticket at a time, by the maintainer's UI shell lane, and nobody else touches that file in parallel. If your work needs a line in lib.rs, say so in the issue and wait for the lane rather than editing it; `conflict:parallel-safe` work proceeds alongside anything else.

**Editing the Tags line.** Several agents edit issues at once. Fetch, change the first line only, write back in the same step, and never touch other body text or comments:

```sh
gh issue view N -R wdunn001/pcbcraft --json body -q .body > body.md
# edit line 1 only
gh issue edit N -R wdunn001/pcbcraft --body-file body.md
```

**Claiming and leases.** Claim a `ready` issue with `status:open` by posting one comment: `CLAIM <agent>@<team> until <UTC time> branch <name>`. Do not put machine names or local paths in it: the tracker is public. A lease lasts 6 hours for coders and 2 hours for research, enrichment and review. Renew it with `CLAIM-RENEW <agent> until <UTC time>` before it runs out; every push also renews it. A claim is live if its `until` time is in the future and no `RELEASE` follows it. If two claims land close together, the earliest comment wins; the later claimant posts `RELEASE <agent>` and moves on. An expired claim may be taken over: comment `CLAIM <agent> (takeover, previous claim expired)`, set `status:claimed`, and the earlier claimer stops. Before claiming, check the issue has no live lease. To stop, post `RELEASE <agent>` and set `status:open`. Claims that do not use the format fall back to 5 days with no pushed branch.

**Questions and blockers.** Comment `BLOCKED: <reason>` or `QUESTION: <question>` on the issue and keep your claim or release it. Decisions that belong to the maintainer are tagged `owner-decision`; do not build past one.

**Pace your writes.** Many agents share GitHub's write limits. Keep issue and pull request writes a few seconds apart. If GitHub says "submitted too quickly", wait 10 minutes instead of retrying.

**Comment headings in use.** `CLAIM <agent>`, `### Research`, `### Standards` (research on rule and output issues), `### Enrichment`, `### Enrichment (update)`, `### Review`, `CLAIM`, `CLAIM-RENEW` and `RELEASE`. The build report is the pull request body. Use these exact headings so searches and the status epic find them.

**Owner decisions.** A question only the maintainer can answer (strategy, accounts, signing, a convention such as the internal Y axis direction) does not wait in a comment thread. Whoever hits it sets `owner-decision` on the issue and adds one line to the "Open decisions for the owner" list in #95: `- [ ] #176: Y-up internally with a display flip? (asked by research, 2026-10-28)`. The maintainer answers on the issue or in #95; whoever records the answer clears the tag and ticks the line.

## 4. Definition of Ready and Definition of Done

Both lists are pasted into the issue and ticked, not only read: the enricher pastes Ready at the top of `### Enrichment`, the worker pastes Done at the end of the build report. "n/a" with one word of reason is an acceptable tick; a blank box blocks the stage. Keep the lists to what the next person cannot discover alone.

**Definition of Ready** (the enricher ticks this before setting `ready`):

- [ ] Goal in one sentence, in the requester's words, with the owner direction quoted if one exists.
- [ ] Acceptance criteria as numbered observable statements: what the user sees or what the test asserts.
- [ ] Research is done or marked n/a; `### Research` findings are reflected in How.
- [ ] Test list: exact test names and the file each goes in, including the `a_newcomer_*` story test for UI work.
- [ ] Anchors: file plus function, type or constant names, verified by reading the code at `Base: main at <sha>`. No bare line numbers.
- [ ] `Base: main at <sha>` recorded and conflict lane set (`conflict:ui-egui-lib` or `conflict:parallel-safe`).
- [ ] Dependencies: issues that must land first, and what breaks if they do not; blocked issues carry `status:blocked`.
- [ ] Out of scope: what this issue does not do, with the follow-up issue number.
- [ ] Tooltips, text catalog, accessibility and logging needs listed for UI work.
- [ ] Owner decisions: none, or listed with `owner-decision` set and a line in #95.
- [ ] Text rule: title and body name no other EDA tool.
- [ ] Size is `s` or `m`; `size:l` work is split into children first. The full Tags line is set.

**Definition of Done** (the worker ticks this in the build report; the reviewer and integrator check it):

- [ ] Branch, commit, merge-base and the number of commits above the base are in the pull request body, and `main` was merged into the branch before the final push.
- [ ] Each acceptance criterion is listed with the test name or screenshot that proves it, or "not done" with the reason and a follow-up number.
- [ ] Every test from the enrichment exists and passes; new behaviour has unit tests; file writers have golden fixtures; UI work has a readable `a_newcomer_*` test.
- [ ] The link to the green GitHub Actions run on the pull request for the same commit is in the body, with the first lines of the `cargo xtask ci` output if you ran it locally.
- [ ] No `unwrap`, `expect`, `panic`, `todo`, `unimplemented` or `unreachable` in production code; invalid input leaves state untouched.
- [ ] Every edit goes through a typed engine command with bounded validation.
- [ ] Every new control has a tooltip; panels keep real margins and padding; labels are short and plain.
- [ ] For `track:ui-design`: screenshots attached or linked.
- [ ] Files touched are listed; `crates/ui-egui/src/lib.rs` is untouched for `conflict:parallel-safe` work, or the lane ticket is named.
- [ ] No other EDA tool is named in code, comments, UI strings, docs, commit messages or the report.
- [ ] Adapted code is recorded in ATTRIBUTION.md with source, revision and license.
- [ ] Limitations and follow-ups are stated with issue numbers, and shown in the UI where the user would meet them.
- [ ] Pull request opened against `main` on wdunn001/pcbcraft; the body ends with an `Issues: #N` line.

After the worker: the integrator has verified the pull request and merged it into `main` with a merge commit, and the Actions build on `main` is green, which is the definition of done; the tracker manager has closed the issue; #95 is updated.

## 5. Code rules

These come from the rule files in `.cursor/rules/` (listed in AGENTS.md) and apply to every change.

- **One window.** Document and workspace tabs plus docked panes. Never spawn separate schematic, board, library, checks or 3D windows. Project and selection are shared.
- **Typed engine commands.** Everything that edits the project is a command in `crates/engine` with a JSON schema. UI, CLI and MCP call the same engine. Programmatic commands never open dialogs.
- **Validation.** Finite, bounded coordinates and dimensions; bounded counts; stable IDs; reference integrity. An invalid command or batch leaves the document, revision and history untouched.
- **No panics.** No `unsafe`; no production `unwrap`, `expect`, `panic`, `todo`, `unimplemented` or `unreachable`. Tests may use `?` with `Result<(), String>`; the e2e tests do. The CI pipeline branch lints these at deny and forbids source `allow` of them.
- **Crate layering.** geom (L0) under doc and library (L1) under rules and io (L2) under engine (L3) under mcp and ui-egui (L4) under the apps. Lower crates never depend on egui, eframe, wgpu or MCP. `cargo xtask ci` checks the layers.
- **Adapted code.** PCBCraft is MIT OR Apache-2.0, so only code under a compatible permissive license (MIT, Apache-2.0, BSD, ISC, Unlicense and the like) may be adapted. Never copy or translate GPL, LGPL or AGPL code; read it for ideas at most. Record the exact source, revision and license in ATTRIBUTION.md. Translating to Rust does not remove attribution. Check each file's license. Do not copy trademarks or proprietary assets from shared Craft dependencies; pin craft-libs revisions.
- **Honest limits.** Do not present initial checks as complete ERC/DRC, or a basic route as a shove router. Track parity by workflows and fixtures, not by percentages.
- **Names.** Never name other EDA tools in code, docs, UI strings, issues or pull requests. Describe formats and behaviours instead ("the board interchange format", "the plot layer format").
- **UI.** Every control gets a tooltip. Panels keep real margins and padding. Labels are short plain words. UI strings go in `crates/text/strings/en.txt`. Status dots, arrows and ticks are painted shapes or icons, never text characters. UI changes are inspected rendered, not only in tests. The design is in [docs/design-system.md](docs/design-system.md) and [docs/page-layouts.md](docs/page-layouts.md); read them before UI work and update them when you change a documented rule.
- **Changeset and docs.** Every pull request adds one changeset file in `.changes/` (see `.changes/README.md`) and updates the docs it touches, or states `docs: n/a: <reason>`. CI job "Changeset and docs" enforces it.
- **Publishing.** Do not publish releases or push to third-party repositories without authorization.

## 6. Testing

- **Unit tests** live beside the code in each crate (`#[cfg(test)]` modules) and cover validation bounds, rejections and the happy path. Name them as sentences: `padstack_rejects_chamfer_ratio_out_of_range`.
- **Golden fixtures** cover every file writer and reader. Put fixture files under `crates/<crate>/tests/fixtures/` and compare whole outputs, not substrings. For output that an independent parser exists for, read the output back with that parser as a dev-dependency. Fixtures from external libraries keep their license file beside them.
- **End-to-end tests** are named `a_newcomer_<does something>`. They drive the real `App` headlessly with the `frame`, `tap`, `paints_text` and `scrolled_to` helpers, and every assertion message is a plain sentence that someone who has never seen the app could follow ("the matching part stays in the list"). One test per user-visible feature. They live in `crates/ui-egui/src/e2e/<area>.rs` (helpers in `src/e2e/support.rs`), one file per area (library, nets, rules, builds, board, checks, artifacts, layout, layers, shell). Put new tests in the matching area file, or add a new area file plus one `mod <area>;` line in `src/e2e/mod.rs`, and never add one to lib.rs. CI runs the full e2e suite as a required check, uploads a screenshot sheet, and fails if any control lacks a tooltip or an e2e story.
- **Coverage.** Line coverage is gated in CI (job 'Coverage'): the bar is 70% and crates may not fall below their recorded baseline; tests must assert behaviour, and a test written only to execute code is not acceptable.
- **End-to-end tests** are named `a_newcomer_<does something>`. They drive the real `App` headlessly with the `frame`, `tap`, `paints_text` and `scrolled_to` helpers, and every assertion message is a plain sentence that someone who has never seen the app could follow ("the matching part stays in the list"). One test per user-visible feature. They live in `crates/ui-egui/src/e2e/<area>.rs` (helpers in `src/e2e/support.rs`), one file per area (library, nets, rules, builds, board, checks, artifacts, layout, layers, shell). Put new tests in the matching area file, or add a new area file plus one `mod <area>;` line in `src/e2e/mod.rs`, and never add one to lib.rs. CI does not run the e2e suite and does not capture a screenshot sheet. Run `cargo xtask e2e` and `cargo xtask screenshots` locally when you need them.
- **Engine round trips** for any new document field: save and load a project carrying it and compare.
- **Screenshots for UI changes.** Render the change and put before and after images in the pull request body. On `main` today:
  ```sh
  xvfb-run -a -s '-screen 0 1600x1100x24' \
    env -u WAYLAND_DISPLAY WGPU_BACKEND=vulkan \
    cargo run -p pcbcraft -- --sample --capture pcbcraft.png
  ```
  `cargo xtask screenshots --out target/screenshots` (from the CI pipeline, #164) captures the standard views, and CI uploads a side-by-side sheet of base and head. Add a view there when a new tab or dialog deserves one. If your issue carries `track:ui-design`, you produce the screenshots yourself and put the sheet path in your report; never ask the owner for evidence you can produce.
- **Where to run.** CI runs on GitHub Actions on wdunn001/pcbcraft. Open the pull request early (as a draft if you like) and use the run as proof. A local `cargo xtask ci` is optional.
  `cargo xtask screenshots --out target/screenshots` captures the standard views locally. Add a view there when a new tab or dialog deserves one. If your issue carries `track:ui-design`, you produce the screenshots yourself and put the sheet path in your report; never ask the owner for evidence you can produce. CI does not capture images.
- **Where to run.** CI runs on GitHub Actions on the owner's fork. Push early and use the run as proof. A local `cargo xtask ci` is optional.

## 7. Branches, commits, merge flow

**Branch names.** `<kind>-<slug>`, or `<kind>-<area>-<slug>` when the area is not obvious from the slug. Kind prefixes: `feat`, `fix`, `docs`, `chore`, `refactor`, `research`. Examples: `feat-chamfered-pads`, `feat-artifacts-drill-map`, `docs-contributing`, `ci-pipeline` (older name, kept). One branch per issue; children of an epic may stack, base named in the report.

**Commits.** Imperative subject under 72 characters that says what changed ("Add chamfered and custom pad shapes"), a blank line, then a body in plain sentences when the subject is not enough. Reference the issue (`#281`) in the body. No AI attribution lines or trailers, no em-dashes, no other tools named. Commit as yourself.

**Merge flow.**

1. Worker pushes `feat-<slug>` and opens a pull request against `main` on wdunn001/pcbcraft. CI runs on the pull request.
2. A reviewer may comment in `### Review`.
3. Integrator verifies and merges the pull request into `main` with a merge commit, resolving conflicts in the merge commit. `main` is never rewritten. The change is done when the Actions build on `main` is green.
4. The tracker manager closes the issue.

Workers do not merge their own pull requests. Pushing to `main` directly is not allowed for anyone.

## 8. Where knowledge lives

- **Program status epic #95** on wdunn001/pcbcraft (it was #95 on the old tracker): every roadmap line with its issue, what is in flight, open owner decisions, and the enrichment and research comment links. Update it when an issue is opened, claimed, finished or dropped.
- **The issue itself**: body, `### Research`, `### Enrichment`, build reports are in the pull request body, `### Review`.
- **Repository docs**: `.cursor/rules/*.mdc` (the canonical agent rules, one file per topic; see [Cursor rules docs](https://cursor.com/docs/rules)), AGENTS.md and CLAUDE.md (thin pointers to those rules), `docs/design-system.md` and `docs/page-layouts.md` (the UI design), `docs/agents/` (role briefs for parallel workers), README.md (what works, how to run), ROADMAP.md (milestones), `docs/architecture.md` (layers and decisions), `docs/validation.md` (evidence for the initial subset), `docs/mcp.md` (MCP client setup), `docs/ci.md` (every CI job and its local command), ATTRIBUTION.md (provenance).
- **Research notes**, available on request from the maintainer: the feature parity matrix, per-issue research notes (routing, length and impedance, pours, fabrication checks, 3D view, simulation), the manufacturing artifact format survey, the shop equipment and machine profile notes, the tagging standard and the coordinator's queue. Ask on the issue you are working and the relevant note is posted or summarised there. A searchable knowledge base of captured standards and sources backs the research stage; findings from it arrive on issues as `### Research` comments.

## 9. Instructions adopted from the pipeline review

These come from a review of the pipeline against the tracker on 2026-10-28 and are in force.

- **Queue and claims.** Check that an issue has no live lease before claiming (section 3). Open owner questions go in #95: set `owner-decision`, state the choice and your recommendation in one paragraph, and add one line to "Open decisions for the owner". Do not wait in the thread.
- **One base.** The only base is `main` on wdunn001/pcbcraft. Branch from it with `git fetch origin && git checkout -b <kind>-<slug> origin/main`, push only your feature branch, never `main`. The integrator merges verified pull requests into `main` with merge commits and posts any logic-changing conflict resolution on your issue. If your branch falls behind, merge `main` into it and fix conflicts on your side; if you have no choice but to rebase, push with `git push origin +<branch>`. Never rewrite `main`. Never post to the frozen tangency-labs tracker.
- **Anchors.** Anchors in an enrichment are file paths plus function or type names, never line numbers, verified at `Base: main at <sha>`. To add scope after enrichment, post `### Enrichment (update)` and edit the Ready block; a bare scope comment sends the issue back to `stage:enrich`.
- **Research dedupe.** Before researching, read every `### Research` and `### Enrichment` comment on the issue and on the issues it links, and search the research notes and source manifest for its number. Open your `### Research` comment with what is already covered and where, then what is new.
- **Tests out of lib.rs.** Do not edit `crates/ui-egui/src/lib.rs` unless your issue is tagged `conflict:ui-egui-lib` and is the active ticket on that lane. Put your `a_newcomer_*` test in `crates/ui-egui/src/e2e/<area>.rs` (helpers in `src/e2e/support.rs`). If your feature needs a lib.rs change (a tab, a menu item, a dock page), ship the engine, io and test work first as a parallel-safe branch and file a one-line follow-up for the lib.rs part.
- **Screenshots yourself.** For `track:ui-design`, you produce the visual evidence (section 6). A description of what renders is not evidence.
- **Stop after two failures.** If the same build or test fails twice with the same output, do not try a third time: post the output, set `status:blocked`, and stop.
- **Done block.** Your pull request body ends with the Definition of Done list (section 4), every line filled. Limitations you list become follow-up issues, not prose.

## 10. Proposed improvements

These are proposals, not current rules. Follow sections 1 to 9 until an item moves up.

- **Queue view and gate script.** A script over the `gh` JSON that parses the Tags line of every issue and prints counts per stage and status, claims with no `ready` tag, expired leases, issues whose latest comment postdates the enrichment, and open owner decisions; with a flag that rewrites a Queue section in #95. Workers would claim only from its `claimable` output. Instruction when adopted: run `scripts/queue.sh claimable` before you claim and claim only an issue it lists.
- **A `status:needs-review` status.** Between `in-progress` and `pr-open`, for pull requests that wait for a reviewer verdict ending in `VERDICT: pass` or `VERDICT: fail`.
- **Lease sweeper.** A command that lists expired claims and, on request, posts "Claim expired, released" and sets `status:open`.
- **Anchor check.** `scripts/queue.sh anchors <N>` greps every anchor in an enrichment at `main` and flags missing ones; run by the enricher before posting and by the worker before claiming.
- **Text-rule linter before posting.** `scripts/post-check.sh <file>` fails on forbidden product names, em-dashes, AI attribution and a malformed Tags line; run it before every `gh issue comment`, `edit` or `create`.
- **Packaged owner test with branch option.** `cargo xtask owner-test --branch <name>`, wrapping the screenshot capture, a side-by-side sheet against `main` and the one-line command to run the branch.
- **Required check.** Mark the CI result job as a required check on `main` once the CI pipeline is merged.
- **Versioned pipeline rules and agent definitions.** Partially adopted: AGENTS.md, CLAUDE.md, `.cursor/rules/`, and `docs/agents/` in the repository. Still to do: pre-post check scripts (`scripts/post-check.sh`, `scripts/queue.sh`).
- **Budgets per agent and per stage.** Turn and cost limits on every headless worker, sized by the `size:` tag, and a cap per stage so research does not starve enrichment.
- **Lessons loop.** Append each reviewer fail and each worker limitation to a lessons file per issue. When a review fails a branch for a reason that has failed another branch, open a `kind:chore` issue to encode the check in `cargo xtask`.

Changes to this file go through the same pipeline: file an issue tagged `kind:docs` `area:docs`, claim it, and push a `docs-<slug>` branch.

## Licence

PCBCraft is licensed under the MIT License or the Apache License, Version 2.0, at your option
(LICENSE-MIT, LICENSE-APACHE). Contributions are accepted under the same terms, inbound = outbound,
which matches the Rust ecosystem convention.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
PCBCraft by you, as defined in the Apache-2.0 license, shall be dual licensed as MIT OR Apache-2.0,
without any additional terms or conditions.

Documentation and screenshots stay under CC BY-SA 4.0, and the PCBCraft name and logo stay reserved.

