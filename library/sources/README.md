# Library sources index

`sources.json` is a plain list of known part and library sources. It is a directory, not a gatekeeper: PCBCraft can also use any URL or folder you add yourself. Nobody has to rely on a server of ours.

- **Why it lives here.** The list is versioned with the code, so every release ships a snapshot and works offline. The app also caches the newest copy it has seen.
- **What a source is.** A static file tree with a signed `manifest.json` (source id, name, licence, maintainer, update date, files with SHA-256 hashes) and content-addressed part files. It can sit on any static host, in a git repository, in a downloadable pack or in a local folder. No code runs on the host.
- **Licences.** Each source keeps its own licence and terms. The index never bundles or mirrors another source's data. Every card in the app shows its source and licence.

## Add a source

Open a pull request that adds one entry to `sources.json`. It must validate against `sources.schema.json` and have:

1. a stable `id` (lowercase letters, digits and dashes),
2. a reachable `url` (and optional `mirrors`),
3. a declared `licence`, and terms that allow what the source does,
4. a `maintainer`,
5. `trust` set to `unreviewed` (a maintainer raises it to `reviewed` after checking).

Sources of data that forbids copying or automated access are not listed.

The plan for the format, the checks and the optional moderated service is in fork issue #813.
