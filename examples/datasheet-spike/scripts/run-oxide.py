#!/usr/bin/env python3
"""Run the pdf_oxide text comparison over the same pages as results.jsonl (run with -I).

Usage: run-oxide.py <scratch-dir> <oxide-text-binary>   -> <scratch-dir>/oxide.jsonl
"""
import json
import os
import subprocess
import sys

scratch, binary = sys.argv[1], sys.argv[2]
pages = {}
for line in open(os.path.join(scratch, "results.jsonl")):
    r = json.loads(line)
    if r["kind"] == "page":
        pages.setdefault(r["tag"], []).append(r["page"])
out = open(os.path.join(scratch, "oxide.jsonl"), "w")
for tag, ps in pages.items():
    cmd = [binary, os.path.join(scratch, "corpus", tag + ".pdf"), tag, os.path.join(scratch, "ref"),
           ",".join(map(str, ps))]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=900)
        for l in r.stdout.splitlines():
            if l.startswith("{"):
                out.write(l + "\n")
        if r.returncode != 0:
            out.write(json.dumps({"kind": "oxide_doc", "tag": tag, "open_ok": False,
                                  "err": f"exit {r.returncode}"}) + "\n")
    except subprocess.TimeoutExpired:
        out.write(json.dumps({"kind": "oxide_doc", "tag": tag, "open_ok": False, "err": "timeout"}) + "\n")
    out.flush()
