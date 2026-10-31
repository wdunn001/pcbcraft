#!/usr/bin/env python3
"""Corpus driver for the datasheet renderer spike (#723).

Context: reads a manifest (url, sha256, bytes, file) of datasheets downloaded to a scratch folder
(never committed), picks up to N pages per document, makes reference output with poppler
(pdftoppm PPM at 150 dpi, pdftotext), then runs `spike bench` per document under a timeout and
collects the JSON lines.

Usage: run-corpus.py <scratch-dir> <spike-binary> [max-pages-per-doc]
Run with `python3 -I`. Output: <scratch-dir>/results.jsonl
"""
import json
import os
import subprocess
import sys


def page_count(pdf):
    out = subprocess.run(["pdfinfo", pdf], capture_output=True, text=True, timeout=60).stdout
    for line in out.splitlines():
        if line.startswith("Pages:"):
            return int(line.split()[1])
    return 0


def pick(n, cap):
    """First 4, last 2 and an even spread: deterministic."""
    if n <= cap:
        return list(range(1, n + 1))
    s = set(range(1, min(4, n) + 1)) | {n - 1, n}
    step = max(1, n // (cap - len(s) + 1))
    p = 5
    while len(s) < cap and p < n:
        s.add(p)
        p += step
    return sorted(s)


def main():
    scratch, spike = sys.argv[1], sys.argv[2]
    cap = int(sys.argv[3]) if len(sys.argv) > 3 else 12
    refdir = os.path.join(scratch, "ref")
    os.makedirs(refdir, exist_ok=True)
    rows = [l.rstrip("\n").split("\t") for l in open(os.path.join(scratch, "manifest_all.tsv"))]
    out = open(os.path.join(scratch, "results.jsonl"), "w")
    for url, sha, size, fname in rows:
        tag = fname[:-4]
        pdf = os.path.join(scratch, "corpus", fname)
        try:
            n = page_count(pdf)
        except Exception as e:  # reference tool failed: still try ours
            n = 0
        pages = pick(n, cap) if n else [1]
        for p in pages:
            base = os.path.join(refdir, f"{tag}-p{p}")
            try:
                with open(base + ".ppm", "wb") as f:
                    subprocess.run(["pdftoppm", "-cropbox", "-r", "150", "-f", str(p), "-l", str(p), pdf],
                                   stdout=f, stderr=subprocess.DEVNULL, timeout=120)
                subprocess.run(["pdftotext", "-f", str(p), "-l", str(p), pdf, base + ".txt"],
                               stderr=subprocess.DEVNULL, timeout=120)
            except Exception:
                pass
        cmd = [spike, "bench", pdf, "--pages", ",".join(map(str, pages)), "--ref", refdir,
               "--tag", tag, "--dpi", "150", "--png", os.path.join(scratch, "png")]
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=900)
            lines = [l for l in r.stdout.splitlines() if l.startswith("{")]
            if r.returncode != 0 and not any('"kind":"doc"' in l for l in lines):
                lines.append(json.dumps({"kind": "doc", "tag": tag, "open_ok": False,
                                         "err": f"exit {r.returncode}: {r.stderr[-200:]}"}))
        except subprocess.TimeoutExpired:
            lines = [json.dumps({"kind": "doc", "tag": tag, "open_ok": False, "err": "timeout 900s"})]
        for l in lines:
            out.write(l + "\n")
        out.flush()
        print(tag, url, len(lines), flush=True)


main()
