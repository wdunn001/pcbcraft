# Testing standard

How PCBCraft tests are written and reviewed. The mechanical parts are enforced by
`cargo xtask test-standards` (job "Test standards", also on draft pull requests);
the rest is for authors and reviewers. Rules for documentation live in
docs/doc-standard.md; coverage numbers in docs/ci.md. This page does not repeat them.

## Which kind of test, and where

| Kind | Use it for | Lives in |
| --- | --- | --- |
| Unit test | Pure logic with a clear input and output | `#[cfg(test)] mod tests` next to the code |
| Property test | An invariant over many inputs (round trips, "never overlaps", "stays inside the board") | Next to the code, with a fixed seed |
| Golden file | Generated output: Gerber, drill, placement, BOM, project files, text mock-ups | `tests/fixtures/<area>/` of the crate, compared by `pcbcraft_testkit::check_golden` |
| Parser round trip | Read a file, write it, read it again, compare | The crate that owns the format |
| Contract test | A rule every implementation of a trait obeys | With the trait, run against every implementation (crates/library/src/sources.rs is the model) |
| e2e story | One user goal, through the real UI | `crates/ui-egui/src/e2e/<area>.rs`, named `a_newcomer_<does something>` |

Choice for property tests: keep small seeded in-house generators (see the `Lcg`
in crates/geom/src/lib.rs). No property-test crate is added; revisit when a test needs
shrinking. A property test names its seed in the test, so a failure reproduces.

## e2e stories

- One story per user goal, named `a_newcomer_<does something>`, written so a novice can read it.
- Drive the real UI by what is visible: labels, tooltips, painted text. Never set internal state to reach a screen.
- Every visible control has a tooltip and a story; `cargo xtask e2e` fails on the ones that do not (docs/ci.md, e2e-exemptions.txt).
- Every acceptance line of an issue maps to a story, and the issue names it.
- Stories do not depend on timing. Wait for a condition ("the label is painted", "the dialog is closed") with a bounded number of frames, never a fixed count that merely seems enough, and never a sleep.
- Screenshots: `cargo xtask screenshots` captures the views listed in `VIEWS` in xtask/src/main.rs; add a row for a new tab or dialog. A story that needs a picture uses the capture helpers in crates/ui-egui/src/e2e/support.rs.

## Unit tests

- Every test asserts behaviour: `assert!`, `assert_eq!`, an `expect` or `unwrap` of a `Result`, `?` on a `Result` that can fail, or `#[should_panic]`. The check fails a `#[test]` that has none. An assertion hidden in a helper still needs one of these in the test, or an `?` on the helper's `Result`.
- No wall-clock time (`Instant::now`, `SystemTime::now`), no sleeps, no network and no reads or writes in the person's real folders (`dirs::`, `config_dir()`, `HOME`, `APPDATA`). Pass in a clock, answer HTTP from recordings (crates/ui-egui/src/e2e/support.rs `RecordedLibrary`), and give the code a temp directory.
- `#[ignore = "<reason>, #<issue>"]` always names a reason and an issue number.
- Random data uses a fixed seed. `thread_rng`, `from_entropy` and `rand::random` are not allowed in tests.
- Fixtures live under `tests/fixtures/<area>/`. A third-party file brings its licence: a LICENSE beside it or a line in ATTRIBUTION.md. A recorded response says where and when it was recorded.
- Speed is a work count (iterations, calls, bytes, nodes visited), not seconds. Wall-time limits are allowed only in release-only gated checks.

The checks over existing code are ratcheted by `ci/test-standards-baseline.txt`:
a file may keep the breaches it has and may not gain one. Fix some, then run
`cargo xtask test-standards --bless` to record the lower count.

## Golden files: one way to rewrite them

Set `PCBCRAFT_BLESS=1` and run the test. `pcbcraft_testkit::blessing()` (crates/testkit/src/lib.rs)
is the only reader of the variable; it is off on CI. `check_golden` writes the
file when blessing, then compares. Review reads the fixture diff. The check
fails any other variable (`UPDATE_GOLDEN`, `BLESS_GOLDEN` and the like) read
in `crates/` or `apps/`. `cargo xtask bless-parity` sets the variable for the parity goldens.

## Flaky tests

A flake is a bug with an issue. Quarantine with `#[ignore = "flaky: #N"]` for at
most 14 days, then fix it or delete the test. Do not retry-loop around it and
do not raise a timeout to hide it. Known flakes, open on 2026-10-30:

- #517: a folder added from anywhere fails under parallel runs (shared state between tests).
- #637: the language byte-identity check trips on the Gerber job CreationDate (a clock leaked into output).

Both causes are the ones this standard forbids: shared state and the clock.

## Review checklist

1. Does each new test fail if the behaviour it names breaks? (Break the code and look.)
2. Is the assertion about an observable result, not about the code having run?
3. Is the test at the lowest level that can see the behaviour (unit before e2e)?
4. Does a new user goal have an `a_newcomer_*` story, and does each acceptance line name one?
5. Does it avoid clocks, sleeps, network, real user folders and unseeded randomness?
6. Is a changed golden file explained in the pull request, with the diff read, not just blessed?
7. Is a new fixture under `tests/fixtures/<area>/` with its licence or source noted?
8. Is any `#[ignore]` carrying a reason and an issue number, and any quarantine younger than 14 days?
9. Does a trait change have a contract test that runs against every implementation?
10. Did the coverage table and `cargo xtask test-standards` stay green without a baseline raise?

## Three layers applied to tests

Same split as docs/doc-standard.md. The contract layer holds the rule every
implementation obeys, so its tests live with the trait and run against each
implementation. The implementation layer tests only what is specific to that
implementation and does not repeat the contract. Global rules (this page, AGENTS.md)
are pointed to, never copied into a test file.
