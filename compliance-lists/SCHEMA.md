# Compliance list snapshot TOML schema

Each file is one dated list snapshot (entity names, substance names, or both). Lists ship with the app under `compliance-lists/` and users can import a newer file with `compliance.lists_update`.

## Header

| Field | Type | Notes |
| --- | --- | --- |
| `schema` | integer | Must be `1`. |
| `id` | string | Stable slug, 1 to 64 bytes, `[a-z0-9-]` only. |
| `title` | string | Display title, 1 to 256 bytes. |
| `source_url` | string | Where the list was published, up to 2048 bytes. Required for bundled lists. |
| `published` | string | Optional publication date `YYYY-MM-DD`. |
| `retrieved` | string | When this snapshot was retrieved, `YYYY-MM-DD`. Required for bundled lists. |
| `licence` | string | SPDX or plain licence label, 1 to 256 bytes. Required for bundled lists. |
| `cadence_days` | integer | Optional refresh interval; findings warn after this many days since `retrieved`. |

## Entity entries

Array of tables `[[entities]]`:

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | Primary name, 1 to 256 bytes. |
| `aliases` | array of string | Up to 32 aliases, each 1 to 256 bytes. |
| `identifiers` | array of string | Up to 16 ids, each 1 to 128 bytes. |
| `country` | string | Optional ISO country code, up to 8 bytes. |
| `parent` | string | Optional parent entity name; ownership is never inferred beyond this field. |
| `added` | string | Optional `YYYY-MM-DD`. |
| `citation` | string | Optional note, up to 512 bytes. |

## Substance entries

Array of tables `[[substances]]`:

| Field | Type | Notes |
| --- | --- | --- |
| `cas` | string | Optional CAS registry number, up to 32 bytes. |
| `ec` | string | Optional EC number, up to 32 bytes. |
| `name` | string | Optional name when no registry id is set. |
| `added` | string | Optional `YYYY-MM-DD`. |

At most 8192 entries total across both arrays. Files larger than 2 MiB are refused.

Distributor listings are never stored in a snapshot.
