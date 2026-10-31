#!/bin/bash
# Records the answers the open library tests replay. Run it by hand, rarely:
# it sends about twenty anonymous requests. CI never runs it.
# usage: record.sh   (writes files and index.json into this folder)
# index.json maps each request URL to a file here; the tests serve from it.
set -eu
cd "$(dirname "$0")"
SYM=45efbd93be87e365a21d743daf9b8a07e6414df6
FPR=a337146e090931ad1176e20ed0b7100990fac5fa
M3D=8975715167d1a76f9b44be155ac02326f704cdbb
API=https://gitlab.com/api/v4/projects
python3 - "$SYM" "$FPR" "$M3D" "$API" <<'PY'
import json, sys, urllib.parse, urllib.request, time
sym, fpr, m3d, api = sys.argv[1:5]
def enc(s): return urllib.parse.quote(s, safe="-_.~")
repos = {
  "symbols": ("kicad/libraries/kicad-symbols", sym),
  "footprints": ("kicad/libraries/kicad-footprints", fpr),
  "models": ("kicad/libraries/kicad-packages3D", m3d),
}
index, files = {}, {}
def get(url):
    time.sleep(0.4)
    req = urllib.request.Request(url, headers={"User-Agent": "pcbcraft-fixture-recorder"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read()
def save(name, url, data):
    open(name, "wb").write(data)
    index[url] = name
for key, (proj, rev) in repos.items():
    base = f"{api}/{enc(proj)}"
    # Keep only the fields the app reads; the full answer names individual authors.
    full = json.loads(get(f"{base}/repository/commits/{rev}"))
    slim = json.dumps({k: full[k] for k in ("id", "short_id", "created_at")}).encode()
    save(f"{key}-commit.json", f"{base}/repository/commits/{rev}", slim)
    # The app asks for the moving branch name; replay it as this pinned commit.
    index[f"{base}/repository/commits/master"] = f"{key}-commit.json"
    page = 1
    while True:
        url = f"{base}/repository/tree?ref={rev}&per_page=100&page={page}"
        data = get(url)
        save(f"{key}-root-{page}.json", url, data)
        if len(json.loads(data)) < 100:
            break
        page += 1
# Trim a library folder listing to a few real entries so the fixture stays small.
def tree(key, folder, keep, fname):
    proj, rev = repos[key]
    base = f"{api}/{enc(proj)}"
    url = f"{base}/repository/tree?path={enc(folder)}&ref={rev}&per_page=100&page=1"
    items, page = [], 1
    while True:
        got = json.loads(get(f"{base}/repository/tree?path={enc(folder)}&ref={rev}&per_page=100&page={page}"))
        items += got
        if len(got) < 100:
            break
        page += 1
    if keep:
        items = [i for i in items if i["name"] in keep]
    save(fname, url, json.dumps(items, indent=1).encode())
def raw(key, path, fname):
    proj, rev = repos[key]
    base = f"{api}/{enc(proj)}"
    url = f"{base}/repository/files/{enc(path)}/raw?ref={rev}"
    save(fname, url, get(url))
tree("symbols", "Device.kicad_symdir", {"R.kicad_sym","R_Small.kicad_sym","C.kicad_sym","LED.kicad_sym","L.kicad_sym","D.kicad_sym"}, "symbols-Device.json")
tree("symbols", "Connector_Generic.kicad_symdir", {"Conn_01x02.kicad_sym","Conn_01x04.kicad_sym"}, "symbols-Connector_Generic.json")
tree("footprints", "Resistor_SMD.pretty", None, "footprints-Resistor_SMD.json")
# The package family folder, trimmed to the 64-pin quad flat packages and relatives.
tree("footprints", "Package_QFP.pretty", None, "footprints-Package_QFP.json")
_trim = json.load(open("footprints-Package_QFP.json"))
json.dump([i for i in _trim if i["name"].startswith(("LQFP-", "TQFP-64"))], open("footprints-Package_QFP.json", "w"), indent=1)
tree("models", "Resistor_SMD.3dshapes", None, "models-Resistor_SMD.json")
raw("symbols", "Device.kicad_symdir/R.kicad_sym", "Device-R.kicad_sym")
raw("symbols", "Device.kicad_symdir/C.kicad_sym", "Device-C.kicad_sym")
raw("footprints", "Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod", "R_0603_1608Metric.kicad_mod")
raw("footprints", "Package_QFP.pretty/LQFP-64_10x10mm_P0.5mm.kicad_mod", "LQFP-64_10x10mm_P0.5mm.kicad_mod")
raw("models", "Resistor_SMD.3dshapes/R_0603_1608Metric.step", "R_0603_1608Metric.step")
json.dump(index, open("index.json", "w"), indent=1, sort_keys=True)
PY
