# Plugin manifest schema (schema = 1)

Data-only plugins ship as a folder with a `plugin.toml` at the root. The core
loads manifests from a local directory; there is no network marketplace in this
slice.

## plugin.toml (required)

| Field | Type | Notes |
| --- | --- | --- |
| `schema` | integer | Must be `1`. |
| `id` | string | Stable id, 1 to 64 bytes, lowercase letters, digits and hyphens. |
| `name` | string | Title shown in lists, 1 to 128 bytes. |
| `version` | string | Semver string, 1 to 32 bytes. |
| `license` | string | SPDX id or plain licence name, 1 to 128 bytes. |
| `content_hash` | string | Optional `sha256:` hex digest of the manifest file bytes. |
| `export_templates` | array | Zero or more export template entries (see below). |
| `rule_set_bundles` | array | Optional paths to rule set files for the rules import path. |
| `importers` | array | Reserved; data-only importers name a format id and path. |

### export_templates[]

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Template id unique within the plugin, 1 to 64 bytes. |
| `path` | string | Relative path to the template TOML, 1 to 256 bytes. |
| `description` | string | Optional summary, up to 256 bytes. |
| `output_name` | string | Default file name for writes, 1 to 128 bytes. |

## Export template TOML (schema = 1, kind = pin_map)

Templates turn the generic pin map (net, reference, pin) into text without
code. Placeholders in `header` and `row`:

- `{net}` net name
- `{reference}` component reference
- `{pin}` pin number or name

| Field | Type | Notes |
| --- | --- | --- |
| `schema` | integer | Must be `1`. |
| `kind` | string | Must be `pin_map` in this slice. |
| `header` | string | Optional prefix (for example a CSV header line). |
| `row` | string | Required; one line per connected pin after sorting. |
| `sort` | string | `net_then_reference` (default) or `reference_then_pin`. |

## Rule sets without a plugin

Rule sets remain plain data imported into the rules library (same path as
built-in compliance and interface profiles). A plugin may list bundled rule set
files in `rule_set_bundles`; approval and import still go through the existing
rules import commands.

## Trust

Each plugin folder is approved explicitly by the user or operator. Optional
`content_hash` on the manifest supports tamper checks after install. Code
plugins (WebAssembly) are out of scope for schema 1.
