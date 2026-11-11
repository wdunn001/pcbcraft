# Plugin System Design

## Overview

PCBCraft uses a data-only plugin architecture for export templates, firmware target
generators, rule sets, and importers. Plugins are TOML manifests with embedded
templates, loaded at runtime without arbitrary code execution.

## Architecture

```
crates/plugin/          # Core plugin infrastructure
crates/plugin-sdk/      # Shared types and traits for plugin developers
crates/plugin-firmware-target/  # Firmware target plugin; the generator itself is a follow-up (see docs/plugins.md)
```

## Plugin Manifest Schema (v1)

Plugins are TOML files with this structure:

```toml
schema = 1
id = "example-export"
name = "Example Export"
version = "0.1.0"
license = "GPL-3.0-or-later"

[[export_templates]]
id = "pin-map-tsv"
path = "templates/pin-map.tsv.toml"
description = "Tab-separated pin map"
output_name = "pin-map.tsv"
```

## Export Templates

Export templates are data-only TOML files that define how to format project data:

```toml
schema = 1
kind = "pin_map"
header = "net\treference\tpin\n"
row = "{net}\t{reference}\t{pin}\n"
sort = "net_then_reference"
```

Template variables: {net}, {reference}, {pin}, {value}, {kind}, etc.

## Plugin Discovery

Plugins are discovered from a local root directory, one subfolder per plugin
with a `plugin.toml`. The user guide in `docs/plugins.md` is the reference for
the current behavior.

## Engine Commands

- plugins.list - List all loaded plugins and their capabilities
- plugins.export - Run an export template and return the output

## Security

- Data-only templates: no code execution
- TOML parsing with schema validation
- No file/network access by default
