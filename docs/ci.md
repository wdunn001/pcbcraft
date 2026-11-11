# Continuous integration and releases

## Getting started

On [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft), push your branch
and open a pull request against `main` (never push to `main`). Wait for the green
**CI result** check on that commit and link the Actions run in the pull request
body. That run is the required proof. A local run of the same commands is
optional. Clone and first-run steps:
[getting-started.md](getting-started.md).

## The pipeline (`.github/workflows/ci.yml`)

Every job runs one `cargo xtask` command or one build, so a failure reproduces
with the same command. Jobs run on self-hosted runners, picked by two runner
labels. The small jobs (change detection, the site checks, the final result) use
one label. The Rust jobs and the deploy use the other label.
The gate runs on pull requests, on pushes to `main`, in the merge queue and on
manual dispatch. A new push to a pull request cancels its older run. Runs on
`main`, in the merge queue and by hand are never cancelled. Every commit on
`main` keeps its own result. The deploy (see below) waits for it.

A draft pull request runs Lint, the site checks when they apply and the result
job. The Clippy, Licenses, test, build and E2E jobs wait until the pull request is
marked ready. The first job, **Changes**, lists the files the change touches. When
only Markdown, `docs/`, licence files or issue templates changed, every Rust job
is skipped (Clippy and Licenses included). Lint still runs. The **Site** job runs
when `site/`, `docs/` or `ci.yml` changed. The last job, **CI result**, always
runs, fails if any job failed or was cancelled, and is the one required check.
A skipped job does not fail it.

| Stage | Job | Command | What it enforces |
| --- | --- | --- | --- |
| 0 | Changes | a shell step | Decides which of the other jobs run |
| 1 | Lint | `cargo xtask lint` (add `--json` for a per-step result array), then on pull requests `cargo xtask changeset require` | No committed conflict markers, every workflow file parses and every `needs:` names a job, rustfmt, workspace lint policy, crate dependency layers, the documentation standard (`docs check`), the text catalog (`strings check`). Then the change file rule (see below) |
| 1 | Clippy | `cargo xtask clippy` | Clippy with `-D warnings` on all targets |
| 1 | Licenses and advisories | `cargo xtask deny` | `deny.toml`: licence allow list, RustSec advisories, yanked crates, unknown sources |
| 2 | Unit tests | `cargo xtask test` | Workspace tests with cargo-nextest (profile `ci`, JUnit report uploaded) plus doctests |
| 3 | Check (wasm) | `cargo xtask wasm-check`, `cargo xtask wasm-deps` | The browser crates check for wasm32, wasm clippy blocks `std::time` clocks; the portable crates pass `cargo check` for wasm32 and the shared library revision is reported |
| 3 | Build (wasm) | `cargo xtask wasm-bundle` | Trunk release build with hashed file names, without wasm-opt (see below), then the headless first-frame smoke. Uploads `web-dist` |
| 3 | Build (Linux x86_64) | `cargo build --release --locked -p pcbcraft -p pcbcraft-cli` | The desktop app and CLI link. Uploads `linux-x86_64` |
| 3 | Build (Windows x86_64) | `cargo xwin build --release --locked --target x86_64-pc-windows-msvc -p pcbcraft -p pcbcraft-cli` | Cross-compiled on a Linux runner. Runs on `main`, in the merge queue and on manual dispatch, after the unit tests pass. Uploads `windows-x86_64` |
| 4 | E2E | `cargo xtask e2e` | Every `a_newcomer_*` story and a walk of every page (see below) |
| - | Site | the `npm run` checks in `site/` | Contrast, features, changelog reader, docs map, Astro check, build, links, content security policy, and a browser accessibility check |
| - | CI result | none | The single required check |

Licenses and advisories, Clippy and Lint gate the unit tests. The two wasm jobs,
the Linux build and E2E wait only for Lint and Clippy. They run side by side
instead of one stage after the other. The "stage" numbers are a reading order,
not a chain. The wasm work is two jobs (Check and Build) so the dev-profile checks
run while the release bundle compiles. The browser smoke runs once, in Build
(wasm), on the bundle it uploads.

Each Rust job uses its own cargo home inside the job. Two jobs on one runner
cannot rewrite each other's registry. The unit and E2E jobs move the build output
to a larger disk when the runner's work folder is small.

### Web bundle: with and without wasm-opt

wasm-opt took about half an hour of the pull request job and tells the pull request
nothing: the job exists to show that the app builds for wasm32 and starts. So
`cargo xtask wasm-bundle` builds a quick bundle by default: the `wasm-quick` cargo
profile (no LTO) and wasm-opt off. The result is large and is never shipped.

`cargo xtask wasm-bundle --optimize` (or `PCBCRAFT_WASM_OPT=1`) builds what
`apps/pcbcraft-web/index.html` describes: the `wasm-release` cargo profile (size
tuned, fat LTO, one codegen unit) and wasm-opt. A plain `trunk build --release` in
`apps/pcbcraft-web` does the same, so the deploy workflow needs no change. The
nightly job "Web bundle (optimized)" runs the optimized path every night so it
cannot rot. To change the optimized settings, edit the rust link in `index.html`
and keep the `OPTIMIZED_ATTRS` constant in `xtask/src/main.rs` equal to it: the
quick build swaps that text, and fails with a message when it is not found.

Local equivalents: `cargo xtask lint` for the Lint job. `cargo xtask ci` runs the
layer check, lint policy, rustfmt, Clippy, tests, the wasm check and bundle, and
the two documentation checks. It does not run `deny`, `e2e`, `strings check`, the
site checks or the Windows build. Run those commands on their own when your
change needs them.
The wasm jobs need `trunk`, `node` and the `wasm32-unknown-unknown` target; the
Windows job needs `cargo-xwin`, `clang` and `lld`.

The Windows binary is linked with `/STACK:16777216`, emitted by
`apps/pcbcraft/build.rs`. Windows fixes the main thread's stack at link
time and the MSVC default is 1 MiB. The window runs on that thread. The
app needs more than that to start. `.cargo/config.toml` raises
`RUST_MIN_STACK` to the same figure. That covers test and spawned threads
but never the main thread of a linked executable. The figure is reserved
address space. It is committed a page at a time and costs no memory.

The unit and e2e jobs build far more than the others. Every test binary is
linked. Where the work disk has under 20 GB free, they point
`CARGO_TARGET_DIR` at `/tmp` on the runner. On the container hosts that is
the big overlay rather than the small work mount.

This used to be done by replacing `target` with a symlink. The cache
restore that runs after it writes a real `target` directory and threw the
symlink away. The build went back to the small disk. The linker died of
no space. An environment variable survives the cache restore.

Nothing in CI runs the Windows binary. The job cross-compiles and uploads
it. A failure that only shows when the executable starts on Windows is
therefore not caught here.

Trunk downloads `wasm-opt` (binaryen) on first use, and only for an optimized build. The version is pinned in
`apps/pcbcraft-web/Trunk.toml` under `[tools]`, and the wasm job restores
trunk's tool cache (`~/.cache/trunk`) with `actions/cache`, keyed on that
version, so a run does not depend on the download. Change the pin and the
cache key follows. Any workflow that runs trunk should restore the same cache.

`wasm-deps` compares the `craft-libs` revision in `Cargo.toml` with a sibling
checkout (`../solidcraft`, or the path in `SOLIDCRAFT_DIR`) and prints a warning
when they differ. It skips the comparison quietly when that checkout is absent.

## Changelog and release notes

Every pull request adds exactly one change file under `.changes/`, named after its branch (format in `.changes/README.md`). The step "Require a change file" in the Lint job runs `cargo xtask changeset require` on pull requests. It fails a pull request that changes the app, the engine or the site and adds none. It also fails when a change file the pull request adds does not parse. A pull request carrying the `docs-only` label is exempt, as is one that changes documentation only.

The step checks only that a valid file is there. You run the stricter rules yourself with `cargo xtask changeset check`: exactly one change file, the `docs` list names only files the pull request changes, and a user visible change carries a documentation change or a `docs: n/a: <reason>` line of at least 20 characters. A pull request that lists a documentation file it did not change is sent back in review.

The site builds its Changelog page from those files. `site/src/lib/changelog-core.js` reads them, keeps the entries marked `user_visible: true` and sorts them newest first. The date and the pull request number come from the git history of the file, so the site and release jobs check out the full history. Files moved to `.changes/released/` by `cargo xtask changelog --version X` are still listed, so the history stays complete.

The `publish-native` job in `cd.yml` writes the same entries into the notes of the `main-latest` release with `site/scripts/release-notes.mjs`. It lists the entries added since the commit named in the previous notes. `npm run test:changelog` in `site/` tests the reader and the notes.

## Nightly and off the pull request path

These checks do not run per pull request. They run in `nightly.yml`, each with
its own `cargo xtask` command:

- Coverage: `cargo xtask coverage` (see below)
- Accessibility: `cargo xtask a11y` ([accessibility.md](accessibility.md))
- Localization: `cargo xtask i18n`
- Test standards: `cargo xtask test-standards`
- Minimum Rust version: `cargo xtask msrv`
- UI screenshots: `cargo xtask screenshots --out target/screenshots`

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
touched by at least one `a_newcomer_*` story. Every new control needs a label (or a
`record_icon` id), a tooltip, and a story. The gate fails when a painted control has
no tooltip, no story, or no usable id.

`e2e-exemptions.txt` holds reviewed exceptions, one per line as
`kind | id | reason` (kind `tooltip`, `story` or `name`, an id ending in `*`
matches by prefix). The current list is the first run's backlog, tracked in
#520, and may only shrink: an entry that matches nothing fails the gate.

## Rules the gates enforce

- **Tests return `Result`.** The workspace denies `unwrap_used` and `expect_used` for all targets, tests included. Clippy runs with `--all-targets`. A test returns `Result<(), String>` and uses `?`. It does not call `unwrap` or `expect`.
- **No lint opt-outs.** `cargo xtask lint` fails on any `allow` or `expect` attribute that names `unsafe_code` or one of `unwrap_used`, `expect_used`, `panic`, `unimplemented`, `todo` or `unreachable`. Only `crates/clock` may opt out of the clock ban in `clippy.toml`. Every crate inherits the workspace lints.
- **Fonts.** Text that sits on the drawing asks `role_font` in `crates/ui-egui/src/canvas/text_style.rs` for its font, or `font_mm` when the designer sized it. No painter picks a fixed point size for drawing text.
- **Strings.** User facing text lives in `crates/text/strings/en.txt`. `cargo xtask strings check` runs in the Lint job. It rejects duplicate keys and a translation that changes a placeholder or has no English key.
- **Docs check.** `cargo xtask docs check` runs in the Lint job (rules in [doc-standard.md](doc-standard.md)). A `# Dependencies` list in a module header names repo-root paths that exist. A module with no dependencies has no such section.

## Status data (`cargo xtask status-data`)

Writes `site/src/data/status.json` for the website status page: per feature (from
`site/src/data/features.yaml`) its issues and their state, merged pull requests, the
first green CI run after each merge, and the tests that prove it; plus the `ROADMAP.md`
lines. The tracker is `wdunn001/pcbcraft`.

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

The nightly workflow captures the desktop UI; to do it locally:

`cargo xtask screenshots --out target/screenshots`

The site images come from the same captures. Run `xvfb-run -a -s "-screen 0 1920x1240x24" cargo xtask screenshots --bin target/release/pcbcraft --out target/shots`, then `cd site && npm ci && node scripts/make-screens.mjs --captures ../target/shots`. The crops are listed in `site/src/data/screens.json` and `site/src/data/guide-shots.json`; each run starts the app with an empty settings folder so the dark default theme and the standard layout are used.

Views are listed in `VIEWS` in xtask/src/main.rs.

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

A published nightly tag is never moved or replaced, so a guide or a launcher can
link straight to one asset of one nightly. A manually dispatched run builds the
packages as workflow artifacts only and does not publish, so running the workflow
by hand adds no entries to the Releases page. Tagged releases stay draft until a
maintainer publishes them by hand; nightly publication never touches a tagged
release.

## Continuous deployment (`cd.yml`)

When the **CI** workflow finishes green on a push to `main`, `cd.yml` runs. It only
trusts push runs of this repository on `main`, never pull request or fork runs. It
can also be started by hand from `main`. Every commit on `main` gets a full CI run
(runs on `main` are never cancelled). A deploy always follows a finished run.
Deploys queue in the `cd` concurrency group and never cancel a deployment in flight.
Both jobs use the exact commit CI tested.

- **Deploy site and demo** (`deploy-site`): runs on a self-hosted runner, as the local deploy step. It builds the Astro site in `site/` (Node 22), runs the site checks (`test:contrast`, `test:features`, `test:docs-map`, `check`, `test:links`, `test:csp`), builds the wasm demo with `trunk build --release --public-url /demo/` (hashed file names, see `apps/pcbcraft-web/Trunk.toml`), then runs `site/scripts/deploy.sh`. The docs pages of the site are generated from `docs/` during the site build.
- **Publish native builds** (`publish-native`): runs on a GitHub hosted runner. It takes the `linux-x86_64` and `windows-x86_64` artifacts of that CI run and replaces the rolling `main-latest` pre-release with `pcbcraft-main-linux-x86_64.tar.gz`, `pcbcraft-main-windows-x86_64.zip` and `SHA256SUMS.txt`. It does nothing when either artifact is missing (a documentation-only run, or artifacts past retention). The pre-release is never marked latest.

### Pointing a runner at a deploy target

No host, path or key is in the repository. On the self-hosted runner that runs the
`deploy-site` job:

1. Copy `site/.env.deploy.example` to a file outside any checkout, readable by the runner
   user, and fill it in (ssh target, remote root, optional `DEPLOY_CHECK_URL`).
2. Set `PCBCRAFT_DEPLOY_ENV` to that file's path, either in the runner's own environment
   (its `.env` file) or as a repository variable of the same name.
3. Use whatever ssh key the runner user already has. The script runs ssh in batch mode and
   takes extra options from `DEPLOY_SSH_OPTS`.

Until step 2 is done the deploy job logs `no deploy target configured`, skips the builds
and ends green.

### What the deploy does

The remote root holds `releases/<UTC time>-<sha>/` directories and a `current` symlink.
`deploy.sh` copies the site into a staging directory, adds the demo under `demo/`, writes a
`demo/_headers` file (hashed files immutable, the entry page revalidated), precompresses
text files for `gzip_static`, writes a checksum manifest, rsyncs the lot into a new release
directory, verifies the manifest on the target, swaps `current` atomically and prunes to the
newest `DEPLOY_KEEP` releases (at least two).

The smoke step then needs `DEPLOY_CHECK_URL`: the site root and `/demo/` must answer 200, and
the hashed `*_bg.wasm` and JavaScript the demo page names must resolve, the wasm starting
with the WebAssembly magic bytes. When the smoke fails, `deploy.sh` switches `current` back
to the previous release, removes the failed one and exits non-zero. The web server is
responsible for sending long `Cache-Control` values for hashed files. The smoke prints what
it sees.

### Rolling back

From a checkout on the deploy runner (or any machine with the env file and ssh access):

```
DEPLOY_ENV_FILE=/path/to/env site/scripts/deploy.sh list
DEPLOY_ENV_FILE=/path/to/env site/scripts/deploy.sh rollback          # previous release
DEPLOY_ENV_FILE=/path/to/env site/scripts/deploy.sh rollback NAME     # a named release
```

A rollback is one atomic symlink swap and runs the same smoke check afterwards.

## Releases (`release.yml`, `build-dist.yml`)

Push a tag `vX.Y.Z` (or `vX.Y.Z-rc.1`) that matches the workspace version in
Cargo.toml. The workflow builds, from clean state:

- Per Linux architecture (`x86_64`, `aarch64`):
  - `pcbcraft-<tag>-linux-<arch>.tar.gz` (FHS tree, also the input of the Flatpak and Snap jobs)
  - `pcbcraft-<tag>-linux-<arch>.AppImage` and `.AppImage.zsync`
  - `pcbcraft-<tag>-linux-<arch>.deb` and `.rpm`
  - `pcbcraft-<tag>-linux-<arch>.flatpak` and `.snap`
- Windows x86_64: `pcbcraft-<tag>-windows-x86_64.msi`, `-windows-x86_64-portable.zip`, the standalone `PCBCraft-<tag>-windows-x86_64.exe` and the legacy `-windows-x86_64.zip` (loose exes)
- `pcbcraft-<tag>-macos-universal.tar.gz` and `.dmg` (Apple silicon and Intel)
- `pcbcraft-<tag>-web.zip` (trunk build of apps/pcbcraft-web)
- `pcbcraft-<tag>-source.tar.gz` and `-source-vendored.tar.gz` (complete source, MIT OR Apache-2.0)
- `SHA256SUMS.txt` (one line per attached file)

Tagged archives also get version-less copies (for example `pcbcraft-linux-x86_64.tar.gz`,
`pcbcraft-linux-x86_64.AppImage`, `pcbcraft-linux-x86_64.deb`) so
`releases/latest/download/<name>` links keep working. AppImage zsync metadata points at the
version-less `.AppImage.zsync` on the latest release.

Everything lands in a DRAFT GitHub Release. A maintainer reviews and publishes
it by hand; nothing publishes automatically. Release notes list commit subjects
since the previous tag.

The nightly and release workflows share `build-dist.yml`, so a nightly run
proves the release packaging works. Linux packaging is in `packaging/linux/package.sh`; macOS and the legacy Windows zip use `.github/scripts/package.sh`. Windows MSI and portable zip use `packaging/windows/package.ps1` with WiX sources in `assets/platforms/windows/`. Linux installers are built in `packaging/linux/package.sh` (called from the Linux matrix jobs in `build-dist.yml`). Flatpak and Snap repackage the FHS tarball via `packaging/linux/flatpak-bundle.sh` and `packaging/linux/snap-pack.sh`. Legacy flat Linux tarballs, Windows zip, macOS bundles, and the web zip still come from `.github/scripts/package.sh`.

### Windows MSI locally (maintainers)

On Windows x86_64 with Rust (MSVC toolchain) and the .NET SDK:

```powershell
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2 WixToolset.Util.wixext/5.0.2
bash packaging/windows/make-icon.sh icons
# or use committed assets/brand/generated/windows/pcbcraft.ico
pwsh packaging/windows/package.ps1
```

Outputs land in `dist/`. Installers are unsigned until code signing lands (#297); Windows SmartScreen may warn on first run.

### Building Linux installers locally

On Ubuntu 22.04 or similar, after a release build for the host triple:

```bash
export PCBCRAFT_DIST_TAG=v0.0.0-local
export DIST="$PWD/dist"
mkdir -p "$DIST"
cargo build --locked --release -p pcbcraft -p pcbcraft-cli
packaging/linux/package.sh --skip-build
```

Install `nfpm`, `desktop-file-utils`, `appstream`, and `zsync` for deb, rpm, and AppImage zsync output (the script prints a warning if zsync is missing). The first AppImage build downloads `appimagetool` into `target/` when it is not on `PATH`.

Flatpak and Snap need extra host tooling and match what CI runs:

- Flatpak: build the FHS tarball (`packaging/linux/package.sh --skip-build --formats tar`), then `packaging/linux/flatpak-bundle.sh dist/pcbcraft-$PCBCRAFT_DIST_TAG-linux-$(uname -m).tar.gz` (needs `flatpak`, `flatpak-builder`, and `librsvg2-common`).
- Snap: same tarball, then `packaging/linux/snap-pack.sh` (needs `snapcraft`; CI uses `--destructive-mode` on Ubuntu 24.04).

CI does not upload to Flathub or the Snap Store; store listing and upload need owner secrets and are out of scope. Bundles are release artifacts only.

## Security posture

- Every action is pinned to a commit SHA with the version in a comment. Dependabot opens weekly updates for actions and Cargo (egui, eframe and craft-ui are excluded and bumped by hand).
- The workflow default is `permissions: {}`; each job asks for `contents: read`. The release draft job and the nightly pre-release publish job have `contents: write`.
- Checkouts do not persist credentials. Untrusted values (tag names, inputs) reach shell steps through `env`, never inline.
- Release signing reads repository secrets only in `release.yml` via `build-dist.yml`; values are passed through step `env`, never interpolated in `run:` scripts, and signing scripts keep `set +x`.

## Limits

Release packaging, signing and hosted web bundles are maintainer decisions listed
below. Fork branch pushes do not publish artifacts unless a maintainer cuts a tag.

## Brand icons

The logo masters and every platform icon live in `assets/brand/`. Run `assets/brand/build.sh` (needs docker) to regenerate the masters, the Linux hicolor PNGs and scalable SVG, the Windows `.ico`, the macOS `.icns` and the web icons in `assets/brand/generated/`. Commit the result; packaging and the app embed the committed files and need no rasteriser.

## See also

- [getting-started.md](getting-started.md) for branch and proof workflow
- [CONTRIBUTING.md](../CONTRIBUTING.md) for the full delivery pipeline

## Open decisions for the maintainers

- Code signing and notarization: Windows Authenticode and Apple Developer ID need paid certificates stored as repository secrets. Binaries and the Windows MSI are unsigned until then; macOS users must open the app via right-click, and Windows may show SmartScreen on first launch.
- Package formats: Linux AppImage, deb, rpm, Flatpak, and Snap ship from CI; Windows MSI and portable zip ship from CI. Other formats remain open.
- Where the web bundle is hosted (Pages or another host) and whether tags publish it.
- Whether to put Tests, Coverage, E2E, strings, localization or screenshots back on the gate later.
