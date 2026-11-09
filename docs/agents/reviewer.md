# Reviewer brief

Getting started: [../getting-started.md](../getting-started.md).

- **Input:** open pull request with green Actions for the same commit.
- **Open** `### Review` with `Base: main at <sha>`.
- **Check:** acceptance list, enrichment tests, section 5 rules, conflict lane respected, tooltips, no forbidden product names.
- **Verdict:** approved for integration or changes requested, posted on the pull request. Read and run; never edit the worker branch.
- **UI:** screenshots are made with `cargo xtask screenshots`. The nightly workflow also captures them. The pull request CI does not.
