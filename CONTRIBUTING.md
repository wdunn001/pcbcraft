# Contributing to PCBCraft

This file explains how work moves through the PCBCraft tracker, from a request to a merged change. It is written for people and for agents, including outside agents that claim issues on their own. If you only want to fix something small, read "Worker" and "Definition of Done" and skip the rest.

**The codebase of record is the fork's `main`: github.com/wdunn001/pcbcraft, branch `main`.** Every upstream pull request and every contributor branch is merged into it, so it is the actual PCBCraft as it stands. Read code there, branch from it, merge the latest fork `main` into your branch before your final push, and resolve conflicts on your side. Upstream `main` on tangency-labs/pcbcraft lags behind it; upstream receives a pull request only after the maintainer has tested the integrated change.

Issues live upstream on tangency-labs/pcbcraft. CI is GitHub Actions on the fork: pushing a branch there runs the whole gate, and the Actions run is the proof of a green build. In this file "fork/main" means that branch, and `Base: fork/main at <sha>` is the first line of every `### Research`, `### Enrichment` and `### Review` comment, so everyone names the same code.

**Definition of done for the program:** a fully featured, complete build on the fork's `main` that contains every upstream pull request and every contributor branch, with as few conflicts as possible. A change is done when it is merged into fork `main` and the full GitHub Actions build on fork `main` is green. Everything else in this file serves that.

**Start here.** Get the latest code from fork `main`. Find work on the upstream tracker: `gh issue list -R tangency-labs/pcbcraft --search "ready in:body" --state open`, then claim one with a `CLAIM` comment (section 3). Deliver by pushing a branch to a fork, waiting for a green Actions run, and posting a build report. Issues come first: nothing is built that is not an enriched, tagged issue. No upstream pull request is opened until the maintainer has tested the change.

## 1. The pipeline at a glance

Every piece of work becomes an issue before anyone touches code. The issue then moves through fixed stages. Tags on the first line of the issue body say which stage it is in.

```
 request                 raw ticket               researched                enriched
 (owner, user, agent) -> [reporter] ------------> [researcher] -----------> [enricher] ---------> ready
                         stage:research           research:done             enrich:done           `ready`
                         or stage:enrich          stage:enrich              full Tags line
                         research:n/a

 ready --> [worker claims] --> [worker builds] ---> branch pushed --------> [reviewer] ---------> [integrator]
           CLAIM <agent>       own clone off          Actions run green        ### Review            merge commit into
           status:claimed      fork/main, merges      build report comment     approved or           fork main, Actions
                               fork/main before       "PR pending owner        changes requested     green on fork main
                               the final push         review"                                        = DONE
                                                      status:in-progress ->
                                                      status:needs-owner-test

 fork main --> [owner test] --> upstream pull request --> upstream main
               owner runs the      one PR per feature,       status:pr-open,
               integrated build    targets upstream main     then closed
```

Stages in words:

1. **Raw ticket.** A reporter files a short issue: the request in the requester's words, the goal, links. Tags: `stage:research`, or `stage:enrich` plus `research:n/a` when there is no open standards or design question.
2. **Research.** A researcher answers the standards, format, machine or reference-rule questions, posts a `### Research` comment, sets `research:done` and moves the issue to `stage:enrich`.
3. **Enrichment.** An enricher verifies file and function names, lists every place to change, dependencies and order, the conflict lane, the test list, tooltips, text catalog, accessibility and logging needs, and splits the work into one-PR children. It posts `### Enrichment`, writes the full Tags line, sets `enrich:done` and replaces `stage:enrich` with `ready`.
4. **Ready.** A worker may claim it. Workers claim only `ready` issues. Workers who share a build machine with others also need `conflict:parallel-safe`.
5. **Work.** The worker claims, branches off fork `main`, builds, tests, merges the latest fork `main` back in, pushes the branch, waits for the Actions run, and posts a build report ending in "PR pending owner review".
6. **Review.** A reviewer checks the pushed branch against the acceptance criteria and posts `### Review`.
7. **Integration.** The integrator merges the branch into fork `main` with a merge commit. The merge message ends with a line `Issues: #N, #M` listing every issue the branch delivers, with no other text on that line. When the Actions build on fork `main` is green, the change is done.
8. **Owner test.** The maintainer runs fork `main`. Only then does a pull request to upstream open.

## 2. Roles, step by step

Replace `N` with the issue number and `<agent>` with your handle (a GitHub login or an agent name such as `claude-agent-docs`). All commands use the GitHub CLI, `gh`, authenticated as you.

### Reporter

Anyone who turns a request into an issue. The maintainer's coordinator does this for the owner's requests; outside contributors may file their own.

1. Search first so the request is not already tracked:
   ```sh
   gh issue list -R tangency-labs/pcbcraft --search "<keywords>" --state all
   ```
2. Create the issue with the body template below. The first line is the Tags line. Keep the How section short or write "To be completed by the enrichment stage"; the enricher fills it in.
   ```sh
   gh issue create -R tangency-labs/pcbcraft --title "<plain title>" --body-file body.md
   ```
3. Choose the stage. If the work depends on an open standards, file format, machine or reference-rule question, tag `stage:research` and `research:needed`. Otherwise tag `stage:enrich` and `research:n/a`.
4. Add the new issue to the program status epic #352 (comment on it, or edit if you have access).

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
- Branch off fork main (wdunn001/pcbcraft) and merge the latest fork main into your branch before your final push. Push the branch; do not open a PR. The maintainer opens upstream PRs after testing.
- New code and PR text may not name other EDA tools.
- Typed engine commands with bounded validation; no unwrap/expect/panic anywhere, tests included.
- The GitHub Actions run on your pushed branch must be green; it is the proof.
- Every new control gets a tooltip; panels keep real margins and padding.
- End-to-end tests in crates/ui-egui are named like `a_newcomer_*` and must read like a story for someone who has never seen the app.

### Claiming
Comment `CLAIM <agent-name>` to take this. A coder's claim lasts 6 hours and any other stage's claim lasts 2 hours; renew with `CLAIM-RENEW <agent-name>`. An expired claim may be taken over.
```

### Researcher

Takes `stage:research` issues. The deliverable is a comment, not code.

1. Find work:
   ```sh
   gh issue list -R tangency-labs/pcbcraft --search "stage:research in:body" --state open
   ```
2. Check what is already known before reading anything new: the `### Enrichment` and `### Research` comments on this issue and on every issue it links, and the research notes (ask on the issue). Open the comment with what was already covered and where, so the same defect is not researched twice.
3. Read the published specifications, standards and reference material the question needs. Read the code paths the issue names, so the findings say what the code does today and what the specification says it must do.
4. Post one `### Research` comment. Format:
   ```markdown
   ### Research

   Base: fork/main at <sha>
   Already covered: #286 enrichment items 1, 2 and 6. New here: items 3 to 5.
   One line on what was read (specification name and revision, code paths).

   1. **Finding title.** What the code does, what the standard requires, file and line anchors, the change this implies.
   2. **Next finding.** ...

   Owner decision: the question the maintainer must answer, if any, stated as a choice.
   ```
   Research comment #351 is the model: numbered findings, each tied to a section of the specification and a place in the code, ending with one owner decision.
5. Update the Tags line: add `research:done`, replace `stage:research` with `stage:enrich`. If `enrich:done` is already present, set `ready` instead of `stage:enrich`. If the maintainer must decide something, add `owner-decision`, say what the choice is, and add one line to the "Open decisions for the owner" list in #352 (section 3, "Owner decisions").
6. Captured sources (specification pages, datasheets, application notes) go into the project's knowledge base. Ask on the issue if you want a source captured; the maintainer's research agents maintain the manifest.

### Enricher

Takes `stage:enrich` issues. The enricher owns the Tags line and makes the issue buildable without further questions.

1. Find work:
   ```sh
   gh issue list -R tangency-labs/pcbcraft --search "stage:enrich in:body" --state open
   ```
2. Read the code on fork `main` and record the commit: the first line of the enrichment is `Base: fork/main at <sha>`. If the work must build on a branch not yet merged into fork `main`, say so on the next line (`Stacks on: feat-<slug> at <sha>`) and prefer to have that branch integrated first. Anchors are file names plus function, type or constant names (`crates/doc/src/lib.rs: Pad::outline_points, Pad::bounding_radius`), never bare line numbers; line numbers go stale within a day on this repository, names can be grepped. Verify every anchor exists at the recorded commit before posting.
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
   The enrichment on #281 is the model.
4. Rewrite the whole Tags line in the same pass: track, kind, area, milestone, crate, conflict, size, machine, status, then `ready` (when research is done or n/a) or `stage:research` (when research is still needed), then `research:*` and `enrich:done`. How to edit the line safely is in section 3.
5. If an owner decision is needed before work can start, add `owner-decision` and `status:blocked`, say what the choice is, and add it to the decision list in #352.
6. Scope added later goes through the enricher again. Anyone who adds a requirement to an issue after the `### Enrichment` comment posts it as `### Enrichment (update)` and refreshes the test list and the Tags line in the same comment. A loose "scope note" comment with no update does not count; a worker who finds one asks for the update before claiming.

### Worker

Takes `ready` issues. A worker is a human or an agent that writes the code.

**Find work**

```sh
# everything a worker may claim
gh issue list -R tangency-labs/pcbcraft --state open --search "ready in:body status:open in:body"
# only what can run alongside other workers (required when you share a build machine)
gh issue list -R tangency-labs/pcbcraft --state open --search "ready in:body status:open in:body conflict:parallel-safe in:body"
```

Read the issue body, every `### Research` and `### Enrichment` comment, and the linked issues. Check the program status epic #352 for who is working nearby.

**Claim**

```sh
gh issue comment N -R tangency-labs/pcbcraft --body "CLAIM <agent>"
```

Optionally add one or two lines of plan under the claim, as on #281. Then set `status:claimed` on the Tags line (section 3). Do not claim more than one issue at a time unless they are stacked children of one epic. Your lease is 6 hours; renew it with `CLAIM-RENEW <agent>`. An expired claim may be taken over.

**Branch**

One clone or worktree per agent. Never work in a checkout another agent is using. Clone the codebase of record and branch off its `main`:

```sh
git clone https://github.com/wdunn001/pcbcraft.git pcbcraft-<slug>
cd pcbcraft-<slug>
git remote add upstream https://github.com/tangency-labs/pcbcraft.git
git checkout -b feat-<slug> origin/main
```

Branch from the base the enrichment recorded (`Base: fork/main at <sha>`, or the `Stacks on:` branch when there is one). Branch names follow section 7. If you do not have push access to the maintainer's fork, add it as a remote named `fork`, work on your own fork and push there; the report names the fork and branch.

Before your final push, bring the latest fork `main` into your branch and resolve conflicts on your side, so the integrator receives a branch that merges cleanly:

```sh
git fetch origin main
git merge origin/main
# resolve, build, test, commit the merge
```

A branch that does not merge cleanly into fork `main` at review time goes back to the worker.

**Build and test**

CI is GitHub Actions on the fork, and it runs on pull requests and on `main`, not on plain branch pushes. Open a draft pull request to get the cheap checks (format, policy, clippy, docs); mark it ready for review to run the full gate. The green run is the proof of a build; link it in your report. Running the gate yourself before pushing is optional and saves round trips:

```sh
cargo xtask ci
```

It runs formatting, dependency layer checks, clippy with warnings denied, the workspace tests (including the `a_newcomer_*` end-to-end tests) and the WebAssembly check. Once the CI pipeline branch (#340) is merged, the Actions gate also runs policy, docs, MSRV and license checks, the full e2e suite and the screenshot sheet; `docs/ci.md` lists every job and its local command. If you build on a shared machine, cap parallelism (`CARGO_BUILD_JOBS=4`) and keep your own target directory; do not run full builds on a laptop when a build machine or Actions can do it.

For UI changes, look at the rendered result (section 6).

**Push**

```sh
git push -u origin feat-<slug>
```

Push only your feature branch. Do not push to `main` on any repository and do not open a pull request. The integrator merges into the fork's `main`; the maintainer opens upstream pull requests after testing. Set `status:in-progress` on the first push.

**Report**

Post a build report on the issue when the branch is complete. Format (the report on #281 is the model, with the Done checklist added):

```markdown
Implemented on branch feat-<slug> (fork <owner>/pcbcraft), commit <sha>, based on <branch> at <sha>, 7 commits above the base.

Built:
- What landed, per crate, in plain sentences.

Bugs fixed in passing:
- ...

Acceptance:
- criterion 1: test `clearance_to_a_chamfered_corner_measures_the_cut_edge`
- criterion 2: not done, reason, follow-up #NNN

Tests: first lines of the `cargo xtask ci` output, not the word green. CI run: <link>.

Files touched: list, and "crates/ui-egui/src/lib.rs untouched" or the lane ticket number.

Screenshots: <link or attachment> (UI changes only).

Limitations: what the change does not do, stated plainly, with follow-up issue numbers.

Definition of Done:
- [x] ... (the list from section 4, ticked)

PR pending owner review.
```

Then set `status:needs-owner-test` on the Tags line. Nothing else is required of the worker; the reviewer and integrator pick it up from the comment.

### Reviewer

Checks each "PR pending owner review" branch against its issue before integration.

1. Find work:
   ```sh
   gh issue list -R tangency-labs/pcbcraft --state open --search "status:needs-owner-test in:body"
   ```
2. Fetch the branch into your own read-only clone and read the diff against its recorded base. Start the `### Review` with `Base: fork/main at <sha>`. Confirm the branch merges cleanly into current fork `main`; a branch that does not goes back to the worker. Check that the Actions run in the report is green and for the branch head; if it is missing or red, fail the branch.
3. Walk the Acceptance list and the test list from the enrichment, running each named test. Check the rules in section 5 (no unwrap or panic, typed commands, validation bounds, no other tools named in the diff or in the comments, tooltips, margins, attribution). Confirm `crates/ui-egui/src/lib.rs` is untouched when the issue is `conflict:parallel-safe`. For UI work, open the screenshots. The reviewer reads and runs; it never edits the branch.
4. Post one `### Review` comment:
   ```markdown
   ### Review

   Base: fork/main at <sha>

   Branch feat-<slug> at <sha>.

   Acceptance:
   - [x] criterion, evidence (test name, file, screenshot)
   - [ ] criterion, what is missing

   Rules: pass, or the rule and the place it is broken.

   Verdict: approved for integration
   ```
   or `Verdict: changes requested` followed by the list. The worker fixes and reposts a build report; the reviewer reviews again.
5. The reviewer does not merge and does not change the status tag; the verdict is the signal the integrator waits for.

### Integrator

One agent owns the integration branch, the fork's `main`. It merges every reviewed branch so the maintainer can test everything together.

1. Watch for `### Review` comments with "approved for integration" and for new or force-pushed branches on the fork.
2. Merge each branch into the fork's `main` with a merge commit, never a squash or a rebase of `main`:
   ```sh
   git checkout main && git pull --ff-only origin main
   git merge --no-ff feat-<slug>
```
End the merge commit message with a line `Issues: #N, #M` listing every issue the branch delivers, with no other text on that line.
3. Resolve conflicts in the merge commit, keeping both sides' intent; if the resolution is not obvious, ask on both issues before pushing.
4. Push the merge, then watch the GitHub Actions run on fork `main`. The change is done when that full build is green; if it is red, fix forward on a branch or report on the issue, and never rewrite `main`. Where a local run is cheap, run `cargo xtask ci` on the merged tree before the push:
   ```sh
   git push origin main
   ```
5. Never rewrite the fork's `main`, never push to upstream, never open upstream pull requests.
6. Keep a ledger in `integration-ledger.md`: date, branch, branch head, issue, fork main before and after, Actions run, conflict files and how each was resolved. Comment on each issue with the merge commit, and post any resolution that changes logic so the worker and reviewer see it.

### Owner test

The maintainer tests with one packaged command that builds the branch or fork `main` and produces the screenshot sheet. It wraps `cargo xtask screenshots` from the CI pipeline (#340); the command is tracked as its own issue. Until it exists, run `cargo xtask screenshots --out target/screenshots` yourself. Only after the maintainer has tested:

- The maintainer opens one pull request per feature from the feature branch to upstream `main`, with a body that starts with the Tags line, and sets `status:pr-open` on the issue.
- Upstream pull requests merge only with green CI.
- When upstream merges, the issue closes, and the status epic #352 is updated.

If the maintainer finds a problem, they comment on the issue; the status goes back to `status:in-progress` and the worker continues on the same branch.

## 3. Tags and claiming

The maintainer has read permission upstream, so GitHub labels cannot be used. Tags live on the first line of every issue and pull request body, as one `**Tags:**` line of backticked tags, in this order: track, kind, area, milestone, crate, conflict, size, machine, status, stage (or `ready`), research, enrich, owner-decision.

```
**Tags:** `track:ai-handoff` `kind:feature` `area:layout` `milestone:m2` `crate:engine` `crate:ui-egui` `conflict:ui-egui-lib` `size:m` `status:open` `ready` `research:n/a` `enrich:done`
```

Vocabulary, one line each. "One" means exactly one value.

| Namespace | Values |
| --- | --- |
| track (one or more) | `track:ai-handoff` an agent can take it without asking; `track:ui-design` it changes what the user sees |
| kind (one) | `kind:feature` `kind:epic` `kind:bug` `kind:research` `kind:chore` `kind:refactor` `kind:docs` |
| area (one or two) | `area:schematic` `area:layout` `area:library` `area:rules` `area:io` `area:artifacts` `area:3d` `area:sim` `area:ui-shell` `area:mcp` `area:ci` `area:docs` `area:packaging` `area:kb` |
| milestone (zero to two) | `milestone:m1` to `milestone:m4` from ROADMAP.md, `milestone:m5` Inner copper layers (epic #316), `milestone:m6` Manufacturing artifacts (epic #322) |
| crate (one or more) | `crate:engine` `crate:doc` `crate:rules` `crate:io` `crate:geom` `crate:library` `crate:mcp` `crate:ui-egui` `crate:xtask` `crate:sim` `crate:apps` |
| conflict (one) | `conflict:ui-egui-lib` edits crates/ui-egui/src/lib.rs; `conflict:parallel-safe` stays out of that file |
| size (one) | `size:s` one PR, one crate, under a day; `size:m` one PR, several crates; `size:l` several PRs or an epic |
| machine (zero or more) | `machine:neoden-yy1` `machine:eufymake-e1` `machine:snapmaker-cnc` `machine:commarker-b6` |
| status (one) | `status:open` `status:claimed` `status:in-progress` `status:pr-open` `status:needs-owner-test` `status:blocked` |
| stage (one) | `stage:research` `stage:enrich` `ready` (bare; an issue never carries both `ready` and a `stage:` tag) |
| research | `research:needed` `research:done` `research:n/a` |
| enrich | `enrich:done` (an `### Enrichment` comment exists) |
| flag | `owner-decision` the maintainer must choose something; say what in the body or a comment |

Deriving the stage: no enrichment and research needed gives `stage:research`; no enrichment and research done or n/a gives `stage:enrich`; enrichment done and research still needed gives `stage:research`; enrichment done and research done or n/a gives `ready`.

**Who sets which tag**

- Reporter: the first line, with at least `track:`, `kind:`, `area:`, `status:open` and the stage.
- Researcher: `research:done`, and the move from `stage:research` to `stage:enrich` (or to `ready` if enrichment already happened).
- Enricher: the whole line, including `enrich:done` and `ready`.
- Worker: `status:claimed` on `CLAIM`, `status:in-progress` on first push, `status:needs-owner-test` with the build report.
- Maintainer: `status:pr-open` when the upstream pull request opens.
- Anyone: correct a wrong `area:`, `crate:` or `conflict:` tag; set `owner-decision` on hitting a question; clear it when recording the answer.

**The conflict lane.** `crates/ui-egui/src/lib.rs` holds the tabs, docks, menus and inspector sections, and nearly every UI change lands there. It is edited serially: one `conflict:ui-egui-lib` ticket at a time, by the maintainer's UI shell lane, and nobody else touches that file in parallel. If your work needs a line in lib.rs, say so in the issue and wait for the lane rather than editing it; `conflict:parallel-safe` work proceeds alongside anything else.

**Editing the Tags line.** Several agents edit issues at once. Fetch, change the first line only, write back in the same step, and never touch other body text or comments:

```sh
gh issue view N -R tangency-labs/pcbcraft --json body -q .body > body.md
# edit line 1 only
gh issue edit N -R tangency-labs/pcbcraft --body-file body.md
```

**Claiming and leases.** Claim a `ready` issue with `status:open` by posting one comment: `CLAIM <agent>@<host> until <UTC time> clone <path> branch <name>`. A lease lasts 6 hours for coders and 2 hours for research, enrichment and review. Renew it with `CLAIM-RENEW <agent>` and a one-line progress note; every push also renews it, and you post a line at least every 2 hours while working. An expired claim may be taken over: comment `CLAIM <agent> (takeover, previous claim expired)`, set `status:claimed`, and the earlier claimer stops. Before claiming, check the issue has no live lease. Abandoning a claim: comment "Releasing claim" and set `status:open`. Claims that do not use the format fall back to 5 days with no pushed branch.

**Comment headings in use.** `CLAIM <agent>`, `### Research`, `### Standards` (research on rule and output issues), `### Enrichment`, `### Enrichment (update)`, `### Review`, and a build report whose last line is `PR pending owner review`. Use these exact headings so searches and the status epic find them.

**Owner decisions.** A question only the maintainer can answer (strategy, accounts, signing, a convention such as the internal Y axis direction) does not wait in a comment thread. Whoever hits it sets `owner-decision` on the issue and adds one line to the "Open decisions for the owner" list in #352: `- [ ] #351: Y-up internally with a display flip? (asked by research, 2026-10-28)`. The maintainer answers on the issue or in #352; whoever records the answer clears the tag and ticks the line.

## 4. Definition of Ready and Definition of Done

Both lists are pasted into the issue and ticked, not only read: the enricher pastes Ready at the top of `### Enrichment`, the worker pastes Done at the end of the build report. "n/a" with one word of reason is an acceptable tick; a blank box blocks the stage. Keep the lists to what the next person cannot discover alone.

**Definition of Ready** (the enricher ticks this before setting `ready`):

- [ ] Goal in one sentence, in the requester's words, with the owner direction quoted if one exists.
- [ ] Acceptance criteria as numbered observable statements: what the user sees or what the test asserts.
- [ ] Research is done or marked n/a; `### Research` findings are reflected in How.
- [ ] Test list: exact test names and the file each goes in, including the `a_newcomer_*` story test for UI work.
- [ ] Anchors: file plus function, type or constant names, verified by reading the code at `Base: fork/main at <sha>`. No bare line numbers.
- [ ] `Base: fork/main at <sha>` recorded and conflict lane set (`conflict:ui-egui-lib` or `conflict:parallel-safe`).
- [ ] Dependencies: issues that must land first, and what breaks if they do not; blocked issues carry `status:blocked`.
- [ ] Out of scope: what this issue does not do, with the follow-up issue number.
- [ ] Tooltips, text catalog, accessibility and logging needs listed for UI work.
- [ ] Owner decisions: none, or listed with `owner-decision` set and a line in #352.
- [ ] Text rule: title and body name no other EDA tool.
- [ ] Size is `s` or `m`; `size:l` work is split into children first. The full Tags line is set.

**Definition of Done** (the worker ticks this in the build report; the reviewer and integrator check it):

- [ ] Branch, commit, merge-base and the number of commits above the base are in the report, and fork main was merged into the branch before the final push.
- [ ] Each acceptance criterion is listed with the test name or screenshot that proves it, or "not done" with the reason and a follow-up number.
- [ ] Every test from the enrichment exists and passes; new behaviour has unit tests; file writers have golden fixtures; UI work has a readable `a_newcomer_*` test.
- [ ] The link to the green GitHub Actions run on the fork for the same commit is in the report, with the first lines of the `cargo xtask ci` output if you ran it locally.
- [ ] No `unwrap`, `expect`, `panic`, `todo`, `unimplemented` or `unreachable` in production code; invalid input leaves state untouched.
- [ ] Every edit goes through a typed engine command with bounded validation.
- [ ] Every new control has a tooltip; panels keep real margins and padding; labels are short and plain.
- [ ] For `track:ui-design`: screenshots attached or linked.
- [ ] Files touched are listed; `crates/ui-egui/src/lib.rs` is untouched for `conflict:parallel-safe` work, or the lane ticket is named.
- [ ] No other EDA tool is named in code, comments, UI strings, docs, commit messages or the report.
- [ ] Adapted code is recorded in ATTRIBUTION.md with source, revision and license.
- [ ] Limitations and follow-ups are stated with issue numbers, and shown in the UI where the user would meet them.
- [ ] Branch pushed to the fork; report ends in "PR pending owner review"; `status:needs-owner-test` set.

After the worker: `### Review` verdict "approved for integration"; merged into the fork's `main` with a merge commit and the Actions build on fork `main` green, which is the program's definition of done; the maintainer has run it with the owner-test command (section 6); an upstream pull request is open; #352 is updated.

## 5. Code rules

These come from AGENTS.md and apply to every change.

- **One window.** Document and workspace tabs plus docked panes. Never spawn separate schematic, board, library, checks or 3D windows. Project and selection are shared.
- **Typed engine commands.** Everything that edits the project is a command in `crates/engine` with a JSON schema. UI, CLI and MCP call the same engine. Programmatic commands never open dialogs.
- **Validation.** Finite, bounded coordinates and dimensions; bounded counts; stable IDs; reference integrity. An invalid command or batch leaves the document, revision and history untouched.
- **No panics.** No `unsafe`; no production `unwrap`, `expect`, `panic`, `todo`, `unimplemented` or `unreachable`. Tests may use `?` with `Result<(), String>`; the e2e tests do. The CI pipeline branch lints these at deny and forbids source `allow` of them.
- **Crate layering.** geom (L0) under doc and library (L1) under rules and io (L2) under engine (L3) under mcp and ui-egui (L4) under the apps. Lower crates never depend on egui, eframe, wgpu or MCP. `cargo xtask ci` checks the layers.
- **Adapted GPL code.** Source from other GPL-compatible projects may be inspected and adapted. Record the exact source, revision and license in ATTRIBUTION.md. Translating to Rust does not remove attribution or copyleft. Check each file's license. Do not copy trademarks or proprietary assets from shared Craft dependencies; pin craft-libs revisions.
- **Honest limits.** Do not present initial checks as complete ERC/DRC, or a basic route as a shove router. Track parity by workflows and fixtures, not by percentages.
- **Names.** Never name other EDA tools in code, docs, UI strings, issues or pull requests. Describe formats and behaviours instead ("the board interchange format", "the plot layer format").
- **UI.** Every control gets a tooltip. Panels keep real margins and padding. Labels are short plain words. UI changes are inspected rendered, not only in tests.
- **Publishing.** Do not publish releases or push to third-party repositories without authorization.

## 6. Testing

- **Unit tests** live beside the code in each crate (`#[cfg(test)]` modules) and cover validation bounds, rejections and the happy path. Name them as sentences: `padstack_rejects_chamfer_ratio_out_of_range`.
- **Golden fixtures** cover every file writer and reader. Put fixture files under `crates/<crate>/tests/fixtures/` and compare whole outputs, not substrings. For output that an independent parser exists for, read the output back with that parser as a dev-dependency. Fixtures from external libraries keep their license file beside them.
- **End-to-end tests** are named `a_newcomer_<does something>`. They drive the real `App` headlessly with the `frame`, `tap`, `paints_text` and `scrolled_to` helpers, and every assertion message is a plain sentence that someone who has never seen the app could follow ("the matching part stays in the list"). One test per user-visible feature. They live in `crates/ui-egui/src/e2e/<area>.rs` (helpers in `src/e2e/support.rs`), one file per area (library, nets, rules, builds, board, checks, artifacts, layout, layers, shell). Put new tests in the matching area file, or add a new area file plus one `mod <area>;` line in `src/e2e/mod.rs`, and never add one to lib.rs. CI runs the full e2e suite as a required check, uploads a screenshot sheet, and fails if any control lacks a tooltip or an e2e story.
- **Engine round trips** for any new document field: save and load a project carrying it and compare.
- **Screenshots for UI changes.** Render the change and attach the image to the build report. On `main` today:
  ```sh
  xvfb-run -a -s '-screen 0 1600x1100x24' \
    env -u WAYLAND_DISPLAY WGPU_BACKEND=vulkan \
    cargo run -p pcbcraft -- --sample --capture pcbcraft.png
  ```
  `cargo xtask screenshots --out target/screenshots` (from the CI pipeline, #340) captures the standard views, and CI uploads a side-by-side sheet of base and head. Add a view there when a new tab or dialog deserves one. If your issue carries `track:ui-design`, you produce the screenshots yourself and put the sheet path in your report; never ask the owner for evidence you can produce.
- **Where to run.** CI runs on GitHub Actions on the owner's fork. Push early and use the run as proof. A local `cargo xtask ci` is optional.

## 7. Branches, commits, merge flow

**Branch names.** `<kind>-<slug>`, or `<kind>-<area>-<slug>` when the area is not obvious from the slug. Kind prefixes: `feat`, `fix`, `docs`, `chore`, `refactor`, `research`. Examples: `feat-chamfered-pads`, `feat-artifacts-drill-map`, `docs-contributing`, `ci-pipeline` (older name, kept). One branch per issue; children of an epic may stack, base named in the report.

**Commits.** Imperative subject under 72 characters that says what changed ("Add chamfered and custom pad shapes"), a blank line, then a body in plain sentences when the subject is not enough. Reference the issue (`#281`) in the body. No AI attribution lines or trailers, no em-dashes, no other tools named. Commit as yourself.

**Merge flow.**

1. Worker pushes `feat-<slug>` to the fork. CI runs on the push.
2. Reviewer approves in `### Review`.
3. Integrator merges `feat-<slug>` into the fork's `main` with `--no-ff`, resolving conflicts in the merge commit. The fork's `main` is never rewritten. The change is done when the Actions build on fork `main` is green.
4. Maintainer tests the fork's `main` with the owner-test command.
5. Maintainer opens one pull request per feature from `feat-<slug>` to upstream `main`. The pull request body starts with the Tags line. It merges with green CI.

Pull requests are never opened by workers, reviewers or the integrator. Pushing to any `main` is reserved to the integrator (fork) and upstream merges (upstream).

## 8. Where knowledge lives

- **Program status epic #352** on tangency-labs/pcbcraft: every roadmap line with its issue, what is in flight, open owner decisions, and the enrichment and research comment links. Update it when an issue is opened, claimed, finished or dropped.
- **The issue itself**: body, `### Research`, `### Enrichment`, build reports, `### Review`.
- **Repository docs**: AGENTS.md (rules for all agents), CLAUDE.md (Claude Code workflow), `.cursor/rules/*.mdc` (Cursor Agent rules; see [Cursor rules docs](https://cursor.com/docs/rules)), `docs/agents/` (role briefs for parallel workers), README.md (what works, how to run), ROADMAP.md (milestones), `docs/architecture.md` (layers and decisions), `docs/validation.md` (evidence for the initial subset), `docs/mcp.md` (MCP client setup), `docs/ci.md` (every CI job and its local command), ATTRIBUTION.md (provenance).
- **Research notes**, available on request from the maintainer: the feature parity matrix, per-issue research notes (routing, length and impedance, pours, fabrication checks, 3D view, simulation), the manufacturing artifact format survey, the shop equipment and machine profile notes, the tagging standard and the coordinator's queue. Ask on the issue you are working and the relevant note is posted or summarised there. A searchable knowledge base of captured standards and sources backs the research stage; findings from it arrive on issues as `### Research` comments.

## 9. Instructions adopted from the pipeline review

These come from a review of the pipeline against the tracker on 2026-10-28 and are in force.

- **Queue and claims.** Check that an issue has no live lease before claiming (section 3). Open owner questions go in #352: set `owner-decision`, state the choice and your recommendation in one paragraph, and add one line to "Open decisions for the owner". Do not wait in the thread.
- **One base.** The only base is fork main. Branch from it with `git fetch origin && git checkout -b <kind>-<slug> origin/main` (in a clone of the fork), push only your feature branch, never main. The integrator merges reviewed branches into fork main with merge commits and posts any logic-changing conflict resolution on your issue. If your branch falls behind, merge fork main into it and fix conflicts on your side; if you have no choice but to rebase, push with `git push origin +<branch>`. Never rewrite fork main and never touch upstream.
- **Anchors.** Anchors in an enrichment are file paths plus function or type names, never line numbers, verified at `Base: fork/main at <sha>`. To add scope after enrichment, post `### Enrichment (update)` and edit the Ready block; a bare scope comment sends the issue back to `stage:enrich`.
- **Research dedupe.** Before researching, read every `### Research` and `### Enrichment` comment on the issue and on the issues it links, and search the research notes and source manifest for its number. Open your `### Research` comment with what is already covered and where, then what is new.
- **Tests out of lib.rs.** Do not edit `crates/ui-egui/src/lib.rs` unless your issue is tagged `conflict:ui-egui-lib` and is the active ticket on that lane. Put your `a_newcomer_*` test in `crates/ui-egui/src/e2e/<area>.rs` (helpers in `src/e2e/support.rs`). If your feature needs a lib.rs change (a tab, a menu item, a dock page), ship the engine, io and test work first as a parallel-safe branch and file a one-line follow-up for the lib.rs part.
- **Screenshots yourself.** For `track:ui-design`, you produce the visual evidence (section 6). A description of what renders is not evidence.
- **Stop after two failures.** If the same build or test fails twice with the same output, do not try a third time: post the output, set `status:blocked`, and stop.
- **Done block.** Your final comment ends with the Definition of Done list (section 4), every line filled. Limitations you list become follow-up issues, not prose.

## 10. Proposed improvements

These are proposals, not current rules. Follow sections 1 to 9 until an item moves up.

- **Queue view and gate script.** A script over the `gh` JSON that parses the Tags line of every issue and prints counts per stage and status, claims with no `ready` tag, expired leases, issues whose latest comment postdates the enrichment, and open owner decisions; with a flag that rewrites a Queue section in #352. Workers would claim only from its `claimable` output. Instruction when adopted: run `scripts/queue.sh claimable` before you claim and claim only an issue it lists.
- **A `status:needs-review` status.** Between `in-progress` and `needs-owner-test`, set by the worker with the build report; the reviewer moves it to `needs-owner-test` on pass or back to `in-progress` on fail, and the worker never sets `needs-owner-test` on its own work. A reviewer verdict would end in `VERDICT: pass` or `VERDICT: fail`.
- **Lease sweeper.** A command that lists expired claims and, on request, posts "Claim expired, released" and sets `status:open`.
- **Anchor check.** `scripts/queue.sh anchors <N>` greps every anchor in an enrichment at fork main and flags missing ones; run by the enricher before posting and by the worker before claiming.
- **Text-rule linter before posting.** `scripts/post-check.sh <file>` fails on forbidden product names, em-dashes, AI attribution and a malformed Tags line; run it before every `gh issue comment`, `edit` or `create`.
- **Packaged owner test with branch option.** `cargo xtask owner-test --branch <name>`, wrapping the screenshot capture, a side-by-side sheet against fork main and the one-line command to run the branch.
- **Real labels.** Once the maintainer has triage permission upstream, convert the Tags vocabulary into GitHub labels and keep the body line as a mirror for search.
- **Required check.** Mark the CI result job as a required check on fork main and upstream main once the CI pipeline is merged.
- **Integration ledger in the repository.** Publish the integrator's ledger as a file in the fork.
- **Versioned pipeline rules and agent definitions.** Partially adopted: AGENTS.md, CLAUDE.md, `.cursor/rules/`, and `docs/agents/` in the fork. Still to do: pre-post check scripts (`scripts/post-check.sh`, `scripts/queue.sh`).
- **Budgets per agent and per stage.** Turn and cost limits on every headless worker, sized by the `size:` tag, and a cap per stage so research does not starve enrichment.
- **Lessons loop.** Append each reviewer fail and each worker limitation to a lessons file per issue. When a review fails a branch for a reason that has failed another branch, open a `kind:chore` issue to encode the check in `cargo xtask`.

Changes to this file go through the same pipeline: file an issue tagged `kind:docs` `area:docs`, claim it, and push a `docs-<slug>` branch.
