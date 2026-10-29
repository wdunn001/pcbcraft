"""Generates the golden values used by crates/geom/src/impedance.rs tests.

Run in a clean Python container (the build host used python:3.11-slim):
    pip install scikit-rf scipy numpy && python oracle.py
Single microstrip values come from scikit-rf MLine (hammerstadjensen, no
dispersion), an independent implementation. Coupled and stripline values are
checked against finite-difference field solves in check_fd.py and
strip_fd.py (Richardson extrapolated from two grids)."""
import json, math
import numpy as np
import skrf
from skrf import Frequency
from skrf.media import MLine
from scipy.optimize import brentq
import models as M

f = Frequency(1, 1, 1, "GHz")


def sk(w, h, t, er):
    m = MLine(frequency=f, w=w * 1e-3, h=h * 1e-3, t=t * 1e-3, ep_r=er, model="hammerstadjensen", disp="none")
    return float(m.z0_characteristic[0].real), float(m.ep_reff[0].real)


out = {"skrf": skrf.__version__, "single": [], "width": [], "coupled": [], "stripline": []}
worst = 0
for er, h, t in [(4.5, 1.6, 0.035), (4.4, 1.6, 0.035), (3.5, 0.8, 0.018), (9.8, 0.5, 0.035), (2.2, 0.254, 0.018)]:
    for u in (0.1, 0.25, 0.5, 1.0, 1.006, 2.0, 4.0, 8.0):
        w = u * h
        z, ee = sk(w, h, t, er)
        mz, mee = M.microstrip(w, h, t, er)
        worst = max(worst, abs(mz / z - 1), abs(mee / ee - 1))
        out["single"].append({"w": w, "h": h, "t": t, "er": er, "z": round(z, 4), "eeff": round(ee, 5)})
out["transcription_vs_skrf_worst_rel"] = worst
for target in (30, 40, 50, 60, 65, 70, 75, 90, 100):
    w = brentq(lambda x: sk(x, 1.6, 0.035, 4.5)[0] - target, 0.02, 40, xtol=1e-9)
    out["width"].append({"target": target, "h": 1.6, "t": 0.035, "er": 4.5, "w": round(w, 4)})
for er, h, w, s in [(4.5, 1.6, 0.8, 0.8), (4.5, 1.6, 1.6, 0.4), (3.5, 0.8, 0.64, 0.4), (4.5, 0.2, 0.15, 0.15)]:
    ze, zo, ee, eo = M.coupled(w, s, h, er)
    out["coupled"].append({"w": w, "s": s, "h": h, "er": er, "ze": round(ze, 4), "zo": round(zo, 4), "ee_e": round(ee, 5), "ee_o": round(eo, 5)})
for w, b, t, er in [(0.14, 1.68, 0.035, 4.5), (0.28, 1.68, 0.035, 4.5), (0.56, 1.68, 0.035, 4.5), (1.12, 1.68, 0.035, 4.5), (0.2, 0.5, 0.018, 3.6)]:
    out["stripline"].append({"w": w, "b": b, "t": t, "er": er, "z": round(M.stripline(w, b, t, er), 4)})
print(json.dumps(out, indent=1))
