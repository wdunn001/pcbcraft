import sys, math
import numpy as np
sys.path.insert(0, ".")
from fd import capacitance, C0, EPS0
import models as M


def stripline_fd(w, b, t, er, dx):
    X = 6 * b
    half = round(X / dx)
    wn = round(w / dx)
    bn = round(b / dx)
    tn = max(round(t / dx), 0)
    nx = 2 * half + 1
    ny = bn + 1
    j0 = bn // 2 - tn // 2
    m = np.zeros((nx, ny), bool)
    i0 = half - wn // 2
    m[i0 : i0 + wn + 1, j0 : j0 + tn + 1] = True
    epsc = np.ones((nx - 1, ny - 1))
    q = capacitance(nx, ny, epsc, [m], [1.0])[0]
    return 1 / (C0 * EPS0 * q * math.sqrt(er))


def cohn_thick(w, b, t, er):
    r = t / b
    d = (2 / math.pi) * math.log(1 / (1 - r) + 1)
    if r > 0:
        d -= (r / math.pi) * math.log(1 / (1 - r) ** 2 - 1)
    return 94.15 / (math.sqrt(er) * (w / (b - t) + d))


if __name__ == "__main__":
    b, er = 1.68, 4.5
    for t in (0.0, 0.035):
        for w in (0.14, 0.28, 0.56, 1.12, 2.24):
            fine = stripline_fd(w, b, t, er, 0.0175 if t else b / 96)
            coarse = stripline_fd(w, b, t, er, 0.035 if t else b / 48)
            ext = fine + (fine - coarse)
            print(f"t={t} w/b={w/b:.3f}: FD fine {fine:.2f} coarse {coarse:.2f} ext {ext:.2f} | cohn_zero {M.stripline_cohn(w,b,er):.2f} cohn_thick {cohn_thick(w,b,t,er):.2f} ({100*(cohn_thick(w,b,t,er)/ext-1):+.2f}%)", flush=True)
