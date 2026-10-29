# Releasing PCBCraft

This document describes how to verify a release draft and publish it.
Release automation for WinGet and Homebrew runs only after a release is
published.

## Prerequisites

- Write access to the `tangency-labs/pcbcraft` repository.
- The `WINGET_TOKEN` and Homebrew bump token secrets configured in the
  repository settings (see below).

## Creating a Release Draft

1. Ensure the target branch (usually `main`) is green on GitHub Actions.
2. Tag the commit: `git tag -a v0.1.0 -m "Release v0.1.0"`
3. Push the tag: `git push origin v0.1.0`
4. On GitHub, go to Releases and create a draft from the tag.
5. Fill in the release notes with user-visible changes.

## Verifying the Draft

Before publishing, verify:

- [ ] All GitHub Actions pass on the tagged commit.
- [ ] Release notes accurately describe changes.
- [ ] Assets (installers, binaries) are attached if applicable.
- [ ] The version number follows semver and matches the tag.

## Publishing

1. On the release draft page, click "Publish release".
2. This triggers the WinGet and Homebrew automation workflows.
3. Monitor the workflows in the Actions tab.

## WinGet Automation

The `.github/workflows/winget.yml` workflow:

- Triggers when a release is published.
- Submits a PR to the WinGet package repository.
- Requires the `WINGET_TOKEN` secret (a GitHub PAT with repo scope).

To configure:
1. Create a GitHub PAT with `repo` scope.
2. Add it as `WINGET_TOKEN` in repository secrets.

## Homebrew Automation

The `.github/workflows/homebrew.yml` workflow:

- Triggers when a release is published.
- Bumps the formula in the Homebrew tap repository.
- Requires a Homebrew bump token secret.

To configure:
1. Create a GitHub PAT with access to the Homebrew tap repo.
2. Add it as the appropriate secret in repository settings.

## Rollback

If a release has issues:

1. Create a new patch release with the fix.
2. The automation will submit updated package versions.
3. For WinGet, older versions remain available until superseded.

## Notes

- Draft releases do not trigger automation.
- Only published releases trigger WinGet/Homebrew updates.
- Package IDs and tap names are configured in the workflow files.
