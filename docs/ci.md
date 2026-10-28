# Continuous integration and releases

Every CI job runs one `cargo xtask` subcommand, so any failure reproduces on a
workstation with the same command. `cargo xtask ci` runs policy, fmt, clippy,
tests, wasm and docs in one go and must be green before pushing.

## Pull request gate (`.github/workflows/ci.yml`)

| Job | Command | What it enforces |
| --- | --- | --- |
| Format and policy | `cargo xtask policy`, `cargo xtask fmt` | Crate dependency layers, workspace lints keep `unwrap_used`, `expect_used`, `panic`, `unimplemented`, `todo` and `unreachable` at deny (AGENTS.md), no source `allow` of them, rustfmt |
| Clippy and docs | `cargo xtask clippy`, `cargo xtask docs` | Clippy with `-D warnings` on all targets, rustdoc with warnings denied |
| Tests | `cargo xtask test` | Workspace tests with cargo-nextest (profile `ci`, JUnit report uploaded) plus doctests. Includes the headless `a_newcomer_*` end-to-end tests in crates/ui-egui |
| WebAssembly | `cargo xtask wasm` | The shared UI and the browser entry point build for wasm32 |
| Minimum Rust version | `cargo xtask msrv` | Builds with the `rust-version` in Cargo.toml |
| Licenses and advisories | `cargo xtask deny` | `deny.toml`: GPL-3.0 compatible license allow list, RustSec advisories, yanked crates, unknown registries and git sources |
| UI screenshots | `cargo xtask screenshots` | Launches the real app under Xvfb with Mesa software Vulkan and captures eight views |
| Build | `cargo build --locked` | Windows on every change, macOS also on main |
| CI result | none | The one check to mark as required in branch protection |

Speed and friendliness to stacked pull requests:

- A new push cancels the older run for the same pull request. Runs on main are never cancelled.
- Documentation-only changes skip the build jobs; the policy, deny and result jobs still run.
- Caches (rust-cache) are written only by main and read by everything else.
- Forks run on every push, so a stacked branch gets results before its pull request exists. In the upstream repository, plain branch pushes are covered by the pull request run.

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
the packages below as workflow artifacts kept 14 days, and runs an advisory scan
so new RustSec advisories surface without a code change.

## Releases (`release.yml`, `build-dist.yml`)

Push a tag `vX.Y.Z` (or `vX.Y.Z-rc.1`) that matches the workspace version in
Cargo.toml. The workflow builds, from clean state:

- `pcbcraft-<tag>-linux-x86_64.tar.gz` and `-linux-aarch64.tar.gz`
- `pcbcraft-<tag>-windows-x86_64.zip`
- `pcbcraft-<tag>-macos-universal.tar.gz` and `.dmg` (Apple silicon and Intel)
- `pcbcraft-<tag>-web.zip` (trunk build of apps/pcbcraft-web)
- `pcbcraft-<tag>-source.tar.gz` and `-source-vendored.tar.gz` (complete corresponding source, GPL-3.0)
- `SHA256SUMS`

Everything lands in a DRAFT GitHub Release. A maintainer reviews and publishes
it by hand; nothing publishes automatically. Release notes list commit subjects
since the previous tag.

The nightly and release workflows share `build-dist.yml`, so a nightly run
proves the release packaging works. Packaging logic is in
`.github/scripts/package.sh`.

## Security posture

- Every action is pinned to a commit SHA with the version in a comment. Dependabot opens weekly updates for actions and Cargo (egui, eframe and craft-ui are excluded and bumped by hand).
- The workflow default is `permissions: {}`; each job asks for `contents: read`. Only the release draft job has `contents: write`.
- Checkouts do not persist credentials. Untrusted values (tag names, inputs) reach shell steps through `env`, never inline.
- No secrets are used.

## Open decisions for the maintainers

- Code signing and notarization: Windows Authenticode and Apple Developer ID need paid certificates stored as repository secrets. Binaries are unsigned until then, and macOS users must open the app via right-click.
- Package formats: AppImage, .deb, .msi or Flatpak are not produced yet.
- Where the web bundle is hosted (Pages or another host) and whether tags publish it.
- Marking `CI result` as a required check in branch protection.
