# Portal service HTTP API (first slice)

This document describes the sharing portal API before any live server ships.
Recorded JSON fixtures live under `crates/portal/tests/fixtures/` and match the
Rust types in `crates/portal/src/api.rs`.

## Capabilities

`GET /v1/capabilities` returns auth modes, upload limits and the static feed path.
Fixture: `capabilities.json`.

Anonymous callers may browse and download published objects. Upload and moderation
require a signed-in account or API token.

## Static registry feed

`GET /v1/feed/static` returns a pull-based feed document that points at the
generated catalogue for this instance. The on-disk layout matches the static
registry crate work in #987. Fixture: `static-feed.json`.

## Two-step upload

1. `POST /v1/uploads` with `UploadInitRequest` (publication id, version label,
   expected bundle root hash, manifest object hash, total bytes).
2. Upload bytes to the returned part URLs.
3. `POST /v1/uploads/{upload_id}/complete` with per-part content hashes.

After complete, Stage 0 checks run. New publishers always enter the human queue
unless policy rules and trust tier say otherwise. Fixture: `upload-init-response.json`.

## Immutable reads

`GET /v1/objects/{content_hash}` serves content-addressed bytes. Responses use
long-lived cache headers keyed by hash.

## List and search

`GET /v1/publications` accepts query text, tags, kind and cursor pagination.
Search runs on manifest fields stored in SQL, not inside object bytes.
Fixture: `publication-list.json`.

## Moderation queue

`GET /v1/moderation/queue` lists pending uploads for moderators.
`POST /v1/moderation/queue/{upload_id}` accepts approve, reject, request changes
or approve and trust. Every action appends a decision log row.
Fixture: `moderation-queue.json`.

## Reports and takedowns

`POST /v1/reports` opens a report on a publication version.
Takedown rows track notice, removal, counter-notice and restore states.

## Accounts

Email magic link, passkey and optional OpenID Connect sign-in are advertised in
capabilities. API tokens are created once and stored hashed server-side.

## Configuration

Operators supply `PortalConfig` (see `crates/portal/src/config.rs`) from
environment or external files. Example placeholder: `docs/portal-config.example.toml`.
