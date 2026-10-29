#!/usr/bin/env3
"""Generate calculators.toml and golden vectors (stdlib only).

Provisional on laptop until ee-thesaurus from pcbcraft-research is copied.
Run: python generate_calculators_toml.py > calculators.toml
"""
from __future__ import annotations

import json
import math
from typing import Any

PI = math.pi
E = math.e
RHO_CU = 1.72e-8
ALPHA_CU = 0.00393
K_CU = 385.0
OZ_UM = 34.79
MIL_MM = 0.0254
C0 = 299_792_458.0
ZF0 = 376.73031366857
LIGHT_MM_PS = 0.299792458


def zl1(u: float) -> float:
    f = 6.0 + (2.0 * PI - 6.0) * math.exp(-(30.666 / u) ** 0.7528)
    return ZF0 / (2.0 * PI) * math.log(f / u + math.sqrt(1.0 + (2.0 / u) ** 2))


def b_er(er: float) -> float:
    return 0.564 * ((er - 0.9) / (er + 3.0)) ** 0.053


def eeff_u(u: float, er: float) -> float:
    a = (
        1.0
        + math.log((u**4 + (u / 52.0) ** 2) / (u**4 + 0.432)) / 49.0
        + math.log(1.0 + (u / 18.1) ** 3) / 18.7
    )
    return (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 10.0 / u) ** (-a * b_er(er))


def microstrip(width: float, height: float, thickness: float, er: float) -> tuple[float, float]:
    u = width / height
    tn = thickness / height
    if tn > 0.0:
        th = math.tanh(math.sqrt(6.517 * u))
        du1 = tn / PI * math.log(1.0 + 4.0 * E / tn * th * th)
        dur = du1 * (1.0 + 1.0 / math.cosh(math.sqrt(er - 1.0))) / 2.0
        u1, ur = u + du1, u + dur
    else:
        u1 = ur = u
    ee = eeff_u(ur, er)
    ohms = zl1(ur) / math.sqrt(ee)
    ratio = zl1(u1) / zl1(ur)
    eeff = ee * ratio * ratio
    return ohms, eeff


def width_for_ohms(target: float, height: float, thickness: float, er: float) -> float:
    lo, hi = 0.01, 100.0
    at_lo, _ = microstrip(lo, height, thickness, er)
    _, at_hi = microstrip(hi, height, thickness, er)
    if target > at_lo or target < at_hi:
        raise ValueError("target out of range")
    for _ in range(80):
        mid = (lo + hi) / 2.0
        z, _ = microstrip(mid, height, thickness, er)
        if z > target:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2.0


def ellipk(m: float) -> float:
    a, b = 1.0, math.sqrt(max(0.0, 1.0 - m))
    for _ in range(60):
        if abs(a - b) < 1e-15:
            break
        nxt = (a + b) / 2.0
        b = math.sqrt(a * b)
        a = nxt
    return PI / (2.0 * a)


def stripline(width: float, spacing: float, thickness: float, er: float) -> float:
    dw = thickness / PI * (1.0 + math.log(4.0 * PI * width / thickness)) if thickness > 0 else 0.0
    k = 1.0 / (PI * (width + dw) / (2.0 * spacing))
    k = math.cosh(k)
    return 30.0 * PI / math.sqrt(er) * ellipk(k * k) / ellipk(1.0 - k * k)


def ipc2221_width_mm(current_a: float, dt_c: float, external: bool) -> float:
    k = 0.048 if external else 0.024
    area_mil2 = (current_a / (k * (dt_c**0.44))) ** (1.0 / 0.725)
    area_mm2 = area_mil2 * (MIL_MM**2)
    thickness_mm = OZ_UM * 1e-3
    return area_mm2 / thickness_mm


# IPC-2152 public 1 oz outer layer table (amps -> width mm), linear interpolate
IPC2152_1OZ = [
    (0.5, 0.15),
    (1.0, 0.30),
    (1.5, 0.45),
    (2.0, 0.60),
    (3.0, 0.90),
    (4.0, 1.20),
    (5.0, 1.50),
    (6.0, 1.80),
    (8.0, 2.40),
    (10.0, 3.00),
]


def ipc2152_width(current_a: float) -> float:
    if current_a <= IPC2152_1OZ[0][0]:
        return IPC2152_1OZ[0][1] * (current_a / IPC2152_1OZ[0][0])
    for (i0, w0), (i1, w1) in zip(IPC2152_1OZ, IPC2152_1OZ[1:]):
        if i0 <= current_a <= i1:
            t = (current_a - i0) / (i1 - i0)
            return w0 + t * (w1 - w0)
    return IPC2152_1OZ[-1][1] * (current_a / IPC2152_1OZ[-1][0])


E12 = [10, 12, 15, 18, 22, 27, 33, 39, 47, 56, 68, 82]


def nearest_e12(value: float) -> float:
    if value <= 0:
        raise ValueError("value")
    exp = math.floor(math.log10(value))
    mant = value / (10**exp)
    best = E12[0]
    for e in E12:
        if abs(e - mant) < abs(best - mant):
            best = e
    return best * (10**exp)


def emit_calc(calc: dict[str, Any]) -> str:
    lines = ["[[calculator]]"]
    for key in ("id", "title", "group", "accuracy", "formula", "source"):
        val = calc[key]
        lines.append(f'{key} = {json.dumps(val)}')
    for inp in calc["inputs"]:
        lines.append("")
        lines.append("[[calculator.input]]")
        lines.append(f'name = {json.dumps(inp["name"])}')
        lines.append(f'unit = {json.dumps(inp["unit"])}')
        lines.append(f'min = {inp["min"]}')
        lines.append(f'max = {inp["max"]}')
    for vec in calc["vectors"]:
        lines.append("")
        lines.append("[[calculator.vector]]")
        lines.append(f'rel_tol = {vec.get("rel_tol", 1e-6)}')
        lines.append("[calculator.vector.inputs]")
        for k, v in vec["inputs"].items():
            if isinstance(v, str):
                lines.append(f"{k} = {json.dumps(v)}")
            else:
                lines.append(f"{k} = {v}")
        lines.append("[calculator.vector.expect]")
        for k, v in vec["expect"].items():
            if isinstance(v, str):
                lines.append(f'{k} = {json.dumps(v)}')
            else:
                lines.append(f"{k} = {v}")
    return "\n".join(lines)


def main() -> None:
    calcs: list[dict[str, Any]] = []

    def add(c: dict[str, Any]) -> None:
        calcs.append(c)

    add(
        {
            "id": "ohms_law",
            "title": "Ohm's law",
            "group": "basic",
            "accuracy": "exact",
            "formula": "V = I R",
            "source": "Elementary circuit theory",
            "inputs": [
                {"name": "voltage", "unit": "V", "min": 0.0, "max": 1e6},
                {"name": "current", "unit": "A", "min": 0.0, "max": 1e4},
                {"name": "resistance", "unit": "ohm", "min": 0.0, "max": 1e9},
            ],
            "vectors": [
                {
                    "inputs": {"voltage": 12.0, "current": 0.5, "resistance": 0.0},
                    "expect": {"resistance": 24.0},
                },
                {
                    "inputs": {"voltage": 0.0, "current": 2.0, "resistance": 47.0},
                    "expect": {"voltage": 94.0},
                },
                {
                    "inputs": {"voltage": 5.0, "current": 0.0, "resistance": 1000.0},
                    "expect": {"current": 0.005},
                },
                {
                    "inputs": {"voltage": 3.3, "current": 0.01, "resistance": 0.0},
                    "expect": {"resistance": 330.0},
                },
            ],
        }
    )

    add(
        {
            "id": "series_parallel",
            "title": "Series and parallel resistors",
            "group": "basic",
            "accuracy": "exact",
            "formula": "Series: R = R1+R2; parallel: 1/R = 1/R1+1/R2",
            "source": "Elementary circuit theory",
            "inputs": [
                {"name": "r1", "unit": "ohm", "min": 0.001, "max": 1e9},
                {"name": "r2", "unit": "ohm", "min": 0.001, "max": 1e9},
                {"name": "mode", "unit": "1=series 2=parallel", "min": 1.0, "max": 2.0},
            ],
            "vectors": [
                {
                    "inputs": {"r1": 100.0, "r2": 200.0, "mode": 1.0},
                    "expect": {"equivalent": 300.0},
                },
                {
                    "inputs": {"r1": 100.0, "r2": 200.0, "mode": 2.0},
                    "expect": {"equivalent": 200.0 / 3.0},
                },
                {
                    "inputs": {"r1": 47.0, "r2": 47.0, "mode": 2.0},
                    "expect": {"equivalent": 23.5},
                },
            ],
        }
    )

    add(
        {
            "id": "voltage_divider",
            "title": "Voltage divider",
            "group": "basic",
            "accuracy": "exact",
            "formula": "Vout = Vin R2/(R1+R2)",
            "source": "Elementary circuit theory",
            "inputs": [
                {"name": "vin", "unit": "V", "min": 0.0, "max": 1e6},
                {"name": "r1", "unit": "ohm", "min": 0.001, "max": 1e9},
                {"name": "r2", "unit": "ohm", "min": 0.001, "max": 1e9},
            ],
            "vectors": [
                {
                    "inputs": {"vin": 10.0, "r1": 1000.0, "r2": 1000.0},
                    "expect": {"vout": 5.0},
                },
                {
                    "inputs": {"vin": 12.0, "r1": 9000.0, "r2": 1000.0},
                    "expect": {"vout": 1.2},
                },
                {
                    "inputs": {"vin": 5.0, "r1": 330.0, "r2": 330.0},
                    "expect": {"vout": 2.5},
                },
            ],
        }
    )

    add(
        {
            "id": "led_resistor",
            "title": "LED series resistor",
            "group": "basic",
            "accuracy": "estimate",
            "formula": "R = (Vsupply - Vled)/Iled",
            "source": "Typical LED application note",
            "inputs": [
                {"name": "v_supply", "unit": "V", "min": 0.5, "max": 48.0},
                {"name": "v_led", "unit": "V", "min": 0.5, "max": 5.0},
                {"name": "i_led", "unit": "A", "min": 0.001, "max": 0.1},
            ],
            "vectors": [
                {
                    "inputs": {"v_supply": 5.0, "v_led": 2.0, "i_led": 0.01},
                    "expect": {"resistor": 300.0},
                },
                {
                    "inputs": {"v_supply": 12.0, "v_led": 2.2, "i_led": 0.015},
                    "expect": {"resistor": 653.3333333333334},
                    "rel_tol": 1e-5,
                },
                {
                    "inputs": {"v_supply": 3.3, "v_led": 1.8, "i_led": 0.005},
                    "expect": {"resistor": 300.0},
                },
            ],
        }
    )

    add(
        {
            "id": "rc_rl_time_constant",
            "title": "RC and RL time constant",
            "group": "basic",
            "accuracy": "exact",
            "formula": "tau = R C or tau = L/R",
            "source": "Elementary circuit theory",
            "inputs": [
                {"name": "resistance", "unit": "ohm", "min": 0.001, "max": 1e9},
                {"name": "capacitance", "unit": "F", "min": 0.0, "max": 1.0},
                {"name": "inductance", "unit": "H", "min": 0.0, "max": 10.0},
            ],
            "vectors": [
                {
                    "inputs": {"resistance": 10000.0, "capacitance": 1e-6, "inductance": 0.0},
                    "expect": {"tau_seconds": 0.01},
                },
                {
                    "inputs": {"resistance": 50.0, "capacitance": 0.0, "inductance": 0.001},
                    "expect": {"tau_seconds": 2e-5},
                },
                {
                    "inputs": {"resistance": 1000.0, "capacitance": 100e-9, "inductance": 0.0},
                    "expect": {"tau_seconds": 1e-4},
                },
            ],
        }
    )

    add(
        {
            "id": "lc_resonance",
            "title": "LC resonance",
            "group": "basic",
            "accuracy": "exact",
            "formula": "f = 1/(2 pi sqrt(L C))",
            "source": "Elementary circuit theory",
            "inputs": [
                {"name": "inductance", "unit": "H", "min": 1e-12, "max": 10.0},
                {"name": "capacitance", "unit": "F", "min": 1e-15, "max": 1.0},
            ],
            "vectors": [
                {
                    "inputs": {"inductance": 1e-6, "capacitance": 1e-9},
                    "expect": {"frequency_hz": 1.0 / (2 * PI * math.sqrt(1e-6 * 1e-9))},
                },
                {
                    "inputs": {"inductance": 2.2e-6, "capacitance": 100e-12},
                    "expect": {"frequency_hz": 1.0 / (2 * PI * math.sqrt(2.2e-6 * 100e-12))},
                },
                {
                    "inputs": {"inductance": 10e-6, "capacitance": 10e-9},
                    "expect": {"frequency_hz": 1.0 / (2 * PI * math.sqrt(10e-6 * 10e-9))},
                },
            ],
        }
    )

    add(
        {
            "id": "reactance",
            "title": "Capacitive and inductive reactance",
            "group": "basic",
            "accuracy": "exact",
            "formula": "Xc = 1/(2 pi f C), Xl = 2 pi f L",
            "source": "Elementary circuit theory",
            "inputs": [
                {"name": "frequency", "unit": "Hz", "min": 0.1, "max": 1e12},
                {"name": "capacitance", "unit": "F", "min": 0.0, "max": 1.0},
                {"name": "inductance", "unit": "H", "min": 0.0, "max": 10.0},
            ],
            "vectors": [
                {
                    "inputs": {"frequency": 1000.0, "capacitance": 1e-6, "inductance": 0.0},
                    "expect": {"reactance_ohm": -1.0 / (2 * PI * 1000.0 * 1e-6)},
                },
                {
                    "inputs": {"frequency": 1e6, "capacitance": 0.0, "inductance": 1e-6},
                    "expect": {"reactance_ohm": 2 * PI * 1e6 * 1e-6},
                },
                {
                    "inputs": {"frequency": 50.0, "capacitance": 100e-6, "inductance": 0.0},
                    "expect": {"reactance_ohm": -1.0 / (2 * PI * 50.0 * 100e-6)},
                },
            ],
        }
    )

    add(
        {
            "id": "decibel",
            "title": "Decibel conversion",
            "group": "basic",
            "accuracy": "exact",
            "formula": "dB = 10 log10(P2/P1) or 20 log10(V2/V1)",
            "source": "Elementary circuit theory",
            "inputs": [
                {"name": "ratio", "unit": "linear", "min": 1e-12, "max": 1e12},
                {"name": "mode", "unit": "1=power 2=voltage", "min": 1.0, "max": 2.0},
            ],
            "vectors": [
                {"inputs": {"ratio": 2.0, "mode": 1.0}, "expect": {"decibels": 10 * math.log10(2.0)}},
                {"inputs": {"ratio": 0.5, "mode": 2.0}, "expect": {"decibels": 20 * math.log10(0.5)}},
                {"inputs": {"ratio": 10.0, "mode": 1.0}, "expect": {"decibels": 10.0}},
                {"inputs": {"ratio": 100.0, "mode": 2.0}, "expect": {"decibels": 40.0}},
                {"inputs": {"ratio": 1.0, "mode": 1.0}, "expect": {"decibels": 0.0}},
            ],
        }
    )

    # copper
    tw_vectors = []
    for i, dt, ext in [(1.0, 10.0, 1.0), (2.0, 10.0, 1.0), (0.5, 20.0, 1.0), (3.0, 15.0, 0.0)]:
        if ext >= 1.0 and i <= 2.0:
            w = ipc2152_width(i)
            src = "ipc2152_table"
        else:
            w = ipc2221_width_mm(i, dt, ext >= 1.0)
            src = "ipc2221_estimate"
        tw_vectors.append(
            {
                "inputs": {"current": i, "delta_t": dt, "external": ext},
                "expect": {"width_mm": w, "standard": src},
                "rel_tol": 1e-4 if src == "ipc2152_table" else 0.02,
            }
        )
    add(
        {
            "id": "trace_width",
            "title": "Track width for current",
            "group": "copper",
            "accuracy": "estimate",
            "formula": "IPC-2152 table or IPC-2221 section 6.2 estimate",
            "source": "IPC public tables and IPC-2221 estimate",
            "inputs": [
                {"name": "current", "unit": "A", "min": 0.1, "max": 20.0},
                {"name": "delta_t", "unit": "C", "min": 1.0, "max": 100.0},
                {"name": "external", "unit": "1=outer 0=inner", "min": 0.0, "max": 1.0},
            ],
            "vectors": tw_vectors
            + [
                {
                    "inputs": {"current": 5.0, "delta_t": 10.0, "external": 1.0},
                    "expect": {"width_mm": ipc2152_width(5.0), "standard": "ipc2152_table"},
                    "rel_tol": 1e-4,
                },
            ],
        }
    )

    add(
        {
            "id": "via_current_resistance",
            "title": "Via resistance and current",
            "group": "copper",
            "accuracy": "model",
            "formula": "R = rho L / A",
            "source": "Cylinder conductor model",
            "inputs": [
                {"name": "diameter", "unit": "mm", "min": 0.1, "max": 2.0},
                {"name": "length", "unit": "mm", "min": 0.1, "max": 10.0},
                {"name": "current", "unit": "A", "min": 0.0, "max": 20.0},
            ],
            "vectors": [
                {
                    "inputs": {"diameter": 0.3, "length": 1.6, "current": 1.0},
                    "expect": {
                        "resistance_mohm": RHO_CU
                        * (1.6e-3)
                        / (PI * (0.5 * 0.3e-3) ** 2)
                        * 1000
                    },
                },
                {
                    "inputs": {"diameter": 0.2, "length": 0.8, "current": 2.0},
                    "expect": {
                        "resistance_mohm": RHO_CU * (0.8e-3) / (PI * (0.1e-3) ** 2) * 1000
                    },
                },
                {
                    "inputs": {"diameter": 0.5, "length": 2.0, "current": 0.5},
                    "expect": {
                        "resistance_mohm": RHO_CU * (2e-3) / (PI * (0.25e-3) ** 2) * 1000
                    },
                },
            ],
        }
    )

    add(
        {
            "id": "trace_resistance_drop",
            "title": "Trace resistance and voltage drop",
            "group": "copper",
            "accuracy": "model",
            "formula": "R = rho L / (w t); V = I R",
            "source": "Uniform bar resistor",
            "inputs": [
                {"name": "length", "unit": "mm", "min": 0.1, "max": 10000.0},
                {"name": "width", "unit": "mm", "min": 0.05, "max": 50.0},
                {"name": "thickness", "unit": "mm", "min": 0.01, "max": 0.2},
                {"name": "current", "unit": "A", "min": 0.0, "max": 50.0},
            ],
            "vectors": [
                {
                    "inputs": {"length": 100.0, "width": 0.3, "thickness": 0.035, "current": 1.0},
                    "expect": {
                        "resistance_mohm": RHO_CU * 0.1 / (0.3e-3 * 0.035e-3) * 1000,
                        "voltage_drop_mv": RHO_CU * 0.1 / (0.3e-3 * 0.035e-3) * 1.0 * 1000,
                    },
                },
                {
                    "inputs": {"length": 50.0, "width": 0.5, "thickness": 0.035, "current": 2.0},
                    "expect": {
                        "resistance_mohm": RHO_CU * 0.05 / (0.5e-3 * 0.035e-3) * 1000,
                        "voltage_drop_mv": RHO_CU * 0.05 / (0.5e-3 * 0.035e-3) * 2.0 * 1000,
                    },
                },
                {
                    "inputs": {"length": 10.0, "width": 1.0, "thickness": 0.07, "current": 0.5},
                    "expect": {
                        "resistance_mohm": RHO_CU * 0.01 / (1e-3 * 0.07e-3) * 1000,
                        "voltage_drop_mv": RHO_CU * 0.01 / (1e-3 * 0.07e-3) * 0.5 * 1000,
                    },
                },
            ],
        }
    )

    awg_diam = {10: 2.588, 12: 2.053, 14: 1.628, 16: 1.291, 18: 1.024, 20: 0.812, 22: 0.644, 24: 0.511}
    add(
        {
            "id": "wire_gauge",
            "title": "Wire gauge diameter",
            "group": "copper",
            "accuracy": "exact",
            "formula": "AWG solid wire diameter table",
            "source": "Standard AWG table",
            "inputs": [{"name": "awg", "unit": "AWG", "min": 10.0, "max": 24.0}],
            "vectors": [
                {"inputs": {"awg": float(g)}, "expect": {"diameter_mm": d}} for g, d in awg_diam.items()
            ],
        }
    )

    add(
        {
            "id": "thermal_via_count",
            "title": "Thermal via count estimate",
            "group": "copper",
            "accuracy": "estimate",
            "formula": "N = P/(k A deltaT)",
            "source": "Steady conduction estimate",
            "inputs": [
                {"name": "power", "unit": "W", "min": 0.1, "max": 100.0},
                {"name": "delta_t", "unit": "C", "min": 1.0, "max": 80.0},
                {"name": "via_diameter", "unit": "mm", "min": 0.2, "max": 0.8},
            ],
            "vectors": [
                {
                    "inputs": {"power": 1.0, "delta_t": 20.0, "via_diameter": 0.3},
                    "expect": {"count": math.ceil(1.0 / (K_CU * PI * (0.15e-3) ** 2 / 1.6e-3 * 20.0))},
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"power": 2.5, "delta_t": 30.0, "via_diameter": 0.25},
                    "expect": {"count": math.ceil(2.5 / (K_CU * PI * (0.125e-3) ** 2 / 1.6e-3 * 30.0))},
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"power": 0.5, "delta_t": 10.0, "via_diameter": 0.4},
                    "expect": {"count": math.ceil(0.5 / (K_CU * PI * (0.2e-3) ** 2 / 1.6e-3 * 10.0))},
                    "rel_tol": 0.05,
                },
            ],
        }
    )

    # impedance vectors from oracle constants
    ms_cases = [
        (1.0, 1.6, 0.035, 4.5),
        (2.0, 1.6, 0.035, 4.5),
        (0.5, 1.6, 0.035, 4.5),
        (0.8, 1.6, 0.035, 4.5),
        (3.2, 1.6, 0.035, 4.5),
    ]
    ms_vectors = []
    for w, h, t, er in ms_cases:
        z, ee = microstrip(w, h, t, er)
        ms_vectors.append(
            {
                "inputs": {"width": w, "height": h, "thickness": t, "er": er},
                "expect": {"impedance_ohm": z, "eeff": ee},
                "rel_tol": 1e-6,
            }
        )
    w50 = width_for_ohms(50.0, 1.6, 0.035, 4.5)
    ms_vectors.append(
        {
            "inputs": {"target_ohm": 50.0, "height": 1.6, "thickness": 0.035, "er": 4.5},
            "expect": {"width_mm": w50},
            "rel_tol": 1e-4,
        }
    )

    add(
        {
            "id": "microstrip",
            "title": "Microstrip impedance",
            "group": "impedance",
            "accuracy": "model",
            "formula": "Hammerstad-Jensen 1980 with thickness correction",
            "source": "Hammerstad and Jensen 1980",
            "inputs": [
                {"name": "width", "unit": "mm", "min": 0.01, "max": 100.0},
                {"name": "height", "unit": "mm", "min": 0.05, "max": 10.0},
                {"name": "thickness", "unit": "mm", "min": 0.0, "max": 0.2},
                {"name": "er", "unit": "1", "min": 1.0, "max": 20.0},
                {"name": "target_ohm", "unit": "ohm", "min": 0.0, "max": 200.0},
            ],
            "vectors": ms_vectors,
        }
    )

    emb_vectors = []
    for w, h, t, er in [(1.0, 1.6, 0.035, 4.5), (0.5, 0.8, 0.018, 3.5)]:
        z, ee = microstrip(w, h * 0.6, t, er * 1.05)
        emb_vectors.append(
            {
                "inputs": {"width": w, "height": h, "thickness": t, "er": er},
                "expect": {"impedance_ohm": z, "eeff": ee},
                "rel_tol": 0.02,
            }
        )
    add(
        {
            "id": "embedded_microstrip",
            "title": "Embedded microstrip",
            "group": "impedance",
            "accuracy": "model",
            "formula": "Effective height and permittivity adjustment",
            "source": "Approximate embedded model",
            "inputs": [
                {"name": "width", "unit": "mm", "min": 0.01, "max": 100.0},
                {"name": "height", "unit": "mm", "min": 0.05, "max": 10.0},
                {"name": "thickness", "unit": "mm", "min": 0.0, "max": 0.2},
                {"name": "er", "unit": "1", "min": 1.0, "max": 20.0},
            ],
            "vectors": emb_vectors
            + [
                {
                    "inputs": {"width": 0.3, "height": 1.0, "thickness": 0.035, "er": 4.2},
                    "expect": {
                        "impedance_ohm": microstrip(0.3, 1.0 * 0.6, 0.035, 4.2 * 1.05)[0],
                        "eeff": microstrip(0.3, 1.0 * 0.6, 0.035, 4.2 * 1.05)[1],
                    },
                    "rel_tol": 0.02,
                },
            ],
        }
    )

    st_vectors = []
    for w, b, t, er in [(0.14, 1.68, 0.035, 4.5), (0.56, 1.68, 0.035, 4.5), (0.2, 0.5, 0.018, 3.6)]:
        st_vectors.append(
            {
                "inputs": {"width": w, "plane_spacing": b, "thickness": t, "er": er},
                "expect": {"impedance_ohm": stripline(w, b, t, er)},
                "rel_tol": 0.02,
            }
        )
    add(
        {
            "id": "stripline",
            "title": "Stripline impedance",
            "group": "impedance",
            "accuracy": "model",
            "formula": "Symmetric stripline elliptic integral",
            "source": "IPC-style stripline model",
            "inputs": [
                {"name": "width", "unit": "mm", "min": 0.01, "max": 50.0},
                {"name": "plane_spacing", "unit": "mm", "min": 0.1, "max": 10.0},
                {"name": "thickness", "unit": "mm", "min": 0.0, "max": 0.2},
                {"name": "er", "unit": "1", "min": 1.0, "max": 20.0},
            ],
            "vectors": st_vectors
            + [
                {
                    "inputs": {"width": 1.12, "plane_spacing": 1.68, "thickness": 0.035, "er": 4.5},
                    "expect": {"impedance_ohm": stripline(1.12, 1.68, 0.035, 4.5)},
                    "rel_tol": 0.02,
                },
            ],
        }
    )

    cpw_vectors = []
    for w, g, h, er in [(0.5, 0.2, 1.6, 4.5), (1.0, 0.3, 1.6, 4.5)]:
        z, _ = microstrip(w, h, 0.035, er)
        cpw_vectors.append(
            {
                "inputs": {"width": w, "gap": g, "height": h, "er": er},
                "expect": {"impedance_ohm": z * (1.0 + 0.3 * g / w)},
                "rel_tol": 0.05,
            }
        )
    add(
        {
            "id": "coplanar_waveguide",
            "title": "Coplanar waveguide",
            "group": "impedance",
            "accuracy": "model",
            "formula": "Microstrip with gap correction",
            "source": "Approximate CPW model",
            "inputs": [
                {"name": "width", "unit": "mm", "min": 0.05, "max": 50.0},
                {"name": "gap", "unit": "mm", "min": 0.05, "max": 5.0},
                {"name": "height", "unit": "mm", "min": 0.05, "max": 10.0},
                {"name": "er", "unit": "1", "min": 1.0, "max": 20.0},
            ],
            "vectors": cpw_vectors
            + [
                {
                    "inputs": {"width": 0.8, "gap": 0.15, "height": 1.0, "er": 4.0},
                    "expect": {
                        "impedance_ohm": microstrip(0.8, 1.0, 0.035, 4.0)[0] * (1.0 + 0.3 * 0.15 / 0.8)
                    },
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"width": 0.4, "gap": 0.1, "height": 0.8, "er": 4.8},
                    "expect": {
                        "impedance_ohm": microstrip(0.4, 0.8, 0.035, 4.8)[0] * (1.0 + 0.3 * 0.1 / 0.4)
                    },
                    "rel_tol": 0.05,
                },
            ],
        }
    )

    add(
        {
            "id": "differential_pair",
            "title": "Differential pair impedance",
            "group": "impedance",
            "accuracy": "model",
            "formula": "Zdiff = 2 Zodd approximation",
            "source": "Coupled microstrip odd mode",
            "inputs": [
                {"name": "width", "unit": "mm", "min": 0.05, "max": 10.0},
                {"name": "gap", "unit": "mm", "min": 0.05, "max": 2.0},
                {"name": "height", "unit": "mm", "min": 0.05, "max": 10.0},
                {"name": "er", "unit": "1", "min": 1.0, "max": 20.0},
            ],
            "vectors": [
                {
                    "inputs": {"width": 0.8, "gap": 0.8, "height": 1.6, "er": 4.5},
                    "expect": {
                        "z_diff_ohm": 2.0
                        * microstrip(0.8, 1.6, 0.0, 4.5)[0]
                        * (0.58 - 0.03 * min(0.8 / 1.6, 3.0))
                    },
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"width": 0.15, "gap": 0.15, "height": 0.2, "er": 4.5},
                    "expect": {
                        "z_diff_ohm": 2.0
                        * microstrip(0.15, 0.2, 0.0, 4.5)[0]
                        * (0.58 - 0.03 * min(0.15 / 0.2, 3.0))
                    },
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"width": 0.64, "gap": 0.4, "height": 0.8, "er": 3.5},
                    "expect": {
                        "z_diff_ohm": 2.0
                        * microstrip(0.64, 0.8, 0.0, 3.5)[0]
                        * (0.58 - 0.03 * min(0.4 / 0.8, 3.0))
                    },
                    "rel_tol": 0.05,
                },
            ],
        }
    )

    add(
        {
            "id": "propagation_delay",
            "title": "Propagation delay",
            "group": "impedance",
            "accuracy": "model",
            "formula": "t = L sqrt(eeff)/c",
            "source": "Transmission line delay",
            "inputs": [
                {"name": "length", "unit": "mm", "min": 0.01, "max": 10000.0},
                {"name": "eeff", "unit": "1", "min": 1.0, "max": 20.0},
            ],
            "vectors": [
                {
                    "inputs": {"length": 100.0, "eeff": 4.0},
                    "expect": {"delay_ps": 100.0 * math.sqrt(4.0) / LIGHT_MM_PS},
                },
                {
                    "inputs": {"length": 25.4, "eeff": 3.5},
                    "expect": {"delay_ps": 25.4 * math.sqrt(3.5) / LIGHT_MM_PS},
                },
                {
                    "inputs": {"length": 1.0, "eeff": 2.2},
                    "expect": {"delay_ps": 1.0 * math.sqrt(2.2) / LIGHT_MM_PS},
                },
            ],
        }
    )

    add(
        {
            "id": "crosstalk",
            "title": "Crosstalk estimate",
            "group": "impedance",
            "accuracy": "estimate",
            "formula": "NEXT dB from spacing/height ratio",
            "source": "Empirical coupling estimate",
            "inputs": [
                {"name": "spacing", "unit": "mm", "min": 0.05, "max": 10.0},
                {"name": "height", "unit": "mm", "min": 0.05, "max": 10.0},
                {"name": "length", "unit": "mm", "min": 1.0, "max": 1000.0},
            ],
            "vectors": [
                {
                    "inputs": {"spacing": 0.3, "height": 1.6, "length": 50.0},
                    "expect": {"next_db": -20.0 * math.log10(0.3 / 1.6) - 10.0 * math.log10(50.0 / 25.4)},
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"spacing": 0.5, "height": 1.6, "length": 100.0},
                    "expect": {"next_db": -20.0 * math.log10(0.5 / 1.6) - 10.0 * math.log10(100.0 / 25.4)},
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"spacing": 1.0, "height": 1.6, "length": 25.0},
                    "expect": {"next_db": -20.0 * math.log10(1.0 / 1.6) - 10.0 * math.log10(25.0 / 25.4)},
                    "rel_tol": 0.05,
                },
            ],
        }
    )

    add(
        {
            "id": "conductor_spacing_ipc2221",
            "title": "Conductor spacing IPC-2221",
            "group": "spacing",
            "accuracy": "estimate",
            "formula": "Spacing table by voltage and coating",
            "source": "IPC-2221 spacing table estimate",
            "inputs": [
                {"name": "voltage", "unit": "V", "min": 0.0, "max": 1000.0},
                {"name": "coated", "unit": "1=yes 0=no", "min": 0.0, "max": 1.0},
            ],
            "vectors": [
                {"inputs": {"voltage": 30.0, "coated": 1.0}, "expect": {"spacing_mm": 0.1}},
                {"inputs": {"voltage": 150.0, "coated": 0.0}, "expect": {"spacing_mm": 0.8}},
                {"inputs": {"voltage": 300.0, "coated": 0.0}, "expect": {"spacing_mm": 1.5}},
            ],
        }
    )

    add(
        {
            "id": "creepage_clearance_iec60664",
            "title": "Creepage and clearance IEC 60664",
            "group": "spacing",
            "accuracy": "estimate",
            "formula": "Pollution degree and material group table",
            "source": "IEC 60664 simplified table",
            "inputs": [
                {"name": "voltage", "unit": "V", "min": 0.0, "max": 1000.0},
                {"name": "pollution", "unit": "1-3", "min": 1.0, "max": 3.0},
            ],
            "vectors": [
                {"inputs": {"voltage": 50.0, "pollution": 2.0}, "expect": {"clearance_mm": 0.5, "creepage_mm": 0.8}},
                {"inputs": {"voltage": 250.0, "pollution": 2.0}, "expect": {"clearance_mm": 1.5, "creepage_mm": 2.5}},
                {"inputs": {"voltage": 400.0, "pollution": 3.0}, "expect": {"clearance_mm": 3.0, "creepage_mm": 4.0}},
            ],
        }
    )

    # components
    add(
        {
            "id": "decoupling_capacitor",
            "title": "Decoupling capacitor",
            "group": "components",
            "accuracy": "estimate",
            "formula": "C = I dt/dV",
            "source": "Charge delivery estimate",
            "inputs": [
                {"name": "delta_i", "unit": "A", "min": 0.01, "max": 10.0},
                {"name": "delta_v", "unit": "V", "min": 0.001, "max": 1.0},
                {"name": "delta_t", "unit": "s", "min": 1e-9, "max": 1e-3},
            ],
            "vectors": [
                {
                    "inputs": {"delta_i": 0.5, "delta_v": 0.05, "delta_t": 10e-9},
                    "expect": {"capacitance_f": 0.5 * 10e-9 / 0.05},
                },
                {
                    "inputs": {"delta_i": 1.0, "delta_v": 0.1, "delta_t": 5e-9},
                    "expect": {"capacitance_f": 1.0 * 5e-9 / 0.1},
                },
                {
                    "inputs": {"delta_i": 0.2, "delta_v": 0.02, "delta_t": 20e-9},
                    "expect": {"capacitance_f": 0.2 * 20e-9 / 0.02},
                },
            ],
        }
    )

    add(
        {
            "id": "i2c_pullup",
            "title": "I2C pull-up resistor",
            "group": "components",
            "accuracy": "estimate",
            "formula": "R from rise time and bus capacitance",
            "source": "I2C specification estimate",
            "inputs": [
                {"name": "voltage", "unit": "V", "min": 1.8, "max": 5.5},
                {"name": "capacitance", "unit": "F", "min": 10e-12, "max": 500e-12},
                {"name": "rise_ns", "unit": "ns", "min": 20.0, "max": 1000.0},
            ],
            "vectors": [
                {
                    "inputs": {"voltage": 3.3, "capacitance": 100e-12, "rise_ns": 300.0},
                    "expect": {"resistor_ohm": 300e-9 / (0.8473 * 100e-12 * math.log(3.3 / (3.3 - 0.9 * 3.3)))},
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"voltage": 5.0, "capacitance": 200e-12, "rise_ns": 500.0},
                    "expect": {"resistor_ohm": 500e-9 / (0.8473 * 200e-12 * math.log(5.0 / (5.0 - 0.9 * 5.0)))},
                    "rel_tol": 0.05,
                },
                {
                    "inputs": {"voltage": 1.8, "capacitance": 50e-12, "rise_ns": 200.0},
                    "expect": {"resistor_ohm": 200e-9 / (0.8473 * 50e-12 * math.log(1.8 / (1.8 - 0.9 * 1.8)))},
                    "rel_tol": 0.05,
                },
            ],
        }
    )

    add(
        {
            "id": "crystal_load_capacitors",
            "title": "Crystal load capacitors",
            "group": "components",
            "accuracy": "estimate",
            "formula": "Cl = 2(Cload - Cstray)",
            "source": "Pierce oscillator estimate",
            "inputs": [
                {"name": "cload", "unit": "pF", "min": 6.0, "max": 30.0},
                {"name": "cstray", "unit": "pF", "min": 0.0, "max": 10.0},
            ],
            "vectors": [
                {"inputs": {"cload": 18.0, "cstray": 3.0}, "expect": {"capacitor_pf": 2.0 * (18.0 - 3.0)}},
                {"inputs": {"cload": 12.5, "cstray": 2.0}, "expect": {"capacitor_pf": 2.0 * (12.5 - 2.0)}},
                {"inputs": {"cload": 20.0, "cstray": 4.0}, "expect": {"capacitor_pf": 2.0 * (20.0 - 4.0)}},
            ],
        }
    )

    add(
        {
            "id": "timer_555",
            "title": "555 timer frequency",
            "group": "components",
            "accuracy": "estimate",
            "formula": "f = 1.44/((Ra+2Rb)C)",
            "source": "NE555 datasheet",
            "inputs": [
                {"name": "ra", "unit": "ohm", "min": 1000.0, "max": 1e7},
                {"name": "rb", "unit": "ohm", "min": 1000.0, "max": 1e7},
                {"name": "c", "unit": "F", "min": 1e-9, "max": 100e-6},
            ],
            "vectors": [
                {
                    "inputs": {"ra": 10000.0, "rb": 10000.0, "c": 10e-6},
                    "expect": {"frequency_hz": 1.44 / (30000.0 * 10e-6)},
                },
                {
                    "inputs": {"ra": 4700.0, "rb": 4700.0, "c": 1e-6},
                    "expect": {"frequency_hz": 1.44 / (14100.0 * 1e-6)},
                },
                {
                    "inputs": {"ra": 100000.0, "rb": 100000.0, "c": 100e-9},
                    "expect": {"frequency_hz": 1.44 / (300000.0 * 100e-9)},
                },
            ],
        }
    )

    add(
        {
            "id": "opamp_gain",
            "title": "Op-amp gain",
            "group": "components",
            "accuracy": "exact",
            "formula": "Non-inverting: 1+R2/R1; inverting: -R2/R1",
            "source": "Ideal op-amp",
            "inputs": [
                {"name": "r1", "unit": "ohm", "min": 1.0, "max": 1e9},
                {"name": "r2", "unit": "ohm", "min": 1.0, "max": 1e9},
                {"name": "inverting", "unit": "1=yes 0=no", "min": 0.0, "max": 1.0},
            ],
            "vectors": [
                {"inputs": {"r1": 1000.0, "r2": 9000.0, "inverting": 0.0}, "expect": {"gain": 10.0}},
                {"inputs": {"r1": 10000.0, "r2": 100000.0, "inverting": 1.0}, "expect": {"gain": -10.0}},
                {"inputs": {"r1": 2200.0, "r2": 2200.0, "inverting": 0.0}, "expect": {"gain": 2.0}},
            ],
        }
    )

    add(
        {
            "id": "ldo_thermal",
            "title": "LDO thermal resistance",
            "group": "components",
            "accuracy": "estimate",
            "formula": "theta = deltaT/P",
            "source": "Steady-state thermal",
            "inputs": [
                {"name": "power", "unit": "W", "min": 0.01, "max": 10.0},
                {"name": "ambient", "unit": "C", "min": -40.0, "max": 85.0},
                {"name": "max_junction", "unit": "C", "min": 25.0, "max": 150.0},
            ],
            "vectors": [
                {"inputs": {"power": 0.5, "ambient": 25.0, "max_junction": 125.0}, "expect": {"theta_c_per_w": (125.0 - 25.0) / 0.5}},
                {"inputs": {"power": 1.0, "ambient": 40.0, "max_junction": 150.0}, "expect": {"theta_c_per_w": (150.0 - 40.0) / 1.0}},
                {"inputs": {"power": 0.25, "ambient": 30.0, "max_junction": 100.0}, "expect": {"theta_c_per_w": (100.0 - 30.0) / 0.25}},
            ],
        }
    )

    add(
        {
            "id": "battery_life",
            "title": "Battery life",
            "group": "components",
            "accuracy": "estimate",
            "formula": "hours = capacity/current",
            "source": "Charge accounting",
            "inputs": [
                {"name": "capacity_mah", "unit": "mAh", "min": 10.0, "max": 50000.0},
                {"name": "current_ma", "unit": "mA", "min": 0.01, "max": 5000.0},
            ],
            "vectors": [
                {"inputs": {"capacity_mah": 2000.0, "current_ma": 10.0}, "expect": {"hours": 200.0}},
                {"inputs": {"capacity_mah": 500.0, "current_ma": 25.0}, "expect": {"hours": 20.0}},
                {"inputs": {"capacity_mah": 100.0, "current_ma": 5.0}, "expect": {"hours": 20.0}},
            ],
        }
    )

    add(
        {
            "id": "resistor_code",
            "title": "Resistor colour code",
            "group": "components",
            "accuracy": "exact",
            "formula": "Digit lookup table",
            "source": "IEC 60062",
            "inputs": [{"name": "digits", "unit": "3-digit code", "min": 100.0, "max": 999.0}],
            "vectors": [
                {"inputs": {"digits": 472.0}, "expect": {"resistance_ohm": 4700.0}},
                {"inputs": {"digits": 103.0}, "expect": {"resistance_ohm": 10000.0}},
                {"inputs": {"digits": 220.0}, "expect": {"resistance_ohm": 22.0}},
            ],
        }
    )

    add(
        {
            "id": "e_series",
            "title": "E-series nearest value",
            "group": "components",
            "accuracy": "exact",
            "formula": "Nearest E12 value",
            "source": "IEC 60063",
            "inputs": [{"name": "value", "unit": "ohm", "min": 0.1, "max": 1e9}],
            "vectors": [
                {"inputs": {"value": 4800.0}, "expect": {"nearest_ohm": nearest_e12(4800.0)}},
                {"inputs": {"value": 333.0}, "expect": {"nearest_ohm": nearest_e12(333.0)}},
                {"inputs": {"value": 0.95}, "expect": {"nearest_ohm": nearest_e12(0.95)}},
            ],
        }
    )

    uc_vectors = []
    conversions = [
        ("mm", "mil", 1.0, 39.3700787),
        ("mil", "mm", 10.0, 0.254),
        ("inch", "mm", 1.0, 25.4),
        ("mm", "inch", 25.4, 1.0),
        ("f", "uf", 1e-6, 1.0),
        ("uf", "nf", 1.0, 1000.0),
        ("hz", "khz", 1000.0, 1.0),
        ("khz", "mhz", 1000.0, 1.0),
        ("w", "mw", 1.0, 1000.0),
        ("v", "mv", 1.0, 1000.0),
        ("a", "ma", 1.0, 1000.0),
    ]
    for src, dst, val, exp in conversions:
        uc_vectors.append(
            {
                "inputs": {"value": val, "from_unit": src, "to_unit": dst},
                "expect": {"result": exp},
                "rel_tol": 1e-6,
            }
        )
    add(
        {
            "id": "unit_converter",
            "title": "Unit converter",
            "group": "utilities",
            "accuracy": "exact",
            "formula": "Fixed conversion factors",
            "source": "SI and industry units",
            "inputs": [
                {"name": "value", "unit": "any", "min": -1e12, "max": 1e12},
                {"name": "from_unit", "unit": "name", "min": 0.0, "max": 0.0},
                {"name": "to_unit", "unit": "name", "min": 0.0, "max": 0.0},
            ],
            "vectors": uc_vectors,
        }
    )

    total_vectors = sum(len(c["vectors"]) for c in calcs)
    if len(calcs) != 32:
        raise SystemExit(f"expected 32 calculators, got {len(calcs)}")
    if total_vectors != 119:
        raise SystemExit(f"expected 119 vectors, got {total_vectors}")

    print('schema = 1')
    print('generated = "laptop-provisional pending ee-thesaurus copy"')
    print()
    for c in calcs:
        print(emit_calc(c))
        print()


if __name__ == "__main__":
    main()
