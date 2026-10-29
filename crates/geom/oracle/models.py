"""Plain transcriptions of the closed forms implemented in crates/geom/src/impedance.rs.
Lengths in millimetres."""
import math

ZF0 = 376.73031366857


def zl1(u):
    f = 6 + (2 * math.pi - 6) * math.exp(-((30.666 / u) ** 0.7528))
    return ZF0 / (2 * math.pi) * math.log(f / u + math.sqrt(1 + (2 / u) ** 2))


def b_er(er):
    return 0.564 * ((er - 0.9) / (er + 3)) ** 0.053


def a_u(u):
    return (
        1
        + math.log((u**4 + (u / 52) ** 2) / (u**4 + 0.432)) / 49
        + math.log(1 + (u / 18.1) ** 3) / 18.7
    )


def eeff_u(u, er):
    return (er + 1) / 2 + (er - 1) / 2 * (1 + 10 / u) ** (-a_u(u) * b_er(er))


def microstrip(w, h, t, er):
    u = w / h
    tn = t / h
    if tn > 0:
        du1 = tn / math.pi * math.log(1 + 4 * math.e / tn * math.tanh(math.sqrt(6.517 * u)) ** 2)
        dur = du1 * (1 + 1 / math.cosh(math.sqrt(er - 1))) / 2
    else:
        du1 = dur = 0.0
    u1, ur = u + du1, u + dur
    ee = eeff_u(ur, er)
    z = zl1(ur) / math.sqrt(ee)
    eet = ee * (zl1(u1) / zl1(ur)) ** 2
    return z, eet


def microstrip_nothick(w, h, er):
    u = w / h
    return zl1(u) / math.sqrt(eeff_u(u, er)), eeff_u(u, er)


def coupled(w, s, h, er):
    """Hammerstad and Jensen even and odd mode, zero thickness.
    Returns (z_even, z_odd, ee_even, ee_odd)."""
    u = w / h
    g = s / h
    ee0 = eeff_u(u, er)
    z0 = zl1(u) / math.sqrt(ee0)
    v = u * (20 + g * g) / (10 + g * g) + g * math.exp(-g)
    ae = (
        1
        + math.log((v**4 + (v / 52) ** 2) / (v**4 + 0.432)) / 49
        + math.log(1 + (v / 18.1) ** 3) / 18.7
    )
    ee_e = (er + 1) / 2 + (er - 1) / 2 * (1 + 10 / v) ** (-ae * b_er(er))
    q1 = 0.8695 * u**0.194
    q2 = 1 + 0.7519 * g + 0.189 * g**2.31
    q3 = 0.1975 + (16.6 + (8.4 / g) ** 6) ** (-0.387) + math.log(g**10 / (1 + (g / 3.4) ** 10)) / 241
    q4 = 2 * q1 / (q2 * (math.exp(-g) * u**q3 + (2 - math.exp(-g)) * u ** (-q3)))
    ze = z0 * math.sqrt(ee0 / ee_e) / (1 - z0 / ZF0 * math.sqrt(ee0) * q4)
    ao = 0.7287 * (ee0 - (er + 1) / 2) * (1 - math.exp(-0.179 * u))
    bo = 0.747 * er / (0.15 + er)
    co = bo - (bo - 0.207) * math.exp(-0.414 * u)
    do = 0.593 + 0.694 * math.exp(-0.562 * u)
    ee_o = ((er + 1) / 2 + ao - ee0) * math.exp(-co * g**do) + ee0
    q5 = 1.794 + 1.14 * math.log(1 + 0.638 / (g + 0.517 * g**2.43))
    q6 = 0.2305 + math.log(g**10 / (1 + (g / 5.8) ** 10)) / 281.3 + math.log(1 + 0.598 * g**1.154) / 5.1
    q7 = (10 + 190 * g * g) / (1 + 82.3 * g**3)
    q8 = math.exp(-6.5 - 0.95 * math.log(g) - (g / 0.15) ** 5)
    q9 = math.log(q7) * (q8 + 1 / 16.5)
    q10 = (q2 * q4 - q5 * math.exp(math.log(u) * q6 * u ** (-q9))) / q2
    zo = z0 * math.sqrt(ee0 / ee_o) / (1 - z0 / ZF0 * math.sqrt(ee0) * q10)
    return ze, zo, ee_e, ee_o


def _ellipk(m):
    # complete elliptic integral of the first kind, parameter m = k^2, by AGM
    a, b = 1.0, math.sqrt(1 - m)
    while abs(a - b) > 1e-15:
        a, b = (a + b) / 2, math.sqrt(a * b)
    return math.pi / (2 * a)


def stripline_cohn(w, b, er):
    """Zero thickness strip centred between two planes b apart."""
    k = 1 / math.cosh(math.pi * w / (2 * b))
    return 30 * math.pi / math.sqrt(er) * _ellipk(k * k) / _ellipk(1 - k * k)


def stripline(w, b, t, er):
    """Symmetric stripline: exact zero thickness strip (Cohn) widened by the
    isolated strip thickness allowance t/pi (1 + ln(4 pi w / t))."""
    dw = t / math.pi * (1 + math.log(4 * math.pi * w / t)) if t > 0 else 0.0
    return stripline_cohn(w + dw, b, er)
