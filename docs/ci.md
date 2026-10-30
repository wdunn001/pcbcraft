# Continuous integration and releases

## Getting started

On [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft), push your branch
to the fork (never to `main`) and open a draft pull request against `main`. When
the work is done, mark the pull request ready for review and wait for the green
**CI result** check. Link that Actions run in the pull request. You do not need
a green local `cargo xtask ci` on a laptop.

Clone and first-run steps: [getting-started.md](getting-started.md).

## Details

Every CI job runs one `cargo xtask` subcommand, so any failure reproduces on a
workstation with the same command. `cargo xtask ci` runs policy, fmt, clippy,
tests, wasm, docs and `docs check` in one go. On the fork it is optional for agents; the green
GitHub Actions run on the pushed branch is the proof (see CONTRIBUTING.md). `cargo xtask docs check` must pass before every push.

### Fork and pull request gate (`.github/workflows/ci.yml`)

The gate runs on pull requests, on pushes to `main`, in the merge queue and on
manual dispatch. A push to a feature branch starts nothing by itself; open a
pull request (a draft is fine) to get results. This avoids the duplicate run
that a branch push plus a pull request used to start.

- **Draft pull request:** only the cheap jobs run: Changes, Format and policy, Clippy and docs, Documentation standard. **CI result** reports that set.
- **Ready pull request:** marking it ready for review (the `ready_for_review` event) starts the full gate, including Tests, Coverage, E2E, UI strings, Accessibility, Localization, WebAssembly, Licenses and advisories, UI screenshots and the Windows build. **CI result** goes green only when all of them succeed.
- **Main and merge queue:** everything, and macOS and the minimum Rust version too.

Documentation-only commits skip the heavy build jobs on any of these but still
run policy, docs standard and the result job.

| Job | Command | What it enforces |
| --- | --- | --- |
| Format and policy | `cargo xtask policy`, `cargo xtask fmt` | Crate dependency layers, workspace lints keep `unwrap_used`, `expect_used`, `panic`, `unimplemented`, `todo` and `unreachable` at deny (AGENTS.md), no source `allow` of them, rustfmt |
| Clippy and docs | `cargo xtask clippy`, `cargo xtask docs` | Clippy with `-D warnings` on all targets, rustdoc with warnings denied |
| Documentation standard | `cargo xtask docs check` | Contract and dependency paths exist, trait implementation lists match the code, AGENTS.md is 60 lines or fewer, missing-docs warnings do not rise above xtask/doc-ratchet.toml |
| Tests | `cargo xtask test` | Workspace tests with cargo-nextest (profile `ci`, JUnit report uploaded) plus doctests. Includes the headless `a_newcomer_*` end-to-end tests in crates/ui-egui |
| Text catalog | `cargo xtask strings check` (and `strings extract`) | `crates/text/strings/en.txt` is well formed with no duplicate keys, translation files have no unknown keys and keep the English `{placeholders}`, every `t!("key")` in crates/ui-egui exists, and no string literal sits in a text call inside a function listed in `crates/text/strings/converted-fns.txt` |
| Accessibility | `cargo xtask a11y` | Headless, no GPU. Every tab, dialog and menu through the AccessKit tree: names, duplicate names, 24 by 24 targets, keyboard reach, contrast, reduce motion. Table per screen in the job summary. See [accessibility.md](accessibility.md) |
| Localization | `cargo xtask i18n` | Locale key and placeholder parity, unused keys, hard-coded text, joined fragments, the `en-XA` pseudo-locale at 1280 and 320 points, font coverage. See [localization.md](localization.md) |
| WebAssembly | `cargo xtask wasm` | The shared UI and the browser entry point build for wasm32 |
| Tooltip text | `cargo xtask tooltips` | Lists tooltips that repeat their label, are too short or long, omit a limit, or leave a disabled control unexplained (docs/tooltips.md). Non-fatal until `--strict` is switched on (#631); exemptions with reasons live in `crates/ui-egui/tooltip-exemptions.txt` |
| Minimum Rust version | `cargo xtask msrv` | Builds with the `rust-version` in Cargo.toml |
| Licenses and advisories | `cargo xtask deny` | `deny.toml`: GPL-3.0 compatible license allow list, RustSec advisories, yanked crates, unknown registries and git sources |
| Coverage | `cargo xtask coverage` | Runs the workspace tests under cargo-llvm-cov 0.9.1 (source-based LLVM coverage on stable with the `llvm-tools` component). Fails when workspace line coverage is under 70% or when any crate falls more than 0.5 points below `ci/coverage-baseline.json`. Uploads `coverage` (lcov and HTML) and writes the per-crate table to the job summary. Ready pull requests and main only |
| E2E and control coverage | `cargo xtask e2e` | Runs every `a_newcomer_*` story, walks every page of the UI, and fails with one line per control that has no tooltip, no story or no id (message names the id). Uploads `e2e-coverage` (report and logs). Exemptions: `e2e-exemptions.txt` |
| UI screenshots | `cargo xtask screenshots` | Launches the real app under Xvfb with Mesa software Vulkan and captures eight views |
| Build | `cargo build --locked` | Windows on every ready pull request, macOS also on main and the merge queue |
| CI result | none | The one check to mark as required in branch protection |

Speed and friendliness to stacked pull requests:

- The concurrency group is the workflow plus the pull request number. A new push, or marking the pull request ready, cancels the older run of that pull request. Runs on main, in the merge queue and manual runs are never cancelled.
- Documentation-only changes skip the build jobs; the policy, deny and result jobs still run.
- Caches (rust-cache) are written only by main and read by everything else.
- A stacked branch gets results once its pull request exists, even a draft one, whatever its base branch.
- Job names are unchanged, so existing branch status checks still match.

## Coverage gate

`cargo xtask coverage` is the local twin of the `Coverage` job: it needs
`rustup component add llvm-tools` and
`cargo install --locked cargo-llvm-cov --version =0.9.1` (Apache-2.0 OR MIT,
run as a test tool only). It runs the workspace tests with nextest, writes
`target/coverage/lcov.info`, `target/coverage/html/` and `target/coverage/summary.md`
(the per-crate table), and applies two checks:

- **The bar:** workspace line coverage of at least 70% (`gate.minimum_lines` in `ci/coverage.json`). The starting measurement was 85.9%, so the bar is on the whole workspace. The file also supports `"mode": "patch"`, which checks that the lines a pull request adds are covered at the minimum (pass `--base REF`).
- **The ratchet:** no crate may fall more than 0.5 points below its entry in `ci/coverage-baseline.json`. After a change that raises coverage, run `cargo xtask coverage --bless` and commit the new baseline so the gain cannot be lost. Raise the 70% bar in `ci/coverage.json` as the baseline allows.

Line coverage counts instrumented lines only; blank lines and comments are not
in the number. The exclusions are listed with their reasons in
`ci/coverage.json` and nowhere else:

- `xtask/*`: repository automation, including the generator of the showcase example data (the examples in `examples/` are not Rust and are not measured).
- `apps/*/src/main.rs`: binary entry glue. The logic it calls lives in library crates, which are measured.
- `apps/pcbcraft-web/*`: the browser entry point, built for wasm32 only and checked by the WebAssembly job.
- `*/tests/*`, `*/tests.rs` and `crates/ui-egui/src/e2e/*`: test-only code, which is not product code.
- `#[cfg(test)] mod` blocks inside source files: removed by reading the source, because stable Rust cannot mark them for the tool.

A file is never excluded because it is poorly covered. The Coverage job does not
reuse the Tests cache: instrumented builds go to `target/llvm-cov-target` with
different compiler flags, so the job keeps its own cache (written by main).
Expect about 5 minutes of tests plus the instrumented build; a cold build is longer.

## Tests must assert behaviour

Coverage is a floor, not a goal. A test that only executes code to raise the
number is not acceptable and is rejected in review. Every test asserts an
observable result (a returned value, a changed document, a drawn label, an
error), and new code arrives with tests that fail if its behaviour breaks.
Check by breaking the code: change the behaviour on purpose and confirm the
test goes red. Do not lower the bar, the baseline or the exclusions to make a
change pass; add the missing test.

## The e2e gate

`cargo xtask e2e` is the local twin of the `E2E stories and control coverage`
job. A control id is its kind plus its label with digits folded to `#`
(`button:Add pad`), or `icon:<id>` for tool-strip icons. The stories log the
controls they press or type into; a walk of every page logs the controls it
paints and hovers each one for a tooltip. A control must show a tooltip and be
touched by at least one story. Every new control needs a label (or a
`record_icon` id), a tooltip, and a story.

`e2e-exemptions.txt` holds reviewed exceptions, one per line as
`kind | id | reason` (kind `tooltip`, `story` or `name`, an id ending in `*`
matches by prefix). The current list is the first run's backlog, tracked in
#520, and may only shrink: an entry that matches nothing fails the gate. The
job `strings` runs `cargo xtask strings check` on every Rust change.

## Status data (`cargo xtask status-data`)

Writes `site/src/data/status.json` for the website status page: per feature (from
`site/src/data/features.yaml`) its issues and their state, merged pull requests, the
first green CI run after each merge, and the tests that prove it; plus the `ROADMAP.md`
lines. The tracker is the fork, `wdunn001/pcbcraft`.

- Offline by default. It reads a cached snapshot, `site/src/data/status-snapshot.json`
  (not committed). `--refresh` rewrites the snapshot with about 10 GitHub API requests
  (`GITHUB_TOKEN` is used when set); if the refresh fails the cached snapshot is used and
  the output says `stale: true`.
- A state is Built only with a merged pull request and a green run; Released only with a
  release published after the merge. A feature with no `test:` evidence that matches a
  `#[test]` function is reported as `no-test`.
- `--check FILE` validates a generated file against `site/src/data/status.schema.json`.
- Tests use the recorded snapshot in `xtask/tests/fixtures/status/` and never touch the
  network. Rewrite the golden with `STATUS_BLESS=1 cargo test -p xtask status`.
- Pull requests merged before the tracker moved name old issue numbers; the generator
  maps them through each issue's "Migrated from" line.

## Seeing UI changes

The screenshots job uploads `screenshots-head` (this change) and, on pull
requests, `screenshots-base` (the target branch). A final job builds one
side-by-side image, `ui-screenshots.png`, and links it from the run summary.
Open the run, then the Summary page, and follow the link. Left column is the
base, right column is the change. Views are listed in `VIEWS` in
xtask/src/main.rs; add a row there when a new tab or dialog deserves a shot.

Locally: `cargo xtask screenshots --out target/screenshots`.

## Nightly (`nightly.yml`)

At 05:17 UTC, if main changed in the last day (or on manual dispatch), builds
the same platform packages as a tagged release (via `build-dist.yml`) as
workflow artifacts kept 14 days, and runs an advisory scan so new RustSec
advisories surface without a code change.

On the scheduled run only (not manual dispatch), when main changed, the
workflow also publishes a GitHub pre-release. The tag is
`v<workspace-version>-nightly.<YYYYMMDD ET>.<run>.<attempt>` (Eastern date,
matching other Craft suite nightlies). Assets include every nightly archive plus
`SHA256SUMS.txt`. Pre-releases are published with `--latest=false`; they never
replace the latest stable download links from tagged releases.

## Releases (`release.yml`, `build-dist.yml`)

Push a tag `vX.Y.Z` (or `vX.Y.Z-rc.1`) that matches the workspace version in
Cargo.toml. The workflow builds, from clean state:

- `pcbcraft-<tag>-linux-x86_64.AppImage` and `-linux-aarch64.AppImage` (+ `.zsync` update files)
- `pcbcraft-<tag>-linux-x86_64.deb` and `-linux-aarch64.deb`
- `pcbcraft-<tag>-linux-x86_64.rpm` and `-linux-aarch64.rpm`
- `pcbcraft-<tag>-linux-x86_64.flatpak` and `-linux-aarch64.flatpak`
- `pcbcraft-<tag>-linux-x86_64.tar.gz` and `-linux-aarch64.tar.gz` (FHS layout)
- `pcbcraft-<tag>-windows-x86_64.zip`
- `pcbcraft-<tag>-macos-universal.tar.gz` and `.dmg` (Apple silicon and Intel)
- `pcbcraft-<tag>-web.zip` (trunk build of apps/pcbcraft-web)
- `pcbcraft-<tag>-source.tar.gz` and `-source-vendored.tar.gz` (complete corresponding source, GPL-3.0)
- `SHA256SUMS`

Everything lands in a DRAFT GitHub Release. A maintainer reviews and publishes
it by hand; nothing publishes automatically. Release notes list commit subjects
since the previous tag.

The nightly and release workflows share `build-dist.yml`, so a nightly run
proves the release packaging works. Linux AppImage, .deb, .rpm and the FHS
tarball come from `packaging/linux/package.sh`. Archives for every platform
are assembled by `.github/scripts/package.sh`.

## Security posture

- Every action is pinned to a commit SHA with the version in a comment. Dependabot opens weekly updates for actions and Cargo (egui, eframe and craft-ui are excluded and bumped by hand).
- The workflow default is `permissions: {}`; each job asks for `contents: read`. The release draft job and the nightly pre-release publish job have `contents: write`.
- Checkouts do not persist credentials. Untrusted values (tag names, inputs) reach shell steps through `env`, never inline.
- No secrets are used.

## Limits

Release packaging, signing and hosted web bundles are maintainer decisions listed
below. Fork branch pushes do not publish artifacts unless a maintainer cuts a tag.

## Brand icons

The logo masters and every platform icon live in `assets/brand/`. Run `assets/brand/build.sh` (needs docker) to regenerate the masters, the Linux hicolor PNGs and scalable SVG, the Windows `.ico`, the macOS `.icns` and the web icons in `assets/brand/generated/`. Commit the result; packaging and the app embed the committed files and need no rasteriser.

## See also

- [getting-started.md](getting-started.md) for branch and proof workflow
- [CONTRIBUTING.md](../CONTRIBUTING.md) for the full delivery pipeline

## Open decisions for the maintainers

- Code signing and notarization: Windows Authenticode and Apple Developer ID need paid certificates stored as repository secrets. Binaries are unsigned until then, and macOS users must open the app via right-click.
- Where the web bundle is hosted (Pages or another host) and whether tags publish it.
- Marking `CI result` as a required check in branch protection.
