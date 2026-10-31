# PCBCraft research knowledge base

Internal reference copies of manufacturer-published documents used to enrich
library part records (#218). PDFs stay in this tree for embedding and review;
the application stores URL and hash only and does not redistribute these files.

Layout:

- `sources/library/datasheets/<manufacturer>/<part>.pdf` stored PDFs
- `sources/library/parts/<part>.toml` extracted facts with page citations
- `sources/library/manifest.tsv` append-only manifest (one row per stored PDF)
- `sources/library/gaps.toml` parts whose manufacturer PDF could not be fetched

License on each row is as stated by the publisher (typically manufacturer
copyright, internal reference only). Only manufacturer-hosted downloads are used.

Harvest tooling for issue #326 lives under `tools/`.
