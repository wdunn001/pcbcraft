# Part search index (tags and vectors)

Offline-first search ranks normalized part listings for a stated need. Tags give exact filters; keyword tokens index description, MPN and parameters; optional embedding vectors add cosine similarity when both the query and the part carry one.

## Library crate

- `crates/library/src/part_search.rs` builds and queries `PartSearchIndex`.
- `tags_for_listing` derives controlled tags (function, package, source, manufacturer, voltage class, certification) and free tags from text.
- Index JSON uses `INDEX_VERSION` and is meant to sit beside the persistent part cache, rebuilt from listings when sources change.

## Engine and MCP

- `parts.search` gathers project and installed listings like `library.list`, applies `PartSearchQuery`, and returns ranked hits with `reasons`.
- Keyword and tag ranking work with no model. Vectors are optional on both the query and stored parts.

## Quality

- `recall_at_k` in the library tests runs on at least thirty synthetic needs in CI.

Relates to issue 946 (child of online libraries).
