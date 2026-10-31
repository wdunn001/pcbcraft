//! Declarative metadata for every calculator, mirroring `data/calculators.toml`.
//!
//! The data file is the source of truth; `tests/golden.rs` cross-checks this
//! table against it. The table exists so the engine and MCP layers can
//! discover ids, input ranges and formulas at runtime without pulling in
//! `toml` as a runtime dependency.
//!
//! This file is generated from the data file by a one-off script; regenerate
//! it whenever the data file changes.

/// One input of a calculator: name, unit label, inclusive range, and
/// whether the input may be omitted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputMeta {
    /// Input name, identical to the data file and the input struct field.
    pub name: &'static str,
    /// Unit label for UI display; empty when dimensionless.
    pub unit: &'static str,
    /// Inclusive lower bound, when the input is bounded.
    pub min: Option<f64>,
    /// Inclusive upper bound, when the input is bounded.
    pub max: Option<f64>,
    /// True when the calculator accepts the input being absent.
    pub optional: bool,
    /// Choice labels, empty for plain numeric inputs.
    pub values: &'static [&'static str],
}

impl InputMeta {
    /// Numeric input with a closed range.
    const fn num(name: &'static str, unit: &'static str, min: f64, max: f64) -> Self {
        Self {
            name,
            unit,
            min: Some(min),
            max: Some(max),
            optional: false,
            values: &[],
        }
    }

    /// Optional numeric input with a closed range.
    const fn opt(name: &'static str, unit: &'static str, min: f64, max: f64) -> Self {
        Self {
            name,
            unit,
            min: Some(min),
            max: Some(max),
            optional: true,
            values: &[],
        }
    }

    /// Choice input (enum or boolean) with labels and no numeric range.
    const fn choice(name: &'static str, values: &'static [&'static str]) -> Self {
        Self {
            name,
            unit: "enum",
            min: None,
            max: None,
            optional: false,
            values,
        }
    }

    /// Free-form text input that may be omitted.
    const fn text(name: &'static str, unit: &'static str) -> Self {
        Self {
            name,
            unit,
            min: None,
            max: None,
            optional: true,
            values: &[],
        }
    }
}

/// Metadata describing one calculator: identity, provenance and inputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalculatorMeta {
    /// Stable id, identical to the data file.
    pub id: &'static str,
    /// Human readable name.
    pub name: &'static str,
    /// Module group: basic, copper, impedance, spacing, components or utilities.
    pub group: &'static str,
    /// One sentence summary.
    pub summary: &'static str,
    /// Formula text, copied from the data file.
    pub formula: &'static str,
    /// Sources backing the numbers.
    pub source: &'static str,
    /// Applicability limits in prose.
    pub validity: &'static str,
    /// Accuracy statement in prose.
    pub accuracy: &'static str,
    /// Inputs in declaration order.
    pub inputs: &'static [InputMeta],
}

/// Every calculator, in data file order.
pub static CALCULATORS: [CalculatorMeta; 32] = [
    CalculatorMeta {
        id: "ohms_law",
        name: "Ohm's law and power",
        group: "basic",
        summary: "Give any two of voltage, current and resistance; get the third and the power.",
        formula: "V = I R; P = V I = I^2 R = V^2 / R. Exactly two inputs must be given.",
        source: "Ohm's law and Joule heating, any introductory circuits text (Horowitz and Hill, The Art of Electronics, 3rd ed., ch. 1).",
        validity: "Linear resistors at DC or RMS AC. Not for reactive loads (use reactance).",
        accuracy: "exact",
        inputs: &[
            InputMeta::opt("voltage", "V", 0.0, 1000000.0),
            InputMeta::opt("current", "A", 0.0, 1000000.0),
            InputMeta::opt("resistance", "ohm", 1e-09, 1000000000000.0),
        ],
    },
    CalculatorMeta {
        id: "series_parallel",
        name: "Series and parallel R, C, L",
        group: "basic",
        summary: "Combine up to 16 resistors, capacitors or inductors in series or in parallel.",
        formula: "R, L series = sum; parallel = 1 / sum(1/x). C is the reverse: parallel = sum, series = 1 / sum(1/x).",
        source: "Kirchhoff's laws; any introductory text.",
        validity: "Ideal components.",
        accuracy: "exact",
        inputs: &[
            InputMeta::choice("kind", &["resistor", "capacitor", "inductor"]),
            InputMeta::num("values", "ohm | F | H", 1e-15, 1000000000000.0),
        ],
    },
    CalculatorMeta {
        id: "voltage_divider",
        name: "Voltage divider with load",
        group: "basic",
        summary: "Output voltage of two resistors, with an optional load resistor across the output.",
        formula: "R2e = load given ? R2 || load : R2; Vout = Vin R2e / (R1 + R2e); I = Vin / (R1 + R2e).",
        source: "Horowitz and Hill, The Art of Electronics, 3rd ed., section 1.2.3 (voltage dividers) and Thevenin loading.",
        validity: "Linear resistors; load modelled as a resistor to ground.",
        accuracy: "exact",
        inputs: &[
            InputMeta::num("vin", "V", 0.0, 100000.0),
            InputMeta::num("r1", "ohm", 0.001, 1000000000000.0),
            InputMeta::num("r2", "ohm", 0.001, 1000000000000.0),
            InputMeta::opt("load", "ohm", 0.001, 1000000000000.0),
        ],
    },
    CalculatorMeta {
        id: "led_resistor",
        name: "LED series resistor",
        group: "basic",
        summary: "Resistor for an LED from supply voltage, forward voltage and wanted current, with the resistor's power and the nearest E-series value.",
        formula: "R = (Vs - Vf) / I; P = (Vs - Vf) I. Refuse when Vf >= Vs.",
        source: "Ohm's law; typical Vf: red 1.8 to 2.2 V, green or blue or white 2.8 to 3.4 V (any LED datasheet).",
        validity: "One LED or a series string whose total Vf is entered. Vf is current dependent, so the result is a starting point.",
        accuracy: "exact for the given Vf",
        inputs: &[
            InputMeta::num("supply", "V", 0.1, 1000.0),
            InputMeta::num("forward_voltage", "V", 0.1, 50.0),
            InputMeta::num("current", "A", 1e-06, 10.0),
            InputMeta::choice("series", &["E6", "E12", "E24", "E96"]),
        ],
    },
    CalculatorMeta {
        id: "rc_rl_time_constant",
        name: "RC and RL time constant and cutoff",
        group: "basic",
        summary: "Time constant and the -3 dB corner frequency of a first-order RC or RL network.",
        formula: "RC: tau = R C. RL: tau = L / R. fc = 1 / (2 pi tau).",
        source: "First-order response; Horowitz and Hill, 3rd ed., section 1.4.2 and 1.7.1.",
        validity: "Single-pole networks only.",
        accuracy: "exact",
        inputs: &[
            InputMeta::choice("kind", &["rc", "rl"]),
            InputMeta::num("resistance", "ohm", 0.001, 1000000000000.0),
            InputMeta::opt("capacitance", "F", 1e-15, 1.0),
            InputMeta::opt("inductance", "H", 1e-12, 1000.0),
        ],
    },
    CalculatorMeta {
        id: "lc_resonance",
        name: "Resonant frequency",
        group: "basic",
        summary: "Resonant frequency of an inductor and capacitor, or the L or C needed for a target frequency.",
        formula: "f0 = 1 / (2 pi sqrt(L C)); solve for the missing one of the three.",
        source: "Thomson's formula; any circuits text.",
        validity: "Lossless LC. Real parts shift f0 by their parasitics.",
        accuracy: "exact",
        inputs: &[
            InputMeta::opt("inductance", "H", 1e-12, 1000.0),
            InputMeta::opt("capacitance", "F", 1e-15, 1.0),
            InputMeta::opt("frequency", "Hz", 0.001, 1000000000000.0),
        ],
    },
    CalculatorMeta {
        id: "reactance",
        name: "Reactance",
        group: "basic",
        summary: "Reactance of a capacitor or inductor at a frequency.",
        formula: "Xc = 1 / (2 pi f C); Xl = 2 pi f L.",
        source: "Any circuits text.",
        validity: "Ideal components; ignores ESR and self resonance.",
        accuracy: "exact",
        inputs: &[
            InputMeta::num("frequency", "Hz", 0.001, 1000000000000.0),
            InputMeta::opt("capacitance", "F", 1e-15, 1.0),
            InputMeta::opt("inductance", "H", 1e-12, 1000.0),
        ],
    },
    CalculatorMeta {
        id: "decibel",
        name: "Decibel conversions",
        group: "basic",
        summary: "Convert between power ratio, voltage ratio, dB, dBm and watts.",
        formula: "dB(power) = 10 log10(P2/P1); dB(voltage, equal impedance) = 20 log10(V2/V1); dBm = 10 log10(P / 1 mW); P = 1 mW * 10^(dBm/10).",
        source: "IEEE Std 100 definitions; any RF text.",
        validity: "Voltage form assumes equal source and load impedance.",
        accuracy: "exact",
        inputs: &[
            InputMeta::choice(
                "mode",
                &["power_ratio", "voltage_ratio", "dbm_to_w", "w_to_dbm"],
            ),
            InputMeta::num("value", "ratio | dB | dBm | W", -300.0, 1000000000000.0),
        ],
    },
    CalculatorMeta {
        id: "trace_width",
        name: "Trace width for current",
        group: "copper",
        summary: "Minimum copper width for a current and allowed temperature rise. IPC-2152 table lookup when inside its published range, IPC-2221 formula otherwise, both stated.",
        formula: "IPC-2221 (closed form, both layers): I = k dT^0.44 A^0.725, A in mil^2, k = 0.048 external, 0.024 internal.\n  A = (I / (k dT^0.44))^(1/0.725); width_mil = A / thickness_mil; thickness_mil = oz * 34.79 / 25.4.\nIPC-2152 (table, used by `auto` only when layer = external, copper_weight = 1 and 1 A <= I <= 10 A and dT = 10 K):\n  log-log interpolation in the published 1 oz / 10 K external table\n  (A, mil): (1,10) (2,30) (3,50) (4,80) (5,110) (6,150) (7,180) (8,220) (9,260) (10,300).\n  Other copper weights scale width by 1/oz as a stated approximation (constant cross-section), flagged in notes.\n",
        source: "IPC-2221 constants: Sierra Circuits, \"How to optimize your PCB trace using IPC-2152\" (sources/standards/ipc2152-protoexpress.txt) and Altium IPC-2221 calculator page (sources/standards/ipc2221-trace-calc-altium.txt); the standard itself is paywalled (IPC-2221C, shop.ipc.org).\nIPC-2152 table: Altium, \"PCB trace width vs current table\" (sources/standards/ipc2152-altium-width-table.txt), a secondary reproduction of one IPC-2152 chart; the standard is paywalled (shop.ipc.org/ipc-2152).\nCross-check table: Philip McGaw trace width vs current table (sources/standards/trace-width-philipmcgaw.txt).\n",
        validity: "IPC-2221 curves come from 1950s test boards; the standard describes them as conservative. Assumptions stated to the user: still air, no adjacent copper planes, trace length much longer than width, FR-4.\nIPC-2152 published chart only covers 1 oz external copper at 10 K rise here; anything else falls back to IPC-2221 and the notes say so.\nInternal layers: IPC-2152 found internal and external traces behave about the same when a plane is nearby; IPC-2221 halves internal current. The notes mention this.\n",
        accuracy: "IPC-2221: matches the published 10 K table within 1 % (the 1 % is the 34.79 vs 35 um copper rounding). IPC-2152: interpolation of a secondary reproduction; treat as +-10 %.",
        inputs: &[
            InputMeta::num("current", "A", 0.001, 100.0),
            InputMeta::num("temperature_rise", "K", 1.0, 100.0),
            InputMeta::num("copper_weight", "oz/ft2", 0.25, 6.0),
            InputMeta::choice("layer", &["external", "internal"]),
            InputMeta::choice("method", &["auto", "ipc2152", "ipc2221"]),
        ],
    },
    CalculatorMeta {
        id: "via_current_resistance",
        name: "Via current and resistance",
        group: "copper",
        summary: "Current a plated via carries for a temperature rise, its DC resistance, and how many vias in parallel reach a target current.",
        formula: "A = pi/4 ((drill + 2 plating)^2 - drill^2)  [mm^2]; A_mil2 = A / 0.0254^2.\nI = 0.024 dT^0.44 A_mil2^0.725   (IPC-2221 internal-conductor constant, the usual via convention).\nR = RHO_CU length / A.  count = ceil(target_current / I).\n",
        source: "IPC-2221 formula as for trace_width; via convention (barrel area with internal k) as described in Altium 'PCB via size and pad size guidelines' (sources/standards/via-pad-size-altium.txt). Copper resistivity 1.72e-8 ohm m (CRC Handbook).",
        validity: "Plating thickness is what the fab actually deposits (IPC-6012 Class 2 average 20 um, Class 3 25 um); barrel resistance ignores the pad to barrel transition.",
        accuracy: "same as IPC-2221; resistance exact for the modelled cylinder",
        inputs: &[
            InputMeta::num("drill", "mm", 0.05, 10.0),
            InputMeta::num("plating", "um", 5.0, 100.0),
            InputMeta::num("length", "mm", 0.1, 10.0),
            InputMeta::num("temperature_rise", "K", 1.0, 100.0),
            InputMeta::opt("target_current", "A", 0.001, 100.0),
        ],
    },
    CalculatorMeta {
        id: "trace_resistance_drop",
        name: "Trace resistance and voltage drop",
        group: "copper",
        summary: "DC resistance of a copper trace at a temperature, and the voltage drop and heat at a current.",
        formula: "t = oz * 34.79e-6 m; R20 = RHO_CU L / (w t); R = R20 (1 + ALPHA_CU (T - 20)); V = I R; P = I^2 R.",
        source: "Resistivity and temperature coefficient of copper, CRC Handbook of Chemistry and Physics (1.72e-8 ohm m, 0.00393 /K).",
        validity: "DC and low frequency. Above about 1 MHz skin depth (66 um at 1 MHz for copper) makes the effective thickness smaller; a note says so when the user enters a frequency in the future.",
        accuracy: "exact for the model; real copper is 2 to 10 % thinner than nominal after etching",
        inputs: &[
            InputMeta::num("width", "mm", 0.01, 100.0),
            InputMeta::num("copper_weight", "oz/ft2", 0.25, 6.0),
            InputMeta::num("length", "mm", 0.1, 10000.0),
            InputMeta::num("current", "A", 0.0, 100.0),
            InputMeta::num("temperature", "C", -55.0, 200.0),
        ],
    },
    CalculatorMeta {
        id: "microstrip",
        name: "Microstrip impedance",
        group: "impedance",
        summary: "Characteristic impedance and effective permittivity of a surface trace over one plane, or the width for a target impedance.",
        formula: "u = w/h, ZF0 = 376.730313668.\nZL1(u) = ZF0/(2 pi) ln( f(u)/u + sqrt(1 + (2/u)^2) ),  f(u) = 6 + (2 pi - 6) exp( -(30.666/u)^0.7528 )\neeff(u, er) = (er+1)/2 + (er-1)/2 (1 + 10/u)^(-a b)\na(u) = 1 + ln( (u^4 + (u/52)^2) / (u^4 + 0.432) )/49 + ln( 1 + (u/18.1)^3 )/18.7\nb(er) = 0.564 ((er - 0.9)/(er + 3))^0.053\nthickness (tn = t/h): du1 = tn/pi ln( 1 + 4e/tn tanh^2( sqrt(6.517 u) ) ); dur = du1 (1 + sech( sqrt(er - 1) ))/2; u1 = u + du1; ur = u + dur\nZ0 = ZL1(ur) / sqrt(eeff(ur, er)); eeff_t = eeff(ur, er) (ZL1(u1)/ZL1(ur))^2\ndelay = sqrt(eeff_t) / C0.\nwidth_for_target: bisection of Z0(w) on [0.01, 100] mm, 80 steps (Z0 is monotonic in w).\n",
        source: "Qucs technical documentation, microstrip line, https://qucs.sourceforge.net/tech/node75.html (transcription checked in pcbcraft-research/research-283-length-impedance.md); independent implementation scikit-rf MLine (BSD-3) https://scikit-rf.readthedocs.io/en/latest/_modules/skrf/media/mline.html. Original: E. Hammerstad and O. Jensen, 'Accurate models for microstrip computer-aided design', IEEE MTT-S 1980.",
        validity: "0.01 <= w/h <= 100, er < 128 (fit range). Quasi-static: no dispersion, fine below a few GHz on FR-4. Solder mask not modelled (lowers Z0 by 1 to 3 ohm).",
        accuracy: "Fit error 0.01 % (w/h <= 1) and 0.03 % (w/h <= 1000) for Z0 against the zero-thickness quasi-static solution; eeff within 0.2 %. Fabrication tolerance on h and er dominates (5 to 10 %).",
        inputs: &[
            InputMeta::opt("width", "mm", 0.01, 100.0),
            InputMeta::num("height", "mm", 0.01, 10.0),
            InputMeta::num("er", "", 1.0, 20.0),
            InputMeta::num("thickness", "mm", 0.0, 0.5),
            InputMeta::opt("target", "ohm", 1.0, 1000.0),
        ],
    },
    CalculatorMeta {
        id: "embedded_microstrip",
        name: "Embedded microstrip impedance",
        group: "impedance",
        summary: "Microstrip whose trace is covered by a dielectric layer (prepreg or thick mask), lowering the impedance.",
        formula: "(Z0s, eeff_s) = microstrip(w, h, er, t)\nf = exp(-2 cover / h); eeff_e = eeff_s f + er (1 - f)\nZ0 = Z0s sqrt(eeff_s / eeff_e)\n",
        source: "The cover correction appears on calculator pages and in Wadell, 'Transmission Line Design Handbook' (Artech, 1991) ch. 3 as the usual engineering estimate; the IPC-2141 embedded form (paragraph 4.2.3) is paywalled and was criticised as wrong in the source trail (see Altium, 'Clearing trace impedance calculators and formulas', https://resources.altium.com/p/clearing-trace-impedance-calculators-and-formulas).",
        validity: "cover <= 2 h. With cover >= 2 h the result approaches a fully buried line (eeff -> er) and a stripline model is better.",
        accuracy: "Engineering estimate, about 5 %. The UI labels it 'estimate; confirm with a field solver or the fab's stackup tool'.",
        inputs: &[
            InputMeta::num("width", "mm", 0.01, 100.0),
            InputMeta::num("height", "mm", 0.01, 10.0),
            InputMeta::num("cover", "mm", 0.0, 10.0),
            InputMeta::num("er", "", 1.0, 20.0),
            InputMeta::num("thickness", "mm", 0.0, 0.5),
        ],
    },
    CalculatorMeta {
        id: "stripline",
        name: "Stripline impedance",
        group: "impedance",
        summary: "Trace centred between two planes (inner layer).",
        formula: "W = w/b, T = t/b\nWeff = W - (0.35 - W)^2 / (1 + 12 T)  if W < 0.35, else W\nCf = (2/pi) ln(1/(1-T) + 1) - (T/pi) ln(1/(1-T)^2 - 1)\nZ0 = (30 pi / sqrt(er)) (1 - T) / (Weff + Cf)\ndelay = sqrt(er) / C0   (fully embedded: eeff = er)\n",
        source: "Wikipedia, 'Stripline', formula section citing M. Steer, Microwave and RF Design (LibreTexts 3.7, 2020); original S. B. Cohn, IRE Trans. MTT-2, 1954. Cross-check: IPC-2141 form Z0 = 60/sqrt(er) ln(4 b / (0.67 pi (0.8 w + t))) as printed by NextPCB, https://www.nextpcb.com/blog/pcb-impedance-calculator-microstrip-stripline-guide (that page's variable definition for h is inconsistent; b plane-to-plane makes its example agree).",
        validity: "W/(1-T) > 0.05 and T < 0.025 for the stated accuracy; symmetric only. Offset (asymmetric) stripline is a follow-up.",
        accuracy: "At least 1 % against the exact conformal mapping (Steer). The IPC-2141 cross-check agrees within 1.2 % on the three vectors.",
        inputs: &[
            InputMeta::num("width", "mm", 0.01, 100.0),
            InputMeta::num("spacing", "mm", 0.02, 20.0),
            InputMeta::num("er", "", 1.0, 20.0),
            InputMeta::num("thickness", "mm", 0.0, 0.5),
        ],
    },
    CalculatorMeta {
        id: "coplanar_waveguide",
        name: "Coplanar waveguide impedance",
        group: "impedance",
        summary: "Trace with ground copper on both sides on the same layer, with or without a plane underneath.",
        formula: "K/K'(k): for 0 <= k <= 1/sqrt2: pi / ln( 2 (1 + sqrt(k')) / (1 - sqrt(k')) ), k' = sqrt(1 - k^2);\n         for 1/sqrt2 <= k <= 1: ln( 2 (1 + sqrt(k)) / (1 - sqrt(k)) ) / pi.\nk1 = w / (w + 2 s)\nCPW, finite substrate h:   k2 = sinh(pi w / 4h) / sinh(pi (w + 2s) / 4h)\n   eeff = 1 + (er - 1)/2 * [K/K'(k2)] / [K/K'(k1)];   Z0 = 30 pi / (sqrt(eeff) K/K'(k1))\nGround-backed CPW:         k3 = tanh(pi w / 4h) / tanh(pi (w + 2s) / 4h)\n   q = K/K'(k3) / (K/K'(k1) + K/K'(k3));  eeff = 1 + q (er - 1);  Z0 = 60 pi / (sqrt(eeff) (K/K'(k1) + K/K'(k3)))\nThickness ignored (Qucs eq. 12.14 to 12.18 is the optional refinement).\n",
        source: "Qucs technical documentation, coplanar waveguides, https://qucs.sourceforge.net/tech/node86.html, eq. 12.4 to 12.13; scikit-rf CPW media (same equations) https://scikit-rf.readthedocs.io/en/latest/_modules/skrf/media/cpw.html.",
        validity: "Quasi-static; ground copper much wider than w + 2s; no thickness. Ground-backed form assumes the side grounds are tied to the plane by vias within a small fraction of a wavelength.",
        accuracy: "Hilberg ratio accurate to about 3e-6 (Qucs citing Gupta); formulas 'effectively exact' for the quasi-static zero-thickness case. Thickness and etch factor add a few percent.",
        inputs: &[
            InputMeta::num("width", "mm", 0.01, 100.0),
            InputMeta::num("gap", "mm", 0.01, 100.0),
            InputMeta::num("height", "mm", 0.01, 10.0),
            InputMeta::num("er", "", 1.0, 20.0),
            InputMeta::choice("ground_backed", &["true", "false"]),
        ],
    },
    CalculatorMeta {
        id: "differential_pair",
        name: "Differential pair impedance",
        group: "impedance",
        summary: "Differential impedance of two equal surface traces side by side, and the gap or width for a target.",
        formula: "Zdiff = 2 Z0(w, h, er, t) (1 - 0.48 exp(-0.96 s / h)); width_for_target by bisection with s fixed.",
        source: "Coupling factor as printed by NextPCB differential microstrip calculator https://www.nextpcb.com/differential-microstrip-impedance-calculator and many others, attributed to IPC-2141 (paywalled; TOC at https://www.ipc.org/TOC/IPC-2141A.pdf lists 4.4.1 edge-coupled surface microstrip). Not confirmed against the standard text (pcbcraft-research/research-283-length-impedance.md). Upgrade path: Hammerstad-Jensen coupled microstrip even/odd modes, Qucs https://qucs.sourceforge.net/tech/node77.html, odd-mode error under 0.3 %.",
        validity: "0.1 <= w/h <= 10, s/h >= 0.1, loosely coupled pairs (s >= w). For s < w the factor underestimates coupling.",
        accuracy: "Unverified against the standard; treat as +-10 %. The UI shows the word 'estimate' and the follow-up issue swaps in the coupled H-J model.",
        inputs: &[
            InputMeta::num("width", "mm", 0.01, 100.0),
            InputMeta::num("gap", "mm", 0.01, 100.0),
            InputMeta::num("height", "mm", 0.01, 10.0),
            InputMeta::num("er", "", 1.0, 20.0),
            InputMeta::num("thickness", "mm", 0.0, 0.5),
            InputMeta::opt("target", "ohm", 10.0, 1000.0),
        ],
    },
    CalculatorMeta {
        id: "propagation_delay",
        name: "Propagation delay and length matching",
        group: "impedance",
        summary: "Signal speed on a trace from its effective permittivity, and the copper length a skew budget allows.",
        formula: "tpd = sqrt(eeff) / C0; total = tpd L; allowed_mismatch = skew_budget / tpd.",
        source: "Johnson and Graham, High-Speed Digital Design (1993), ch. 1 (velocity of propagation); C0 = 299792458 m/s (SI).",
        validity: "Quasi-TEM; use the eeff the impedance calculator gave for the same trace.",
        accuracy: "exact for the given eeff",
        inputs: &[
            InputMeta::num("eeff", "", 1.0, 20.0),
            InputMeta::opt("length", "mm", 0.0, 10000.0),
            InputMeta::opt("skew_budget", "ps", 0.0, 1000000.0),
        ],
    },
    CalculatorMeta {
        id: "crosstalk",
        name: "Crosstalk rule of thumb",
        group: "impedance",
        summary: "Rough near-end coupling between two parallel surface traces from their edge gap and dielectric height, plus the 3W spacing.",
        formula: "coupling = 1 / (1 + (s/h)^2);  dB = 20 log10(coupling);  3W gap = 2 w (centre to centre 3 w);  s for -20 dB: s = 3 h.",
        source: "Johnson and Graham, High-Speed Digital Design (1993), section 5.4 crosstalk estimate (the 1/(1+(D/H)^2) form); 3W rule, Mark Montrose, Printed Circuit Board Design Techniques for EMC Compliance (IEEE Press, 2000).",
        validity: "Order of magnitude only; microstrip over one plane; coupled length short against the rise time. Stripline couples less.",
        accuracy: "Rule of thumb; typical real values within a factor of 2.",
        inputs: &[
            InputMeta::num("gap", "mm", 0.01, 100.0),
            InputMeta::num("height", "mm", 0.01, 10.0),
            InputMeta::num("width", "mm", 0.01, 100.0),
        ],
    },
    CalculatorMeta {
        id: "conductor_spacing_ipc2221",
        name: "Conductor spacing by voltage (IPC-2221)",
        group: "spacing",
        summary: "Minimum copper to copper spacing for a peak voltage, by board location class B1 to A7.",
        formula: "Table lookup (V peak, mm) by column B1 internal | B2 external uncoated to 3050 m | B3 external uncoated above 3050 m | B4 external polymer coated | A5 assembly conformal coated | A6 component lead uncoated | A7 component lead coated:\n  <=15:   0.05 0.1  0.1  0.05 0.13 0.13 0.13\n  <=30:   0.05 0.1  0.1  0.05 0.13 0.25 0.13\n  <=50:   0.1  0.6  0.6  0.13 0.13 0.4  0.13\n  <=100:  0.1  0.6  1.5  0.13 0.13 0.5  0.13\n  <=150:  0.2  0.6  3.2  0.4  0.4  0.8  0.4\n  <=170:  0.2  1.25 3.2  0.4  0.4  0.8  0.4\n  <=250:  0.2  1.25 6.4  0.4  0.4  0.8  0.4\n  <=300:  0.2  2.5  12.5 0.4  0.4  0.8  0.8\n  <=500:  0.25 2.5  12.5 0.8  0.8  1.5  0.8\n  >500:   per volt 0.0025 0.005 0.025 0.00305 0.00305 0.00305 0.00305 (spacing = V * slope)\n",
        source: "Reproductions of IPC-2221B table 6-1: Philip McGaw, 'Creepage and clearance' (sources/standards/ipc2221-spacing-philipmcgaw.txt), SFCircuits PCB School (sources/standards/ipc2221-spacing-sfcircuits.txt), Altium high-voltage calculator page (sources/standards/ipc2221-spacing-altium.txt); the standard is paywalled (IPC-2221C, shop.ipc.org).",
        validity: "IPC-2221 is a design standard, not a safety standard; for mains or anything a person can touch use creepage_clearance_iec60664 and the product safety standard. Solder mask does not count as the B4 coating.",
        accuracy: "Table values reproduced identically by three secondary sources; the >500 V slopes are the standard's own linear extension.",
        inputs: &[
            InputMeta::num("voltage", "V peak", 0.0, 100000.0),
            InputMeta::choice("location", &["B1", "B2", "B3", "B4", "A5", "A6", "A7"]),
        ],
    },
    CalculatorMeta {
        id: "creepage_clearance_iec60664",
        name: "Creepage and clearance (IEC 60664-1 lookup)",
        group: "spacing",
        summary: "Minimum clearance through air for the impulse voltage and creepage along the surface for the working voltage, pollution degree and material group.",
        formula: "1. Rated impulse voltage (Table F.1), by mains voltage ceiling (V) and overvoltage category I/II/III/IV in kV:\n   <=50: 0.33 0.5 0.8 1.5 | <=100: 0.5 0.8 1.5 2.5 | <=150: 0.8 1.5 2.5 4.0 | <=300: 1.5 2.5 4.0 6.0 | <=600: 2.5 4.0 6.0 8.0 | <=1000: 4.0 6.0 8.0 12.0\n2. Clearance (Table F.2, inhomogeneous field, up to 2000 m) by impulse kV -> mm:\n   0.33: 0.01 | 0.5: 0.04 | 0.8: 0.1 | 1.5: 0.5 | 2.5: 1.5 | 4.0: 3.0 | 6.0: 5.5 | 8.0: 8.0 | 12: 14\n   pollution degree 2 floor 0.2 mm, degree 3 floor 0.8 mm. Altitude above 2000 m: multiply by the F.8 factor (2000 m 1.00, 3000 m 1.14, 4000 m 1.29, 5000 m 1.48).\n   reinforced: use the impulse voltage one step higher in the F.1 ladder.\n3. Creepage (Table F.5) by working voltage ceiling, pollution degree 1 (all groups) | degree 2 group I, II, III | degree 3 group I, II, III in mm:\n    50: 0.18 | 0.6  0.85 1.2  | 1.5 1.7 1.9\n    63: 0.2  | 0.63 0.9  1.25 | 1.6 1.8 2.0\n    80: 0.22 | 0.67 0.95 1.3  | 1.7 1.9 2.1\n   100: 0.25 | 0.71 1.0  1.4  | 1.8 2.0 2.2\n   125: 0.28 | 0.75 1.05 1.5  | 1.9 2.1 2.4\n   160: 0.32 | 0.8  1.1  1.6  | 2.0 2.2 2.5\n   200: 0.42 | 1.0  1.4  2.0  | 2.5 2.8 3.2\n   250: 0.56 | 1.25 1.8  2.5  | 3.2 3.6 4.0\n   320: 0.75 | 1.6  2.2  3.2  | 4.0 4.5 5.0\n   400: 1.0  | 2.0  2.8  4.0  | 5.0 5.6 6.3\n   500: 1.3  | 2.5  3.6  5.0  | 6.3 7.1 8.0\n   630: 1.8  | 3.2  4.5  6.3  | 8.0 9.0 10.0\n   800: 2.4  | 4.0  5.6  8.0  | 10.0 11.0 12.5\n  1000: 3.2  | 5.0  7.1  10.0 | 12.5 14.0 16.0\n   reinforced creepage = 2 x basic. Creepage is never less than clearance: result = max(creepage, clearance).\n",
        source: "IEC 60664-1:2020 (paywalled, https://webstore.iec.ch/en/publication/59671). Table values from secondary reproductions: PTR Hartmann (sources/standards/creepage-ptr-hartmann.txt), Wurth Elektronik creepage and clearance of connectors PDF, Battery Design creepage calculator, VIOX IEC 60664-1 calculator (sources/standards/creepage-viox-calc.txt), Elektor parts 1 to 3 (sources/standards/creepage-elektor-part*.txt), TI SLUP421 and SLUAAR5 (sources/standards/iec60664-ti-*.pdf). Material group thresholds from IEC 60664-1 clause 4.8.1.3 as quoted by TI SLUAAR5.",
        validity: "Low-voltage equipment up to 1000 V RMS, frequency to 30 kHz. Product standards (IEC 62368-1, 60335, 61010, 60601) override these with their own tables. Secondary reproductions: the implementer must verify every row against the standard before the panel drops the 'verify' label.",
        accuracy: "Lookup; values are the standard's own. The panel shows 'reproduced from secondary sources, verify against IEC 60664-1' until a licensed copy has been checked.",
        inputs: &[
            InputMeta::num("working_voltage", "V RMS", 10.0, 1000.0),
            InputMeta::num("mains_voltage", "V RMS", 50.0, 1000.0),
            InputMeta::choice("overvoltage_category", &["I", "II", "III", "IV"]),
            InputMeta::choice("pollution_degree", &["1", "2", "3"]),
            InputMeta::choice("material_group", &["I", "II", "IIIa", "IIIb"]),
            InputMeta::choice("insulation", &["functional", "basic", "reinforced"]),
            InputMeta::num("altitude", "m", 0.0, 5000.0),
        ],
    },
    CalculatorMeta {
        id: "decoupling_capacitor",
        name: "Decoupling capacitor sizing",
        group: "components",
        summary: "Bulk capacitance for a current step within a ripple budget, and the self-resonant frequency above which a capacitor stops being one.",
        formula: "C = I dt / dV;  f_srf = 1 / (2 pi sqrt(ESL C));  Ztarget = dV / I.",
        source: "Analog Devices MT-101 'Decoupling Techniques' (manifest: https://www.analog.com/media/en/training-seminars/tutorials/MT-101.pdf); the charge-balance form is in any power integrity text (Bogatin, Signal and Power Integrity Simplified, ch. 13).",
        validity: "First-order. Below the SRF the part is a capacitor; above it the mounting inductance dominates, so several values in parallel cover a wider band. Place small ones nearest the pin.",
        accuracy: "Order of magnitude starting point.",
        inputs: &[
            InputMeta::num("current_step", "A", 1e-06, 1000.0),
            InputMeta::num("step_time", "s", 1e-12, 1.0),
            InputMeta::num("ripple", "V", 0.0001, 10.0),
            InputMeta::num("esl", "H", 1e-12, 1e-06),
            InputMeta::opt("capacitance", "F", 1e-12, 1.0),
        ],
    },
    CalculatorMeta {
        id: "i2c_pullup",
        name: "I2C pull-up resistor",
        group: "components",
        summary: "Smallest and largest pull-up for an I2C bus from supply, sink current, rise time budget and bus capacitance.",
        formula: "Rp_min = (Vcc - VOL_max) / IOL;  Rp_max = tr / (0.8473 Cb)  (tr measured 30 % to 70 %, 0.8473 = ln(0.7/0.3)).",
        source: "Texas Instruments SLVA689, 'I2C Bus Pull-Up Resistor Calculation', equations 1 and 6 (sources/netrules/ti-slva689-i2c-pullup-sizing.txt). Bus limits from the NXP I2C-bus specification UM10204 (public).",
        validity: "Open-drain bus; VOL and IOL from the weakest device on the bus.",
        accuracy: "exact for the model",
        inputs: &[
            InputMeta::num("vcc", "V", 1.0, 6.0),
            InputMeta::num("vol_max", "V", 0.0, 1.0),
            InputMeta::num("iol", "A", 0.0001, 0.03),
            InputMeta::num("rise_time", "s", 1e-09, 1e-05),
            InputMeta::num("bus_capacitance", "F", 1e-12, 1e-09),
        ],
    },
    CalculatorMeta {
        id: "crystal_load_capacitors",
        name: "Crystal load capacitors",
        group: "components",
        summary: "The two external capacitors for a Pierce oscillator from the crystal's specified load capacitance and the board stray capacitance.",
        formula: "CL = C1 C2 / (C1 + C2) + Cstray.  For C1 = C2: C1 = C2 = 2 (CL - Cstray).  Check mode returns CL for the fitted pair.",
        source: "Microchip AN826, 'Crystal Oscillator Basics and Crystal Selection', load capacitance section (sources/netrules/microchip-an826-crystal-oscillator-basics.txt); Raspberry Pi 'Hardware design with RP2040' crystal section (sources/netrules/rpi-hardware-design-with-rp2040.txt) as a worked example.",
        validity: "Pierce oscillator with the crystal between two pins. Cstray is a guess until measured; trim by checking frequency.",
        accuracy: "exact for the model; stray guess dominates",
        inputs: &[
            InputMeta::num("load_capacitance", "pF", 4.0, 40.0),
            InputMeta::num("stray_capacitance", "pF", 0.0, 15.0),
            InputMeta::opt("c1", "pF", 1.0, 100.0),
            InputMeta::opt("c2", "pF", 1.0, 100.0),
        ],
    },
    CalculatorMeta {
        id: "timer_555",
        name: "555 timer",
        group: "components",
        summary: "Astable frequency and duty cycle, or monostable pulse width, for the classic timer.",
        formula: "Astable: t_high = ln2 (R1 + R2) C; t_low = ln2 R2 C; f = 1 / (t_high + t_low); D = (R1 + R2) / (R1 + 2 R2). Monostable: t = ln3 R C (1.0986 R C, often rounded to 1.1 R C).",
        source: "Wikipedia, '555 timer IC', design formulas section and worked tables (checked 2026-10-28); any 555 datasheet (TI NE555).",
        validity: "Bipolar or CMOS 555 with R1 >= 1 k; duty cycle is always above 50 % in the basic astable.",
        accuracy: "exact for the model; the chip's comparator thresholds add a few percent",
        inputs: &[
            InputMeta::choice("mode", &["astable", "monostable"]),
            InputMeta::num("r1", "ohm", 1000.0, 10000000.0),
            InputMeta::opt("r2", "ohm", 1000.0, 10000000.0),
            InputMeta::num("c", "F", 1e-12, 0.01),
        ],
    },
    CalculatorMeta {
        id: "opamp_gain",
        name: "Op-amp gain",
        group: "components",
        summary: "Gain of the inverting and non-inverting resistor-feedback amplifiers, in ratio and dB.",
        formula: "Inverting: G = -Rf / Rin. Non-inverting: G = 1 + Rf / Rg. dB = 20 log10 |G|.",
        source: "Horowitz and Hill, 3rd ed., sections 4.2.1 and 4.2.2.",
        validity: "Ideal op-amp, closed-loop gain well below the open-loop gain at the frequency of interest.",
        accuracy: "exact for the model",
        inputs: &[
            InputMeta::choice("topology", &["inverting", "non_inverting"]),
            InputMeta::num("rf", "ohm", 1.0, 1000000000.0),
            InputMeta::num("rin", "ohm", 1.0, 1000000000.0),
        ],
    },
    CalculatorMeta {
        id: "ldo_thermal",
        name: "LDO dissipation and junction temperature",
        group: "components",
        summary: "Heat in a linear regulator and its junction temperature from theta-JA.",
        formula: "P = (Vin - Vout) Iout + Vin Iq;  Tj = Ta + P theta_JA;  Imax = (((Tj_max - Ta)/theta_JA) - Vin Iq) / (Vin - Vout).",
        source: "Any LDO datasheet thermal section, for example TI TLV1117 or the TI application report SLVA118 'Understanding thermal dissipation and design of a heatsink'; theta-JA definition JEDEC JESD51-2.",
        validity: "theta-JA is measured on a JEDEC test board; a real board with less copper runs hotter. Use the datasheet's theta-JA for the package and copper area closest to yours.",
        accuracy: "exact for the model; theta-JA uncertainty 20 to 50 %",
        inputs: &[
            InputMeta::num("vin", "V", 0.5, 100.0),
            InputMeta::num("vout", "V", 0.1, 100.0),
            InputMeta::num("iout", "A", 0.0, 20.0),
            InputMeta::num("iq", "A", 0.0, 0.1),
            InputMeta::num("theta_ja", "K/W", 1.0, 500.0),
            InputMeta::num("ambient", "C", -55.0, 125.0),
            InputMeta::num("tj_max", "C", 85.0, 175.0),
        ],
    },
    CalculatorMeta {
        id: "battery_life",
        name: "Battery life",
        group: "components",
        summary: "Run time from capacity and a duty-cycled load.",
        formula: "Iavg = Ia d + Is (1 - d);  hours = capacity * usable_fraction / Iavg.",
        source: "Charge balance; derating guidance from any cell datasheet (for example Energizer CR2032 datasheet, Panasonic NCR18650B).",
        validity: "Ignores Peukert and the cell's voltage curve; for a regulator between cell and load, use the input current of the regulator.",
        accuracy: "+-20 % in practice",
        inputs: &[
            InputMeta::num("capacity", "mAh", 1.0, 1000000.0),
            InputMeta::num("active_current", "mA", 0.0, 100000.0),
            InputMeta::num("sleep_current", "mA", 0.0, 100000.0),
            InputMeta::num("active_fraction", "", 0.0, 1.0),
            InputMeta::num("usable_fraction", "", 0.1, 1.0),
        ],
    },
    CalculatorMeta {
        id: "wire_gauge",
        name: "Wire gauge ampacity",
        group: "components",
        summary: "Diameter, area, resistance and the usual current limits for an AWG size, or the AWG for a current.",
        formula: "d = 0.127 * 92^((36 - n)/39) mm;  A = pi d^2 / 4;  R/km = RHO_CU / A * 1e9 (A in mm2).\nAmpacity table (A) for 60C/75C/90C insulation, NEC 310.16 single conductors in free air not used; enclosed values:\n  10: 30/35/40  12: 20/25/30  14: 15/20/25  16: 12/16/18  18: 10/14/16  20: 5/11/-  22: 3/7/-  24: 2.1/3.5/-  26: 1.3/2.2/-  28: 0.83/1.4/-  30: 0.52/0.86/-\nawg for current: smallest gauge number whose ampacity at the rating >= current.\n",
        source: "Wikipedia, 'American wire gauge' (formula, table rows and NEC-derived ampacity columns; checked 2026-10-28); ampacity ultimately NFPA 70 (NEC) table 310.16.",
        validity: "Solid copper at 20 C. Ampacity depends on bundling, ambient and insulation; the NEC columns are for building wire. For PCB wire-to-board use the connector's rating.",
        accuracy: "Diameter and area exact from the AWG definition; resistance within 0.3 % of the published table (resistivity rounding).",
        inputs: &[
            InputMeta::opt("awg", "", 0.0, 40.0),
            InputMeta::opt("current", "A", 0.01, 200.0),
            InputMeta::choice("insulation_rating", &["60C", "75C", "90C"]),
        ],
    },
    CalculatorMeta {
        id: "resistor_code",
        name: "Resistor colour code and SMD code",
        group: "components",
        summary: "Decode colour bands (4, 5 or 6) and SMD markings (3-digit, 4-digit, R-notation, EIA-96), or encode a value.",
        formula: "Colour digits: black 0 brown 1 red 2 orange 3 yellow 4 green 5 blue 6 violet 7 grey 8 white 9.\nMultiplier: same digit as power of ten; gold 0.1, silver 0.01, pink 0.001.\nTolerance: brown 1 % (F), red 2 % (G), green 0.5 % (D), blue 0.25 % (C), violet 0.1 % (B), grey 0.01 % (L), gold 5 % (J), silver 10 % (K), none 20 % (M).\nTempco (6th band): brown 100, red 50, orange 15, yellow 25, blue 10, violet 5 ppm/K.\n4 bands = 2 digits + multiplier + tolerance; 5 bands = 3 digits + multiplier + tolerance; 6 = 5 + tempco.\nSMD: 3-digit \"ABC\" = AB * 10^C; 4-digit \"ABCD\" = ABC * 10^D; \"R\" marks the decimal point (4R7 = 4.7, R10 = 0.10).\nEIA-96 \"NNL\": NN is the 1-based index into E96 (01 = 1.00 ... 96 = 9.76, three significant digits), L is the multiplier letter:\n  Z 0.001, Y or R 0.01, X or S 0.1, A 1, B or H 10, C 100, D 1000, E 10000, F 100000.\n",
        source: "Wikipedia, 'Electronic color code' (IEC 60062:2016 table; checked 2026-10-28). EIA-96 letters: xdevs resistor SMD code guide https://xdevs.com/guide/rescode/ and pimylifeup SMD resistor guide https://pimylifeup.com/smd-resistor/ (both agree on Z Y/R X/S A B/H C D E F).",
        validity: "Manufacturers deviate; the decoder shows 'check the datasheet' for grey or white multipliers and for codes with unusual letters.",
        accuracy: "exact decoding",
        inputs: &[
            InputMeta::text("bands", "colour names"),
            InputMeta::text("smd_code", "text"),
            InputMeta::opt("value", "ohm", 0.001, 1000000000000.0),
        ],
    },
    CalculatorMeta {
        id: "e_series",
        name: "E-series nearest value",
        group: "components",
        summary: "Nearest standard value in E6, E12, E24, E48, E96 or E192, the neighbours either side, and the error.",
        formula: "Base values: v_i = round(10^(i/N)) to 2 significant digits for N <= 24, 3 for N >= 48, i = 0..N-1.\nE24 overrides (IEC 60063 keeps the historic values): 2.6->2.7 2.9->3.0 3.2->3.3 3.5->3.6 3.8->3.9 4.2->4.3 4.6->4.7 8.3->8.2.\nE6 and E12 are every 4th and every 2nd E24 value. E192 override: index 185 is 9.20 (formula gives 9.19).\nNearest: minimise |ln(candidate / value)| over the decade's values plus 10 x the first of the next decade.\n",
        source: "Wikipedia, 'E series of preferred numbers' (IEC 60063 lists, formula, the eight E24 exceptions and the E192 9.20 exception; checked 2026-10-28).",
        validity: "Any unit; the series is dimensionless.",
        accuracy: "exact against IEC 60063",
        inputs: &[
            InputMeta::num("value", "ohm | F | H", 1e-15, 1000000000000000.0),
            InputMeta::choice("series", &["E6", "E12", "E24", "E48", "E96", "E192"]),
        ],
    },
    CalculatorMeta {
        id: "unit_converter",
        name: "Unit converter",
        group: "utilities",
        summary: "mil, mm, inch, micron and copper weight in oz/ft2.",
        formula: "1 inch = 25.4 mm exactly; 1 mil = 0.0254 mm; 1 oz/ft2 copper = 34.79 um (fabs round to 35 um; 0.5 oz = 17.4 um, 2 oz = 69.6 um). oz converts only to and from length units as a thickness.",
        source: "Inch definition: NIST SP 811. Copper weight: 1 oz over 1 ft2 of copper (density 8.96 g/cm3) is 34.79 um; stated in IPC-4562 and every fab capability page (sources/standards/jlcpcb-pcb-capabilities.txt, eurocircuits-copper-layers.txt).",
        validity: "Exact conversions.",
        accuracy: "exact",
        inputs: &[
            InputMeta::num("value", "", -1000000000.0, 1000000000.0),
            InputMeta::choice("from", &["mm", "mil", "inch", "um", "oz"]),
            InputMeta::choice("to", &["mm", "mil", "inch", "um", "oz"]),
        ],
    },
    CalculatorMeta {
        id: "thermal_via_count",
        name: "Thermal via count",
        group: "utilities",
        summary: "How many plated vias under a hot pad reach a target thermal resistance to the plane on the other side.",
        formula: "A = pi/4 ((drill + 2 plating)^2 - drill^2)   (filled: pi/4 (drill + 2 plating)^2)\nRth_via = length / (K_CU A);  count = ceil(Rth_via / target_rth);  array_hint = ceil(sqrt(count)).\n",
        source: "Series thermal resistance of a cylinder, k_copper = 385 W/(m K) (CRC Handbook). Context: TI SLVA959 motor driver layout and SLYT614 buck layout notes on thermal vias (sources/netrules/ti-slva959-*.txt, ti-slyt614-*.txt).",
        validity: "Conduction through the barrel only; ignores the FR-4 in parallel (negligible), the spreading resistance in the pad and the plane, and solder wicking. Real packages see 2 to 3 times the ideal count's benefit tail off after about 9 to 16 vias.",
        accuracy: "order of magnitude",
        inputs: &[
            InputMeta::num("drill", "mm", 0.1, 1.0),
            InputMeta::num("plating", "um", 10.0, 50.0),
            InputMeta::num("length", "mm", 0.1, 5.0),
            InputMeta::num("target_rth", "K/W", 0.1, 100.0),
            InputMeta::choice("filled", &["true", "false"]),
        ],
    },
];

/// Look up a calculator by id.
pub fn calculator(id: &str) -> Option<&'static CalculatorMeta> {
    CALCULATORS.iter().find(|c| c.id == id)
}
