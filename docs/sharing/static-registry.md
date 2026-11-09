# Static registry (first slice)

A static registry is a folder tree (or git repository) that holds publication
versions, a generated catalogue, a small browse page, and an optional signed
registry manifest. PCBCraft lists registries in the public `sources.json` index
alongside git trees, offline packs, and peer instance feeds.

## Layout

```
registry/
  registry.json          catalogue hash and optional signature
  catalogue.json         search fields for every publication version
  index.html             generated browse page
  sources.json           optional local copy of index entries
  publications/
    <id>/
      <version>/
        publication.json   manifest with file hashes
        ...                payload files (or https URLs in the manifest)
  instances/
    instances.json         pull-based federation feed for this tree
```

## Publication manifest

Each version folder carries a `publication.json` file (`format`:
`pcbcraft-publication`, `schema_version`: 1). Files are listed with `path`,
`sha256`, and `size`. The `bundle_root_hash` is the sha256 of the sorted
`(path, sha256)` pairs. Install and CI always verify hashes before use.

## Engine tools

Read tools (via `library.read` / MCP host tools):

- `registry.catalogue` with `{ "path": "<registry root>" }`
- `registry.sources` with `{ "path": "<sources.json>" }`
- `registry.verify`, `registry.rebuild`, `registry.publish`, `registry.install`
- `registry.instance_items` merges local catalogue rows with a peer feed and
  marks remote rows with an `origin` field.

## CI

`cargo xtask registry check` verifies the fixture under
`crates/library/tests/fixtures/static-registry`. Regenerate with
`cargo xtask registry bless` when publications change.
