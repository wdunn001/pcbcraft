# Research: sharing portal for designs and parts

Issue: #984 (child of #598, relates to #813)

## Scope

A self-hosted sharing portal in the style of an open-source library manager for 3D
print files: shareable items with creators, collections, tags, licences, previews,
versions, and optional federation between instances. PCBCraft users publish and
browse (a) complete designs (project bundles) and (b) individual ICs and parts
(symbol, footprint, 3D model, parameters, datasheet links).

Decided constraints: the owner runs and moderates the service (rules decide, a
small local model only advises); the service is never the only path (a public
static sources.json index and any URL or folder also work); service code and API
live in this repository, deployment config stays outside; storage is SQL pointers
plus S3-compatible object storage, content-addressed; distributor data is not
mirrored; datasheets are not copied without terms that allow it.

## Findings

### 1. Reference project: open-source library manager

An open-source library manager for 3D print files (AGPL-3.0, Ruby on Rails,
PostgreSQL, Docker). Data model: models with files, creators, collections, tags,
links, an optional SPDX licence, a sensitive flag, a preview file and a
related-models relation. A "problems" list flags missing licence, creator, tags,
links and images.

Federation is ActivityPub (server to server, WebFinger, NodeInfo, HTTP
signatures): models, creators and collections are actors; Create, Update, Delete,
Follow and Flag flow; file bytes are not federated, only metadata and links.

Moderation is a report button on every object that emails moderators plus an
admin list and domain blocks; roles are administrator, moderator, contributor,
member; sign-in by password or OpenID Connect, API clients by OAuth.

Lessons: licence, creator and source-link hygiene checks; metadata-only
federation; a report on every object. Gaps to do better: a required licence and
a real version and lineage model.

### 2. Cannot adopt the reference project

AGPL code cannot be merged into an MIT OR Apache-2.0 repository, and its model
is one set of mesh files, not a design with parts, BOM and layered licences.
Borrow ideas, not code.

### 3. Comparable sites and practices

Open hardware certification asks for a chosen licence per layer (hardware,
software, documentation), links to all documentation, shareable datasheets for
third-party parts and a code per version. Language registries make versions
immutable and use a "yank" flag, with removal only for legal or abuse reasons.
A model hub uses a card with typed lineage (base_model with a relation) and a
new_version pointer. Code hosts run a written copyright notice and counter
notice process.

### 4. Recommended metadata

One schema-versioned publication.json per publication with identity and
authorship, a required licence choice per layer (SPDX allowlist, no default),
version and changelog, derived_from lineage with relation and parent licence,
controlled tags and roles, BOM and sourcing summary (no prices or stock),
optional declared compliance claims, previews, file manifest with sha256, size
and format versions, and provenance with a publisher signature. Most fields are
rule-checkable at upload (child issue 1); licence truth, copying and datasheet
agreement stay with the human queue and the advisory model.

### 5. Architecture options

(a) A Rust service and web portal in the repo is the target.
(b) Forking an existing portal is not recommended (AGPL, wrong model).
(c) A static git-hosted registry with a generated site is the cheapest and fits
sources.json exactly.
(d) A pull-based index of other instances is recommended over ActivityPub, which
costs about ten times more.

Recommended order: static format and registry first, then the service, then the
pull-based index, ActivityPub only if requested.

### 6. First slice

The publication manifest schema, a bundle writer and reader in the library
crate, the shared Stage 0 validator, a static registry layout with a generated
catalogue, and Publish to folder and Install from source in the app. No
accounts, no service.

### 7. In-app flows

Publish (bundle contents, what is stripped, local checks, preview generation,
signing), Browse and Install from the Library tab, update and yanked badges,
Report. See child issue 4.

### 8. Identity and moderation

Email link or passkey plus optional OpenID Connect, anonymous download, trust
tiers, rate limits, a written takedown and counter-notice process, report
states, domain-proof publisher verification.

### 9. Privacy and safety

Hard-fail scan for keys, personal data and local paths in bundles and previews,
licence compatibility table for bundled parts, no copied datasheets, declared
(never certified) compliance and export flags.

### 10. Costs

Typical design 5 to 20 MB, typical part about 1 MB. 100 GB covers a large
community. Immutable hash-addressed reads cache forever, so one small server and
a reverse proxy cache are enough, no CDN (estimates, low confidence).

### 11. Vendor integrations

A separate, optional module, off by default. A compile-time feature plus a
per-instance setting plus an app setting that hides every such control when off.
Build only the vendor quote handoff first (open the vendor page with a prepared
archive, or link, no keys); do not build a marketplace now. See child issue 6.

## Children

1. #985 Publication manifest schema and Stage 0 rule checks
2. #987 Static registry, generated catalogue and pull-based instance index
3. #988 Portal service: API, storage, accounts, moderation queue and web portal
4. #989 In-app flows: Publish, Browse and Install, update badges, Report
5. #990 Privacy and safety scan, licence compatibility and datasheet rules
6. #992 Optional vendor quote handoff and marketplace module

## Owner decisions needed

1. Slice order: static registry and format first, or the service first.
2. Allowed licence list (SPDX ids) and the policy for view-only designs.
3. Open licences only, or view-only items too.
4. Sign-in options for the first release.
5. Publisher key and signing scheme, and who verifies publishers.
6. Takedown and copyright policy text, appeals window.
7. Federation: pull-based instance index now, ActivityPub later or never.
8. Vendor and marketplace: confirm both off by default behind one switch.
9. Export-control and restricted-content stance.
10. Storage quotas and the object-store driver for small installs.

## Not verified

Version history in the reference project; policy pages of model-sharing sites;
open hardware project hosts; vendor API terms and payloads; tax rules; capacity
numbers are estimates.

## Sources

- Reference project repository and documentation (read directly)
- Open hardware certification requirements (public documentation)
- Language registry versioning practices (public documentation)
- Model hub lineage model (public documentation)
- Code host copyright policy (public documentation)
- SPDX licence list (public)
