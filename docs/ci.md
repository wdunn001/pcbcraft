# Continuous integration and releases

## Getting started

On [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft), push your branch
and open a pull request against `main` (never push to `main`). Wait for the green
**CI result** check on that commit and link the Actions run in the pull request
body. That run is the required proof; you do not need a green local
`cargo xtask ci` on a laptop. Clone and first-run steps:
[getting-started.md](getting-started.md).

## The pipeline (`.github/workflows/ci.yml`)

Every job runs one `cargo xtask` command or one build, so a failure reproduces
with the same command. All jobs run on the owner's self-hosted Linux runners.
The gate runs on pull requests, on pushes to `main`, in the merge queue and on
manual dispatch. A new push to a pull request cancels its older run; runs on
`main` are never cancelled.

A draft pull request runs stage 1 only. A documentation-only change (Markdown,
`docs/`, licence files) skips stages 2 to 4. The last job, **CI result**, always
runs, fails if any job failed or was cancelled, and is the one required check.

| Stage | Job | Command | What it enforces |
| --- | --- | --- | --- |
| 1 | Lint | `cargo xtask lint` (add `--json` for a per-step result array) | No committed conflict markers, every workflow file parses and every `needs:` names a job, rustfmt, workspace lint policy, crate dependency layers, the documentation standard (`docs check`), the text catalog (`strings check`) |
| 1 | Clippy | `cargo xtask clippy` | Clippy with `-D warnings` on all targets |
| 1 | Licenses and advisories | `cargo xtask deny` | `deny.toml`: licence allow list, RustSec advisories, yanked crates, unknown sources |
| 2 | Unit tests | `cargo xtask test` | Workspace tests with cargo-nextest (profile `ci`, JUnit report uploaded) plus doctests |
| 3 | Build (wasm) | `cargo xtask wasm`, `cargo xtask wasm-deps` | The browser crates build for wasm32, wasm clippy blocks `std::time` clocks, trunk release build with hashed file names, headless first-frame smoke; the portable crates pass `cargo check` for wasm32 and the shared library revision is reported. Uploads `web-dist` |
| 3 | Build (Linux x86_64) | `cargo build --release --locked -p pcbcraft -p pcbcraft-cli` | The desktop app and CLI link. Uploads `linux-x86_64` |
| 3 | Build (Windows x86_64) | `cargo xwin build --release --locked --target x86_64-pc-windows-msvc -p pcbcraft -p pcbcraft-cli` | Cross-compiled on a Linux runner; main and the merge queue only. Uploads `windows-x86_64` |
| 4 | E2E | `cargo xtask e2e`, then `node apps/pcbcraft-web/scripts/smoke-first-frame.mjs` | Every `a_newcomer_*` story and a walk of every page (see below), then the browser smoke against the built `web-dist` bundle |
| - | CI result | none | The single required check |

Stage 2 waits for stage 1, stage 3 for stage 2, stage 4 for stage 3. Stage 3
jobs run in parallel and the wasm build is the one to watch.

Local equivalents: `cargo xtask lint` for stage 1, `cargo xtask ci` for stages
1 to 3 in one go.
The wasm jobs need `trunk`, `node` and the `wasm32-unknown-unknown` target; the
Windows job needs `cargo-xwin`, `clang` and `lld`.

`wasm-deps` compares the `craft-libs` revision in `Cargo.toml` with a sibling
checkout (`../solidcraft`, or the path in `SOLIDCRAFT_DIR`) and prints a warning
when they differ. It skips the comparison quietly when that checkout is absent.

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

## Releases (`release.yml`, `build-dist.yml`)

Push a tag `vX.Y.Z` (or `vX.Y.Z-rc.1`) that matches the workspace version in
Cargo.toml. The workflow builds, from clean state:

- `pcbcraft-<tag>-linux-x86_64.tar.gz` and `-linux-aarch64.tar.gz`
- `pcbcraft-<tag>-windows-x86_64.msi`, `-windows-x86_64-portable.zip`, standalone `PCBCraft-<tag>-windows-x86_64.exe`, and legacy `-windows-x86_64.zip` (loose exes)
- `pcbcraft-<tag>-linux-x86_64.tar.gz` and `-linux-aarch64.tar.gz` (FHS tree used by Flatpak and Snap repackage jobs)
- Per Linux architecture (`x86_64`, `aarch64`):
  - `pcbcraft-<tag>-linux-<arch>.AppImage` and `.AppImage.zsync`
  - `pcbcraft-<tag>-linux-<arch>.deb` and `.rpm`
  - `pcbcraft-<tag>-linux-<arch>.flatpak` and `.snap`
- `pcbcraft-<tag>-linux-x86_64.AppImage` and `-linux-aarch64.AppImage` (+ `.zsync` update files)
- `pcbcraft-<tag>-linux-x86_64.deb` and `-linux-aarch64.deb`
- `pcbcraft-<tag>-linux-x86_64.rpm` and `-linux-aarch64.rpm`
- `pcbcraft-<tag>-linux-x86_64.flatpak` and `-linux-aarch64.flatpak`
- `pcbcraft-<tag>-linux-x86_64.tar.gz` and `-linux-aarch64.tar.gz` (FHS layout)
- `pcbcraft-<tag>-windows-x86_64.zip`
- `pcbcraft-<tag>-macos-universal.tar.gz` and `.dmg` (Apple silicon and Intel)
- `pcbcraft-<tag>-web.zip` (trunk build of apps/pcbcraft-web)
- `pcbcraft-<tag>-source.tar.gz` and `-source-vendored.tar.gz` (complete source, MIT OR Apache-2.0)
- `SHA256SUMS`
- `pcbcraft-<tag>-source.tar.gz` and `-source-vendored.tar.gz` (complete corresponding source, GPL-3.0)
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

CI does not upload to Flathub or the Snap Store; bundles are release artifacts only.

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
- Package formats: AppImage, .deb, .msi or Flatpak are not produced yet.
- Code signing and notarization: Windows Authenticode and Apple Developer ID need paid certificates stored as repository secrets. Binaries are unsigned until then, and macOS users must open the app via right-click.
- Where the web bundle is hosted (Pages or another host) and whether tags publish it.
- Whether to put Tests, Coverage, E2E, strings, localization or screenshots back on the gate later.
