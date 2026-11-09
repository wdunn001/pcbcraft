import sys, math
import numpy as np
sys.path.insert(0, ".")
from fd import capacitance, C0, EPS0
import models as M


def microstrip_fd(w, h, er, s=None, dx=None):
    dx = dx or h / 16
    X = 12 * h
    Y = 12 * h
    wn = round(w / dx)
    hn = round(h / dx)
    ny = round(Y / dx) + 1
    half = round(X / dx)
    if s is None:
        nx = 2 * half + 1
        cx = half
        strips = [(cx - wn // 2, cx - wn // 2 + wn)]
    else:
        sn = round(s / dx)
        total = 2 * wn + sn
        nx = total + 2 * half + 1
        a0 = half
        strips = [(a0, a0 + wn), (a0 + wn + sn, a0 + 2 * wn + sn)]
    masks = []
    for a, b in strips:
        m = np.zeros((nx, ny), bool)
        m[a : b + 1, hn] = True
        masks.append(m)

    def run(eps_sub, volts, which=0):
        epsc = np.ones((nx - 1, ny - 1))
        epsc[:, :hn] = eps_sub
        q = capacitance(nx, ny, epsc, masks, volts)
        return q[which]

    if s is None:
        q = run(er, [1.0])
        q0 = run(1.0, [1.0])
        return 1 / (C0 * EPS0 * math.sqrt(q * q0)), q / q0
    qe = run(er, [1.0, 1.0])
    qe0 = run(1.0, [1.0, 1.0])
    qo = run(er, [1.0, -1.0])
    qo0 = run(1.0, [1.0, -1.0])
    # per strip capacitance; mode impedance 1/(c sqrt(C C0))
    return (
        1 / (C0 * EPS0 * math.sqrt(qe * qe0)),
        1 / (C0 * EPS0 * math.sqrt(qo * qo0)),
        qe / qe0,
        qo / qo0,
    )


if __name__ == "__main__":
    which = sys.argv[1]
    if which == "single":
        for er, h, wh in [(4.5, 1.6, 0.5), (4.5, 1.6, 1.0), (4.5, 1.6, 2.0), (3.5, 0.8, 1.5), (9.8, 0.5, 1.0)]:
            w = wh * h
            z, ee = microstrip_fd(w, h, er)
            zm, eem = M.microstrip_nothick(w, h, er)
            print(f"er={er} h={h} w/h={wh}: FD z={z:.2f} ee={ee:.3f} | HJ z={zm:.2f} ee={eem:.3f} | dz={100*(zm/z-1):+.2f}%")
    if which == "coupled":
        for er, h, wh, gh in [(4.5, 1.6, 0.5, 0.5), (4.5, 1.6, 0.5, 1.0), (4.5, 1.6, 1.0, 0.25), (4.5, 1.6, 1.0, 1.0), (3.5, 0.8, 0.8, 0.5), (4.5,1.6,0.3,2.0)]:
            w, s = wh * h, gh * h
            r = microstrip_fd(w, h, er, s)
            m = M.coupled(w, s, h, er)
            print(f"er={er} w/h={wh} s/h={gh}: FD ze={r[0]:.2f} zo={r[1]:.2f} | HJ ze={m[0]:.2f} zo={m[1]:.2f} | dze={100*(m[0]/r[0]-1):+.2f}% dzo={100*(m[1]/r[1]-1):+.2f}%  eeE {r[2]:.3f}/{m[2]:.3f} eeO {r[3]:.3f}/{m[3]:.3f}")
