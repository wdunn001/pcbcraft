# Publication privacy and safety scan

Shared checks for publication uploads live in `crates/publication`. The same
library runs in the desktop app before upload, in CI for a static registry, and
in the portal service.

## Hard fails

The privacy scan reports exact file and line for:

- PEM or OpenSSH private keys
- Common service token prefixes
- Passwords embedded in URLs
- Email addresses outside the allowed author list
- Absolute paths with user or home directory names
- Local host names, loopback names and private IPv4 addresses
- Non-empty project credential fields in JSON text
- Exif metadata left in preview images
- Executable or script file extensions in the bundle
- Copied datasheet PDF bytes without explicit permission
- Licence or source combinations that contradict the chosen publication licence

Each hard fail includes a `remove_and_retry` action.

## Licence table

Part licence pairs against the publication licence are data in
`crates/publication/data/licence_compat.toml`. Unknown pairs go to the human
queue. Sources on the restricted list in
`crates/publication/data/restricted_sources.toml` are excluded from public
offering.

## Engine command

`publication.scan` accepts the upload payload as JSON and returns `ok_to_upload`,
`hard_fails`, `human_queue`, and `notes`. It does not change the open project.

## Compliance flags

Publisher compliance claims are returned as notes and are never marked certified.
Designs flagged for mains, battery or high-voltage use receive a not-reviewed
safety note. Export review requests go to the human queue.
