# Optional vendor quote handoff module

Child of the sharing portal research (#984). Relates to #992.

Vendor integrations (send manufacturing outputs for a quote or open a fabricator page with a prepared archive) and a possible marketplace are a **separate module**, off by default, not part of the core sharing portal.

This slice is **schema and flags only**: no live vendor network calls, no payment code, and no marketplace checkout.

## Switches

| Layer | Switch | Default |
| --- | --- | --- |
| Build | Cargo feature `vendor-handoff` on `pcbcraft-engine` | off |
| Instance | `VendorHandoffModuleSettings.enabled` | off |
| Instance | `vendor_quotes.enabled` | off |
| Instance | `marketplace.enabled` | off (reserved, not implemented) |
| App | `vendor_quotes.show_in_app` | off |

When the master switch is off, engine commands are not registered (default builds) or return empty adapter lists and capability JSON without `vendor_quotes` fields. Sub-switches require the master switch.

## Adapter modes

| Mode | Meaning |
| --- | --- |
| `documented_api` | Reserved for a future adapter backed by a published API and operator-issued credentials |
| `upload_flow` | Open a vendor page with a prepared archive; the user finishes in the browser |
| `link_only` | Link a publication to a vendor project page without uploading from PCBCraft |

Recommendation from research: ship upload-flow and link-only first; add API mode only when a vendor publishes terms and issues keys.

## Quote manifest

`VendorQuoteHandoffManifest` lists every file (path, size, sha256, optional role), the adapter id and mode, optional host label, stackup notes and finish options. Default policy: manufacturing outputs only, never project source or library parts. Vendor tokens stay in the user credential store or operator server config, never in bundles or logs.

## Engine commands (feature `vendor-handoff`)

| Command | Purpose |
| --- | --- |
| `vendor_handoff.capabilities` | Instance capability fragment for the app |
| `vendor_handoff.settings.validate` | Validate settings JSON |
| `vendor_handoff.quote.validate` | Validate a quote manifest |
| `vendor_handoff.adapters.list` | Built-in adapter descriptors when quotes are enabled |

Types live in `crates/doc/src/vendor_handoff.rs`. A fixture upload adapter (`fixture-upload`), manifest, prepared archive (`crates/doc/tests/fixtures/vendor-quote-handoff/fixture-upload.zip`), and `VendorQuoteConfirmDialog` support dialog and upload-flow tests without network access.

## Marketplace

Not built. The settings schema reserves `marketplace.enabled` so a future service can sit behind the same master switch after legal review. Paid designs and processor checkout are out of scope for this slice.
