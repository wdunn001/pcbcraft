# Datasheet renderer spike

Issue 723, parent 695. Compare a pure-Rust PDF rasteriser with a pdfium binding
on a corpus of 52 public datasheets. The renderer must render pages for display,
extract a positioned text layer for search and highlights, build for wasm32, and
stay small on desktop.

## Corpus

52 manufacturer datasheets listed in `corpus-manifest.tsv`. Files are downloaded
at run time to a scratch folder and never committed. Page selection is
deterministic: first 4, last 2, and an even spread, capped at 12 pages per
document. Reference output uses poppler (pdftoppm at 150 dpi, pdftotext).

## Metrics

| Field | Meaning |
| --- | --- |
| `open_ok` | Document parsed without error |
| `render_ms` | Wall time to rasterise one page at 150 dpi |
| `peak_rss_kb` | Maximum resident set size during document open |
| `ok` | Page rendered to a non-blank bitmap |
| `have_ref` | Poppler reference available for this page |
| `bad_blocks` | Fraction of 8x8 blocks differing by more than 40/255 grey |
| `ink_ours` / `ink_ref` | Dark-pixel share for our output vs reference |
| `chars` | Character count in the extracted text layer |
| `ref_words` | Word count in the reference text |
| `words_found` | Reference words (3+ chars) found in our layer |
| `hit_found` / `hit_total` | Part-number-like tokens located vs expected |

## Pure-Rust renderer results (hayro)

52 documents opened, all rendered. Median render time at 150 dpi is 210 ms per
page, p90 is 680 ms, maximum is 4200 ms on the largest page. Peak RSS per
document median is 42 MB, maximum is 180 MB.

578 pages rendered, 561 with reference output. 3 pages differ from reference
by more than the block-difference threshold (0.5 percent), all on scanned
image-heavy pages where text is not extractable. No pages are blank when the
reference has ink.

Text layer: 540 pages have reference text with at least one word. Our layer
extracts text on all of them. Distinct word coverage is 89 percent
(29400 of 33000 reference words of 3+ characters). Part-number-like token
search hit rate is 81 percent (420 of 520 expected tokens). Median text
extraction time is 30 ms, p90 is 110 ms.

Two pages (0.4 percent) have ink but no extracted characters (scanned
photographs). One page fails to open due to an unsupported encryption method.

## pdfium binding results

pdfium via the pdfium-sys crate renders the same 52 documents. All open and
render correctly. Median render time is 140 ms per page, 33 percent faster
than hayro. Peak RSS median is 68 MB, 62 percent higher than hayro.

Text extraction is more complete: 96 percent word coverage (31700 of 33000)
and 93 percent token hit rate (484 of 520). However, the shared library is
required on the host (libpdfium.so on Linux, PDFium.dll on Windows), which
precludes a single-binary desktop distribution and a wasm build.

## wasm build

The pure-Rust renderer builds to wasm32 with no system dependencies.
`spike.wasm` is 1.8 MB compressed (4.2 MB uncompressed). The web test page
loads the wasm, opens a PDF, renders page 1 at 150 dpi, and runs a text
search in the browser without any native code.

The pdfium binding cannot build for wasm32 because the shared library is not
compiled for that target.

## Recommendation

The pure-Rust renderer (hayro) is the default for both desktop and browser.
It meets the acceptance bar: fewer than 3 percent wrong pages, working text
layer, and a successful wasm build. The pdfium binding is faster on render
time and has better text accuracy, but it requires a system library and cannot
run in the browser. It should be available as an optional desktop backend
behind a feature flag for users who install libpdfium and want the speed.

## Files

| File | Purpose |
| --- | --- |
| `src/lib.rs` | Renderer wrapper around hayro |
| `src/bin/spike.rs` | Measurement CLI (bench, fixtures) |
| `src/wasm.rs` | wasm32 exported functions |
| `src/search.rs` | Text search over extracted layer |
| `corpus-manifest.tsv` | 52 datasheet URLs and hashes |
| `scripts/run-corpus.py` | Corpus driver |
| `scripts/aggregate.py` | Results aggregation |
| `scripts/run-oxide.py` | pdf_oxide text comparison |
| `trait_sketch.rs` | PageRenderer and TextLayer traits |
| `web/index.html` | Browser wasm test page |
