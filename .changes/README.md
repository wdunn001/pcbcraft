# Changesets

Every pull request adds exactly one file here, named after its branch slug (`fix-track-snap.md`), so two pull requests never collide. The file is the release note and the proof that documentation was considered. `cargo xtask changeset check` (CI job "Changeset and docs") enforces it.

## Template

Copy this, name it `.changes/<branch-slug>.md`, and edit every line:

```
---
type: fix
area: layout
issues: [123]
user_visible: true
docs: [docs/getting-started.md]
summary: One plain sentence a user could read.
---
```

## Fields

- `type`: feature, fix, docs, ci, refactor, test or chore.
- `area`: a tab or crate in lowercase, for example layout, library, engine, io, site.
- `issues`: fork issue numbers, `[]` when there are none.
- `user_visible`: `true` if a user of the app or site can notice the change.
- `docs`: the documentation files this change updated, as a list, or `n/a: <reason of at least 20 characters>`. Each listed file must be changed in the pull request.
- `summary`: one plain sentence a user could read. No dashes used as punctuation.

Each field is one `key: value` line, split at the first colon, so other tools read the files without a YAML library (`cargo xtask changeset list` prints them as JSON).

## What counts as documentation

At least one documentation change is required when `user_visible` is true, or when the pull request touches crates/ui-egui, crates/engine/src, the writers in crates/io/src, or site/. Documentation is any change under docs/, site/src/content/docs/, README.md, CHANGELOG.md or .cursor/rules/. Outside crates/ui-egui, new `//!` file headers and `///` doc blocks in Rust code count too, so list those Rust files in `docs`. The alternative is `docs: n/a: <reason>`. Docs-only, ci-only and bot pull requests use `type: docs`, `ci` or `chore` with `user_visible: false`.

The documentation itself follows the standard in [docs/doc-standard.md](../docs/doc-standard.md), from [the article behind it](https://themildtake.com/articles/2026-05-05-documentation-beating-your-ai-stack/): intent lives next to the code in layered file, contract, function and implementation blocks. `cargo xtask docs check` is the quality gate for those blocks; the changeset check only asks whether documentation was touched and a changeset exists. A change to a trait or other contract should also update its `# Implementations` list, or say `docs: n/a: <reason>`.

## Examples

A user-visible fix with documentation:

```
---
type: fix
area: layout
issues: [412]
user_visible: true
docs: [docs/getting-started.md]
summary: Tracks now snap to the grid while you drag them.
---
```

An internal change with no documentation:

```
---
type: refactor
area: engine
issues: []
user_visible: false
docs: n/a: pure move of code between files, behaviour unchanged
summary: The routing code now lives in its own module.
---
```

## Releases

`cargo xtask changelog --version X` writes the unreleased files into CHANGELOG.md grouped by area and type with issue links, then moves them to `.changes/released/X/`. Without `--version` it only prints the section.
