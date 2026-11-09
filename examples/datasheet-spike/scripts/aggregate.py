#!/usr/bin/env python3
"""Aggregate results.jsonl from run-corpus.py into the numbers quoted in the note (run with -I).

Usage: aggregate.py <results.jsonl> [oxide.jsonl]
A page is "suspect" when it differs from the reference by more than the thresholds below; suspects
are then looked at by eye and classified (see the note): the metric alone does not decide a fault.
"""
import json
import statistics as st
import sys

BAD_BLOCKS = 0.03   # fraction of 8x8 blocks whose mean grey differs by more than 40/255
INK_RATIO = 0.35    # relative difference in dark-pixel share

rows = [json.loads(l) for l in open(sys.argv[1])]
pages = [r for r in rows if r["kind"] == "page"]
docs = [r for r in rows if r["kind"] == "doc"]


def pct(a, b):
    return 100.0 * a / b if b else 0.0


def q(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))] if xs else 0


ok = [p for p in pages if p["ok"]]
print(f"documents: {len(docs)}, opened: {sum(1 for d in docs if d['open_ok'])}")
print("open failures:", [(d["tag"], d.get("err", "")) for d in docs if not d["open_ok"]])
print(f"pages tried: {len(pages)}, rendered: {len(ok)} ({pct(len(ok), len(pages)):.1f}%)")
print("render failures:", [(p["tag"], p["page"], p["err"]) for p in pages if not p["ok"]])
t = [p["render_ms"] for p in ok]
print(f"render ms at 150 dpi: median {st.median(t):.0f}, p90 {q(t, .9):.0f}, p99 {q(t, .99):.0f}, max {max(t):.0f}")
rss = [d["peak_rss_kb"] / 1024 for d in docs if d.get("open_ok")]
print(f"peak RSS per document (MB): median {st.median(rss):.0f}, max {max(rss):.0f}")
refd = [p for p in ok if p["have_ref"]]
sus = []
for p in refd:
    ink_rel = abs(p["ink_ours"] - p["ink_ref"]) / max(p["ink_ref"], 0.002)
    why = []
    if p["size_mismatch"]:
        why.append("size")
    if p["bad_blocks"] > BAD_BLOCKS:
        why.append("blocks")
    if ink_rel > INK_RATIO and abs(p["ink_ours"] - p["ink_ref"]) > 0.003:
        why.append("ink")
    if p["blank"] and p["ink_ref"] > 0.002:
        why.append("blank")
    if why:
        sus.append((p, why))
print(f"pages with reference: {len(refd)}, suspect by metric: {len(sus)} ({pct(len(sus), len(refd)):.1f}%)")
for p, why in sus:
    print(f"  {p['tag']} p{p['page']} {why} bad={p['bad_blocks']:.3f} ink {p['ink_ours']:.3f}/{p['ink_ref']:.3f}")
tx = [p for p in ok if p["ref_words"] > 0]
print(f"text: pages with reference text {len(tx)}; text layer ok on {sum(1 for p in ok if p['text_ok'])}/{len(ok)}")
cw = sum(p["words_found"] for p in tx)
rw = sum(p["ref_words"] for p in tx)
print(f"word coverage (distinct words of 3+ chars found in our layer): {pct(cw, rw):.1f}% ({cw}/{rw})")
ht = sum(p["hit_total"] for p in tx)
hf = sum(p["hit_found"] for p in tx)
print(f"part-number-like token search hit rate: {pct(hf, ht):.1f}% ({hf}/{ht})")
low = [(p["tag"], p["page"], p["words_found"], p["ref_words"]) for p in tx if p["ref_words"] >= 20 and p["words_found"] < 0.8 * p["ref_words"]]
print(f"pages with text coverage under 80% (20+ ref words): {len(low)}", low[:15])
empty = [(p["tag"], p["page"]) for p in ok if p["chars"] == 0 and p["ink_ours"] > 0.003]
print(f"inked pages with no text layer (image-only): {len(empty)}", empty[:15])
tms = [p["text_ms"] for p in ok]
print(f"text layer ms: median {st.median(tms):.0f}, p90 {q(tms, .9):.0f}, max {max(tms):.0f}")
if len(sys.argv) > 2:
    ox = [json.loads(l) for l in open(sys.argv[2])]
    op = [o for o in ox if o["kind"] == "oxide_page"]
    oo = [o for o in op if o["ok"] and o["ref_words"] > 0]
    ow = sum(o["words_found"] for o in oo)
    orw = sum(o["ref_words"] for o in oo)
    oh = sum(o["hit_found"] for o in oo)
    oht = sum(o["hit_total"] for o in oo)
    print(f"pdf_oxide: pages ok {sum(1 for o in op if o['ok'])}/{len(op)}; coverage {pct(ow, orw):.1f}% ({ow}/{orw}); hit rate {pct(oh, oht):.1f}% ({oh}/{oht}); ms median {st.median([o['ms'] for o in op if o['ok']]):.0f}")
