# Plugin system (schema 1)

PCBCraft stays a general core. Integrations that target one industry or one
firmware family ship as plugins: export templates, optional importers, and
bundled rule-set files. Rule sets also import on their own into the rules
library (same path as compliance packs and interface profiles); a plugin only
groups files for distribution.

## What a plugin can add

| Kind | Schema 1 | Notes |
| --- | --- | --- |
| Export template | yes | Data-only TOML over the generic pin map (net, reference, pin). |
| Rule set bundle | path only | Files listed in the manifest; import uses existing rules import. |
| Importer | reserved | Manifest slot for a future data-only format descriptor. |
| Firmware target generator | follow-up | Split from the firmware plugin work; not in this slice. |
| Sandboxed code | no | WebAssembly remains a later phase if data-only is not enough. |

Plugins read the design through the same project view as exports and MCP. They
must not mutate the document, run arbitrary code with file or network access by
default, or replace core commands.

## Form: data first

Schema 1 is manifest plus TOML templates only. See
[crates/plugin/SCHEMA.md](../crates/plugin/SCHEMA.md) for field lists. The
example fixture under `crates/plugin/tests/fixtures/example-pin-map/` is the
golden harness plugin authors can copy.

Engine commands:

- `plugins.list` scans a local root directory (one subfolder per plugin with
  `plugin.toml`).
- `plugins.export` renders a pin-map template from the open design; an optional
  `directory` writes the file without changing the project.

There is no network marketplace in schema 1. Discovery is a folder the user or
operator approves; optional `content_hash` on the manifest supports tamper
checks after copy.

## Comparison with existing data packs

| Mechanism | Location | Role |
| --- | --- | --- |
| Interface profiles | `crates/engine/profiles` | Built-in TOML rule packs applied by id. |
| Library sources | `crates/library/src/sources.rs` | Trait for part search; fixture JSON for tests. |
| Plugins | `crates/plugin` | Manifest registry plus export templates outside core. |

Rule sets inside a plugin reuse the rules import path rather than a separate
plugin runtime.

## Browser and desktop

Schema 1 commands run anywhere the engine runs. Listing and export only need
filesystem access to the plugin root and optional output directory. Browser
builds should treat the plugin root as a user-selected folder; wiring UI and
persistence is follow-up work.

## Recommended follow-up issues

1. **Firmware target plugin**: move the flight-controller target generator from
   the core split (see the dedicated firmware plugin issue) into the first code
   plugin folder using schema 1 bundles.
2. **Rule set import command**: explicit import from a plugin `rule_set_bundles`
   path with licence display and approval.
3. **Importer descriptors**: data-only import formats registered in the manifest,
   executed through existing io parsers.
4. **Plugin approval UX**: desktop and browser surfaces to pick a root, show
   licence and hash, and enable templates.
5. **Golden harness CLI**: `xtask` or script that runs every export template in
   a plugin folder against fixture projects (extends the crate test pattern).
6. **WebAssembly phase**: sandbox spec only if a template cannot stay data-only.

## Tests

- `crates/plugin/tests/golden_export.rs` loads the example plugin and compares
  output to `expected/pin-map.tsv`.
- `crates/engine/tests/plugins.rs` exercises `plugins.list` and `plugins.export`.
