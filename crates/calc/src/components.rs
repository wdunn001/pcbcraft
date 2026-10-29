//! Component calculators: decoupling, I2C pull-ups, crystal load caps,
//! the 555 timer, op-amp gain, LDO thermal, battery life, wire gauge,
//! resistor colour and SMD codes, and the E-series lookup everything
//! else leans on for "nearest standard value".
//!
//! `nearest_e_series` replicates the reference tables exactly (the
//! eight historic E24 overrides and the E192 9.20 exception included),
//! because the golden vectors were generated from those tables.

use crate::{Error, finite, int_num, num, opt_num};

/// E-series selector for [`e_series`] and [`nearest_e_series`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Series {
    /// 6 values per decade.
    E6,
    /// 12 values per decade.
    E12,
    /// 24 values per decade.
    #[default]
    E24,
    /// 48 values per decade.
    E48,
    /// 96 values per decade.
    E96,
    /// 192 values per decade.
    E192,
}

impl Series {
    /// Values per decade for this series.
    pub fn count(self) -> usize {
        match self {
            Series::E6 => 6,
            Series::E12 => 12,
            Series::E24 => 24,
            Series::E48 => 48,
            Series::E96 => 96,
            Series::E192 => 192,
        }
    }

    /// Parse a name like "E96"; None for anything else.
    pub fn parse(name: &str) -> Option<Series> {
        match name {
            "E6" => Some(Series::E6),
            "E12" => Some(Series::E12),
            "E24" => Some(Series::E24),
            "E48" => Some(Series::E48),
            "E96" => Some(Series::E96),
            "E192" => Some(Series::E192),
            _ => None,
        }
    }

    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            Series::E6 => "E6",
            Series::E12 => "E12",
            Series::E24 => "E24",
            Series::E48 => "E48",
            Series::E96 => "E96",
            Series::E192 => "E192",
        }
    }
}

/// Round `v` to `digits` decimals with round-half-to-even, matching
/// Python's built-in `round` used by the reference tables.
fn round_to(v: f64, digits: usize) -> f64 {
    let scale = 10f64.powi(digits as i32);
    let scaled = v * scale;
    let floor = scaled.floor();
    let diff = scaled - floor;
    let picked = if diff > 0.5 {
        floor + 1.0
    } else if diff < 0.5 {
        floor
    } else if floor % 2.0 != 0.0 {
        floor + 1.0
    } else {
        floor
    };
    picked / scale
}

/// Generate one decade of an E-series, matching IEC 60063 and the
/// reference tables: banker's rounding to 2 significant digits for
/// N <= 24 and 3 for N >= 48, the eight E24 overrides, E6 / E12 as
/// every 4th / 2nd E24 value, and the E192 index 185 exception.
pub fn e_series_values(count: usize) -> Vec<f64> {
    let digits = if count <= 24 { 1 } else { 2 };
    let mut vals: Vec<f64> = (0..count)
        .map(|i| round_to(10.0f64.powf(i as f64 / count as f64), digits))
        .collect();
    if count == 24 {
        let fixes: [(f64, f64); 8] = [
            (2.6, 2.7),
            (2.9, 3.0),
            (3.2, 3.3),
            (3.5, 3.6),
            (3.8, 3.9),
            (4.2, 4.3),
            (4.6, 4.7),
            (8.3, 8.2),
        ];
        for v in vals.iter_mut() {
            for (from, to) in fixes {
                if (*v - from).abs() < 1e-9 {
                    *v = to;
                }
            }
        }
    }
    if count == 6 || count == 12 {
        let e24 = e_series_values(24);
        let step = 24 / count;
        vals = (0..count).map(|i| e24[i * step]).collect();
    }
    if count == 192 {
        for v in vals.iter_mut() {
            if (*v - 9.19).abs() < 1e-9 {
                *v = 9.20;
            }
        }
    }
    vals
}

/// Nearest standard value in the series with `count` values per decade.
///
/// # Rules
/// - Work on the mantissa inside the value's decade and minimise the
///   absolute log distance; also offer 10 x the first value, so
///   9.9 -> 10 in E24 (decade wrap).
/// - Deterministic: ties keep the earlier candidate (`min` on the
///   first strictly smaller distance).
/// # Dependencies
/// - `crates/calc/src/lib.rs` for [`e_series_values`].
/// # Do not
/// - Feed non-positive values; the logarithm needs value > 0.
pub fn nearest_e_series(value: f64, count: usize) -> f64 {
    let series = e_series_values(count);
    nearest_from(&series, value)
}

/// Nearest of an explicit series list (shared by the calculators).
fn nearest_from(series: &[f64], value: f64) -> f64 {
    let dec = 10f64.powf(value.log10().floor());
    let m = value / dec;
    let mut best = (f64::MAX, 10.0 * dec);
    for v in series {
        let d = (v / m).ln().abs();
        if d < best.0 {
            best = (d, v * dec);
        }
    }
    let d = (10.0 / m).ln().abs();
    if d < best.0 {
        best.1 = 10.0 * dec;
    }
    best.1
}

/// Input for [`e_series`].
#[derive(Debug, Clone, Copy)]
pub struct ESeriesInput {
    /// Target value, 1e-15 to 1e15, any unit.
    pub value: f64,
    /// Series to search; default E24.
    pub series: Series,
}

impl Default for ESeriesInput {
    fn default() -> Self {
        ESeriesInput {
            value: 4500.0,
            series: Series::E24,
        }
    }
}

/// Answer from [`e_series`].
#[derive(Debug, Clone, Copy)]
pub struct ESeriesOut {
    /// Closest standard value to the target.
    pub nearest: f64,
    /// Largest standard value not above the target.
    pub below: f64,
    /// Smallest standard value not below the target.
    pub above: f64,
    /// (nearest - value) / value, as a fraction.
    pub error: f64,
}

/// Nearest standard E-series value plus its neighbours and error.
///
/// # Rules
/// - `below` scans the decade down from the target's, `above` scans
///   up, both by log distance like [`nearest_e_series`].
/// - An exact member returns itself for all three and error 0.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for [`e_series_values`], `nearest_from`, `crate::num`.
/// # Do not
/// - Mix series between the three outputs; one series per call.
pub fn e_series(a: ESeriesInput) -> Result<ESeriesOut, Error> {
    let v = num("value", a.value, 1.0e-15, 1.0e15)?;
    let series = e_series_values(a.series.count());
    let nearest = nearest_from(&series, v);
    let dec = 10f64.powf(v.log10().floor());
    let below = if v >= series[0] * dec {
        nearest_below(&series, v, dec)
    } else {
        nearest_below(&series, v, dec / 10.0)
    };
    let above = nearest_above(&series, v, dec);
    Ok(ESeriesOut {
        nearest,
        below,
        above,
        error: (nearest - v) / v,
    })
}

/// Largest series value (in decade `dec` or the one below) <= v.
fn nearest_below(series: &[f64], v: f64, dec: f64) -> f64 {
    let mut best = f64::MIN;
    for scale in [dec, dec / 10.0] {
        for s in series {
            let c = s * scale;
            if c <= v * (1.0 + 1e-12) && c > best {
                best = c;
            }
        }
    }
    best
}

/// Smallest series value (in decade `dec` or the one above) >= v.
fn nearest_above(series: &[f64], v: f64, dec: f64) -> f64 {
    let mut best = f64::MAX;
    for scale in [dec, dec * 10.0] {
        for s in series {
            let c = s * scale;
            if c >= v * (1.0 - 1e-12) && c < best {
                best = c;
            }
        }
    }
    best
}

/// Input for [`decoupling_capacitor`].
#[derive(Debug, Clone, Copy)]
pub struct Decoupling {
    /// Current step the rail must source, amperes, 1e-6 to 1000.
    pub current_step: f64,
    /// Rise time of the step, seconds, 1e-12 to 1.
    pub step_time: f64,
    /// Allowed ripple, volts, 1e-4 to 10.
    pub ripple: f64,
    /// Capacitor plus mounting inductance, henries, 1e-12 to 1e-6.
    pub esl: f64,
    /// A chosen part, farads, 1e-12 to 1, for its self-resonance.
    pub capacitance: Option<f64>,
}

impl Default for Decoupling {
    fn default() -> Self {
        Decoupling {
            current_step: 1.0,
            step_time: 5.0e-9,
            ripple: 0.05,
            esl: 1.0e-9,
            capacitance: None,
        }
    }
}

/// Answer from [`decoupling_capacitor`].
#[derive(Debug, Clone, Copy)]
pub struct DecouplingOut {
    /// Charge-balance capacitance in farads.
    pub capacitance_needed: f64,
    /// Self-resonance of the chosen part, when one was given.
    pub self_resonance: Option<f64>,
    /// Ripple divided by current step, ohms.
    pub impedance_target: f64,
}

/// Bulk decoupling from a charge balance, plus a part's self-resonance.
///
/// # Rules
/// - C = I dt / dV; f_srf = 1 / (2 pi sqrt(ESL C)); Ztarget = dV / I.
/// - First-order starting point; several values in parallel widen the
///   useful band.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::opt_num`, `crate::finite`.
/// # Do not
/// - Read the SRF as the place the capacitor stops working; the
///   mounting inductance takes over near it.
pub fn decoupling_capacitor(a: Decoupling) -> Result<DecouplingOut, Error> {
    let i = num("current_step", a.current_step, 1.0e-6, 1000.0)?;
    let dt = num("step_time", a.step_time, 1.0e-12, 1.0)?;
    let dv = num("ripple", a.ripple, 1.0e-4, 10.0)?;
    let esl = num("esl", a.esl, 1.0e-12, 1.0e-6)?;
    let c = opt_num("capacitance", a.capacitance, 1.0e-12, 1.0)?;
    let srf = c
        .map(|c| {
            finite(
                "self_resonance",
                1.0 / (2.0 * std::f64::consts::PI * (esl * c).sqrt()),
            )
        })
        .transpose()?;
    Ok(DecouplingOut {
        capacitance_needed: finite("capacitance_needed", i * dt / dv)?,
        self_resonance: srf,
        impedance_target: finite("impedance_target", dv / i)?,
    })
}

/// Input for [`i2c_pullup`].
#[derive(Debug, Clone, Copy)]
pub struct I2cPullup {
    /// Supply voltage, 1 to 6 V.
    pub vcc: f64,
    /// Maximum low-level output voltage, 0 to 1 V; default 0.4.
    pub vol_max: f64,
    /// Sink current the low transistor must handle, amperes; default 3 mA.
    pub iol: f64,
    /// Rise-time budget 10 % to 90 %, seconds, 1e-9 to 1e-5.
    pub rise_time: f64,
    /// Total bus capacitance, farads, 1e-12 to 1e-9.
    pub bus_capacitance: f64,
}

impl Default for I2cPullup {
    fn default() -> Self {
        I2cPullup {
            vcc: 3.3,
            vol_max: 0.4,
            iol: 3.0e-3,
            rise_time: 300.0e-9,
            bus_capacitance: 100.0e-12,
        }
    }
}

/// Answer from [`i2c_pullup`].
#[derive(Debug, Clone, Copy)]
pub struct I2cPullupOut {
    /// Smallest usable pull-up in ohms (sink limit).
    pub rp_min: f64,
    /// Largest usable pull-up in ohms (rise-time limit).
    pub rp_max: f64,
    /// Nearest E24 value to the geometric mean of the limits.
    pub suggested: f64,
}

/// I2C pull-up bounds from sink current and rise time (TI SLVA689).
///
/// # Rules
/// - Rp_min = (Vcc - VOL) / IOL; Rp_max = tr / (0.8473 Cb), the
///   0.8473 being ln(0.7 / 0.3) for the 30 % to 70 % edge.
/// - `suggested` is the nearest E24 value to sqrt(Rp_min Rp_max).
/// # Dependencies
/// - `crates/calc/src/lib.rs` for [`nearest_e_series`], `crate::num`, `crate::finite`.
/// # Do not
/// - Exceed the bus-capacitance limits of the I2C specification;
///   Rp_max assumes the budget and capacitance you supplied are legal.
pub fn i2c_pullup(a: I2cPullup) -> Result<I2cPullupOut, Error> {
    let vcc = num("vcc", a.vcc, 1.0, 6.0)?;
    let vol = num("vol_max", a.vol_max, 0.0, 1.0)?;
    let iol = num("iol", a.iol, 1.0e-4, 0.03)?;
    let tr = num("rise_time", a.rise_time, 1.0e-9, 1.0e-5)?;
    let cb = num("bus_capacitance", a.bus_capacitance, 1.0e-12, 1.0e-9)?;
    let rp_min = finite("rp_min", (vcc - vol) / iol)?;
    let rp_max = finite("rp_max", tr / (0.8473 * cb))?;
    if rp_min > rp_max {
        return Err(Error::Invalid(format!(
            "no usable pull-up: rp_min {rp_min:.1} ohm exceeds rp_max {rp_max:.1} ohm; \
             raise the rise time or lower the bus capacitance"
        )));
    }
    Ok(I2cPullupOut {
        rp_min,
        rp_max,
        suggested: nearest_e_series((rp_min * rp_max).sqrt(), 24),
    })
}

/// Input for [`crystal_load_capacitors`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CrystalCaps {
    /// Crystal load capacitance, 4 to 40 pF.
    pub load_capacitance: f64,
    /// Stray capacitance per side, 0 to 15 pF; default 3.
    pub stray_capacitance: f64,
    /// Check mode: capacitor you fitted, 1 to 100 pF.
    pub c1: Option<f64>,
    /// Check mode: second capacitor, 1 to 100 pF.
    pub c2: Option<f64>,
}

/// Answer from [`crystal_load_capacitors`].
#[derive(Debug, Clone, Copy)]
pub struct CrystalCapsOut {
    /// Equal pair value in pF, when not in check mode.
    pub c1_c2: Option<f64>,
    /// Effective load of the fitted pair in pF, check mode.
    pub effective_load: Option<f64>,
    /// Nearest E24 value to the pair, in pF.
    pub nearest_e24: Option<f64>,
}

/// Pierce-oscillator load capacitors for a specified crystal CL.
///
/// # Rules
/// - CL = C1 C2 / (C1 + C2) + Cstray; the equal pair is
///   C1 = C2 = 2 (CL - Cstray). Give c1 and c2 together for check mode.
/// - Check mode returns the effective load of the fitted pair instead
///   of a pair suggestion.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for [`nearest_e_series`], `crate::num`, `crate::opt_num`.
/// # Do not
/// - Forget the stray estimate; 2 to 5 pF is typical and it shifts CL
///   one-for-one.
pub fn crystal_load_capacitors(a: CrystalCaps) -> Result<CrystalCapsOut, Error> {
    let cl = num("load_capacitance", a.load_capacitance, 4.0, 40.0)?;
    let cs = num("stray_capacitance", a.stray_capacitance, 0.0, 15.0)?;
    let c1 = opt_num("c1", a.c1, 1.0, 100.0)?;
    let c2 = opt_num("c2", a.c2, 1.0, 100.0)?;
    match (c1, c2) {
        (Some(c1), Some(c2)) => Ok(CrystalCapsOut {
            c1_c2: None,
            effective_load: Some(finite("effective_load", c1 * c2 / (c1 + c2) + cs)?),
            nearest_e24: None,
        }),
        _ => {
            let pair = 2.0 * (cl - cs);
            if pair < 1.0 {
                return Err(Error::Invalid(format!(
                    "stray capacitance {cs} pF leaves no room for the pair at CL {cl} pF"
                )));
            }
            Ok(CrystalCapsOut {
                c1_c2: Some(pair),
                effective_load: None,
                nearest_e24: Some(nearest_e_series(pair, 24)),
            })
        }
    }
}

/// Astable or monostable operation of the 555 timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Timer555Mode {
    /// Free-running square-wave oscillator.
    #[default]
    Astable,
    /// One-shot triggered pulse.
    Monostable,
}

/// Input for [`timer_555`].
#[derive(Debug, Clone, Copy)]
pub struct Timer555 {
    /// Which circuit to solve; astable unless set otherwise.
    pub mode: Timer555Mode,
    /// R1 in ohms, 1e3 to 1e7 (R for the monostable).
    pub r1: f64,
    /// R2 in ohms, 1e3 to 1e7; required for astable.
    pub r2: Option<f64>,
    /// Timing capacitor in farads, 1e-12 to 1e-2.
    pub c: f64,
}

impl Default for Timer555 {
    fn default() -> Self {
        Timer555 {
            mode: Timer555Mode::Astable,
            r1: 8200.0,
            r2: Some(68000.0),
            c: 10.0e-9,
        }
    }
}

/// Answer from [`timer_555`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Timer555Out {
    /// Oscillation frequency in hertz, astable.
    pub frequency: Option<f64>,
    /// Duty cycle in percent, astable.
    pub duty_cycle: Option<f64>,
    /// High time in seconds, astable.
    pub t_high: Option<f64>,
    /// Low time in seconds, astable.
    pub t_low: Option<f64>,
    /// Output pulse width in seconds, monostable.
    pub pulse_width: Option<f64>,
}

/// 555 timer astable frequency and duty, or monostable pulse width.
///
/// # Rules
/// - Astable: t_high = ln2 (R1 + R2) C, t_low = ln2 R2 C,
///   f = 1 / (t_high + t_low), duty = (R1 + R2) / (R1 + 2 R2).
/// - Monostable: t = ln3 R C (the usual 1.1 R C rounded).
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::opt_num`, `crate::finite`.
/// # Do not
/// - Expect duty below 50 % from the basic astable; the discharge
///   diode mod is a different circuit.
pub fn timer_555(a: Timer555) -> Result<Timer555Out, Error> {
    let r1 = num("r1", a.r1, 1000.0, 1.0e7)?;
    let c = num("c", a.c, 1.0e-12, 1.0e-2)?;
    if a.mode == Timer555Mode::Monostable {
        return Ok(Timer555Out {
            pulse_width: Some(finite("pulse_width", 3.0f64.ln() * r1 * c)?),
            ..Timer555Out::default()
        });
    }
    let r2 = num(
        "r2",
        a.r2.ok_or_else(|| Error::Invalid("astable needs r2".to_string()))?,
        1000.0,
        1.0e7,
    )?;
    let th = std::f64::consts::LN_2 * (r1 + r2) * c;
    let tl = std::f64::consts::LN_2 * r2 * c;
    Ok(Timer555Out {
        frequency: Some(finite("frequency", 1.0 / (th + tl))?),
        duty_cycle: Some(finite("duty_cycle", (r1 + r2) / (r1 + 2.0 * r2) * 100.0)?),
        t_high: Some(th),
        t_low: Some(tl),
        pulse_width: None,
    })
}

/// Feedback topology for [`opamp_gain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpampTopology {
    /// Inverting amplifier, gain -Rf / Rin.
    #[default]
    Inverting,
    /// Non-inverting amplifier, gain 1 + Rf / Rg.
    NonInverting,
}

/// Input for [`opamp_gain`].
#[derive(Debug, Clone, Copy)]
pub struct OpampGain {
    /// Which amplifier to solve; inverting unless set otherwise.
    pub topology: OpampTopology,
    /// Feedback resistor Rf, 1 ohm to 1e9.
    pub rf: f64,
    /// Input resistor (inverting) or ground resistor, 1 ohm to 1e9.
    pub rin: f64,
}

impl Default for OpampGain {
    fn default() -> Self {
        OpampGain {
            topology: OpampTopology::Inverting,
            rf: 100000.0,
            rin: 10000.0,
        }
    }
}

/// Answer from [`opamp_gain`].
#[derive(Debug, Clone, Copy)]
pub struct OpampGainOut {
    /// Closed-loop gain, signed for the inverting case.
    pub gain: f64,
    /// Gain magnitude in decibels.
    pub gain_db: f64,
    /// Input impedance in ohms, inverting topology only.
    pub input_impedance: Option<f64>,
}

/// Resistor-feedback op-amp gain in ratio and dB.
///
/// # Rules
/// - Inverting G = -Rf / Rin with input impedance Rin;
///   non-inverting G = 1 + Rf / Rg.
/// - dB always uses |G|.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::finite`.
/// # Do not
/// - Push the closed-loop gain near the open-loop gain at the frequency
///   of interest; the ideal model stops holding there.
pub fn opamp_gain(a: OpampGain) -> Result<OpampGainOut, Error> {
    let rf = num("rf", a.rf, 1.0, 1.0e9)?;
    let rin = num("rin", a.rin, 1.0, 1.0e9)?;
    let (gain, zin) = match a.topology {
        OpampTopology::Inverting => (-rf / rin, Some(rin)),
        OpampTopology::NonInverting => (1.0 + rf / rin, None),
    };
    let gain = finite("gain", gain)?;
    Ok(OpampGainOut {
        gain,
        gain_db: finite("gain_db", 20.0 * gain.abs().log10())?,
        input_impedance: zin,
    })
}

/// Input for [`ldo_thermal`].
#[derive(Debug, Clone, Copy)]
pub struct LdoThermal {
    /// Input voltage, 0.5 to 100 V.
    pub vin: f64,
    /// Output voltage, 0.1 to 100 V.
    pub vout: f64,
    /// Load current, 0 to 20 A.
    pub iout: f64,
    /// Quiescent current, 0 to 0.1 A; default 0.
    pub iq: f64,
    /// Junction-to-ambient thermal resistance, 1 to 500 K/W.
    pub theta_ja: f64,
    /// Ambient temperature, -55 to 125 C; default 25.
    pub ambient: f64,
    /// Maximum rated junction temperature, 85 to 175 C; default 125.
    pub tj_max: f64,
}

impl Default for LdoThermal {
    fn default() -> Self {
        LdoThermal {
            vin: 5.0,
            vout: 3.3,
            iout: 0.5,
            iq: 0.0,
            theta_ja: 65.0,
            ambient: 25.0,
            tj_max: 125.0,
        }
    }
}

/// Answer from [`ldo_thermal`].
#[derive(Debug, Clone, Copy)]
pub struct LdoThermalOut {
    /// Total dissipation in watts.
    pub power: f64,
    /// Temperature rise over ambient in kelvin.
    pub rise: f64,
    /// Junction temperature in degrees Celsius.
    pub tj: f64,
    /// Margin to tj_max in kelvin.
    pub headroom: f64,
    /// Load current that reaches tj_max, None if already over.
    pub max_iout_at_tj_max: Option<f64>,
}

/// Linear-regulator dissipation and junction temperature.
///
/// # Rules
/// - P = (Vin - Vout) Iout + Vin Iq; Tj = Ta + P theta_JA;
///   Imax = (((Tjmax - Ta) / theta_JA) - Vin Iq) / (Vin - Vout).
/// - Vin must exceed Vout; equal voltages give no dissipation model.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::finite`.
/// # Do not
/// - Trust theta_JA across copper areas; take the datasheet value for
///   the package and copper closest to the real board.
pub fn ldo_thermal(a: LdoThermal) -> Result<LdoThermalOut, Error> {
    let vin = num("vin", a.vin, 0.5, 100.0)?;
    let vout = num("vout", a.vout, 0.1, 100.0)?;
    let iout = num("iout", a.iout, 0.0, 20.0)?;
    let iq = num("iq", a.iq, 0.0, 0.1)?;
    let theta = num("theta_ja", a.theta_ja, 1.0, 500.0)?;
    let ta = num("ambient", a.ambient, -55.0, 125.0)?;
    let tj_max = num("tj_max", a.tj_max, 85.0, 175.0)?;
    if vin <= vout {
        return Err(Error::Invalid(format!(
            "vin {vin} V must exceed vout {vout} V for an LDO"
        )));
    }
    let p = (vin - vout) * iout + vin * iq;
    let rise = p * theta;
    let tj = ta + rise;
    let imax = ((tj_max - ta) / theta - vin * iq) / (vin - vout);
    Ok(LdoThermalOut {
        power: finite("power", p)?,
        rise: finite("rise", rise)?,
        tj: finite("tj", tj)?,
        headroom: tj_max - tj,
        max_iout_at_tj_max: if imax > 0.0 { Some(imax) } else { None },
    })
}

/// Input for [`battery_life`].
#[derive(Debug, Clone, Copy)]
pub struct BatteryLife {
    /// Cell capacity, 1 to 1e6 mAh.
    pub capacity: f64,
    /// Active-mode current, 0 to 1e5 mA.
    pub active_current: f64,
    /// Sleep current, 0 to 1e5 mA; default 0.
    pub sleep_current: f64,
    /// Fraction of time active, 0 to 1; default 1.
    pub active_fraction: f64,
    /// Usable fraction of capacity, 0.1 to 1; default 0.85.
    pub usable_fraction: f64,
}

impl Default for BatteryLife {
    fn default() -> Self {
        BatteryLife {
            capacity: 2000.0,
            active_current: 100.0,
            sleep_current: 0.0,
            active_fraction: 1.0,
            usable_fraction: 0.85,
        }
    }
}

/// Answer from [`battery_life`].
#[derive(Debug, Clone, Copy)]
pub struct BatteryLifeOut {
    /// Duty-averaged current draw in milliamps.
    pub average_current: f64,
    /// Run time in hours.
    pub hours: f64,
    /// Run time in days.
    pub days: f64,
}

/// Duty-cycled battery run time from capacity and average current.
///
/// # Rules
/// - Iavg = Ia d + Is (1 - d); hours = capacity times usable_fraction
///   divided by Iavg.
/// - A zero average current has no finite run time.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::finite`.
/// # Do not
/// - Use the cell's terminal voltage curve or Peukert effects; this
///   is a charge balance with a derating knob.
pub fn battery_life(a: BatteryLife) -> Result<BatteryLifeOut, Error> {
    let cap = num("capacity", a.capacity, 1.0, 1.0e6)?;
    let ia = num("active_current", a.active_current, 0.0, 1.0e5)?;
    let is = num("sleep_current", a.sleep_current, 0.0, 1.0e5)?;
    let d = num("active_fraction", a.active_fraction, 0.0, 1.0)?;
    let eff = num("usable_fraction", a.usable_fraction, 0.1, 1.0)?;
    let iavg = ia * d + is * (1.0 - d);
    if iavg <= 0.0 {
        return Err(Error::Invalid(
            "average current is zero; the battery never drains in this model".to_string(),
        ));
    }
    let hours = cap * eff / iavg;
    Ok(BatteryLifeOut {
        average_current: iavg,
        hours: finite("hours", hours)?,
        days: hours / 24.0,
    })
}

/// Insulation temperature rating selecting the ampacity column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InsulationRating {
    /// 60 C building-wire column.
    C60,
    /// 75 C building-wire column.
    #[default]
    C75,
    /// 90 C building-wire column.
    C90,
}

/// Ampacity table, amperes per AWG for 60 C / 75 C / 90 C columns;
/// None where the published table has no value.
const AMPACITY: &[(i64, [Option<f64>; 3])] = &[
    (10, [Some(30.0), Some(35.0), Some(40.0)]),
    (12, [Some(20.0), Some(25.0), Some(30.0)]),
    (14, [Some(15.0), Some(20.0), Some(25.0)]),
    (16, [Some(12.0), Some(16.0), Some(18.0)]),
    (18, [Some(10.0), Some(14.0), Some(16.0)]),
    (20, [Some(5.0), Some(11.0), None]),
    (22, [Some(3.0), Some(7.0), None]),
    (24, [Some(2.1), Some(3.5), None]),
    (26, [Some(1.3), Some(2.2), None]),
    (28, [Some(0.83), Some(1.4), None]),
    (30, [Some(0.52), Some(0.86), None]),
];

/// Input for [`wire_gauge`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WireGauge {
    /// AWG size, 0 to 40, integer.
    pub awg: Option<i64>,
    /// Current to carry, 0.01 to 200 A, to find a gauge.
    pub current: Option<f64>,
    /// Insulation rating selecting the ampacity column; default 75C.
    pub insulation_rating: InsulationRating,
}

/// Answer from [`wire_gauge`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WireGaugeOut {
    /// Conductor diameter in millimetres.
    pub diameter: Option<f64>,
    /// Copper cross-section in square millimetres.
    pub area: Option<f64>,
    /// DC resistance in ohms per kilometre at 20 C.
    pub resistance: Option<f64>,
    /// Ampacity in amperes for the chosen rating, when tabulated.
    pub ampacity: Option<f64>,
    /// Smallest gauge (largest ampacity) meeting the current, when one
    /// was asked for.
    pub awg: Option<i64>,
}

/// AWG diameter, area, resistance and ampacity, or the AWG for a current.
///
/// # Rules
/// - d = 0.127 * 92^((36 - n) / 39) mm; A = pi d^2 / 4;
///   R per km = RHO_CU / A * 1e9 with A in mm^2.
/// - For a current, scan gauges 30 up to 10 and take the first whose
///   tabulated ampacity at the rating reaches it.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (RHO_CU), `AMPACITY`, `crate::num`,
///   `crate::int_num`.
/// # Do not
/// - Treat the ampacity as a PCB trace limit; these are building-wire
///   figures and bundling or ambient changes them.
pub fn wire_gauge(a: WireGauge) -> Result<WireGaugeOut, Error> {
    let cur = opt_num("current", a.current, 0.01, 200.0)?;
    let col = match a.insulation_rating {
        InsulationRating::C60 => 0,
        InsulationRating::C75 => 1,
        InsulationRating::C90 => 2,
    };
    let n = match (a.awg, cur) {
        (Some(n), _) => int_num("awg", n, 0, 40)?,
        (None, Some(_)) => {
            let goal = cur.unwrap_or(0.0);
            let mut found = None;
            for (gauge, cols) in AMPACITY.iter().rev() {
                if let Some(cap) = cols[col]
                    && cap >= goal
                {
                    found = Some(*gauge);
                    break;
                }
            }
            match found {
                Some(g) => g,
                None => {
                    return Err(Error::Invalid(format!(
                        "no tabulated gauge to 30 carries {goal} A at this rating"
                    )));
                }
            }
        }
        (None, None) => {
            return Err(Error::Invalid(
                "give an awg size or a current to carry".to_string(),
            ));
        }
    };
    let d = 0.127 * 92.0f64.powf((36.0 - n as f64) / 39.0);
    let area = std::f64::consts::FRAC_PI_4 * d * d;
    let ampacity = AMPACITY
        .iter()
        .find(|(g, _)| *g == n)
        .and_then(|(_, cols)| cols[col]);
    Ok(WireGaugeOut {
        diameter: Some(d),
        area: Some(area),
        resistance: Some(crate::consts::RHO_CU / (area * 1.0e-6) * 1000.0),
        ampacity,
        awg: Some(n),
    })
}

/// Digit value of a colour name, IEC 60062.
fn colour_digit(name: &str) -> Option<u32> {
    match name {
        "black" => Some(0),
        "brown" => Some(1),
        "red" => Some(2),
        "orange" => Some(3),
        "yellow" => Some(4),
        "green" => Some(5),
        "blue" => Some(6),
        "violet" => Some(7),
        "grey" | "gray" => Some(8),
        "white" => Some(9),
        _ => None,
    }
}

/// Multiplier of a colour band; None for tolerance-only colours.
fn colour_multiplier(name: &str) -> Option<f64> {
    match name {
        "gold" => Some(0.1),
        "silver" => Some(0.01),
        "pink" => Some(0.001),
        other => colour_digit(other).map(|d| 10.0f64.powi(d as i32)),
    }
}

/// Tolerance percent of a tolerance band.
fn colour_tolerance(name: &str) -> Option<f64> {
    match name {
        "brown" => Some(1.0),
        "red" => Some(2.0),
        "green" => Some(0.5),
        "blue" => Some(0.25),
        "violet" => Some(0.1),
        "grey" | "gray" => Some(0.01),
        "gold" => Some(5.0),
        "silver" => Some(10.0),
        "none" => Some(20.0),
        _ => None,
    }
}

/// Temperature coefficient ppm/K of a sixth band.
fn colour_tempco(name: &str) -> Option<f64> {
    match name {
        "brown" => Some(100.0),
        "red" => Some(50.0),
        "orange" => Some(15.0),
        "yellow" => Some(25.0),
        "blue" => Some(10.0),
        "violet" => Some(5.0),
        _ => None,
    }
}

/// EIA-96 multiplier letter to its factor.
fn eia96_multiplier(letter: char) -> Option<f64> {
    match letter {
        'Z' => Some(0.001),
        'Y' | 'R' => Some(0.01),
        'X' | 'S' => Some(0.1),
        'A' => Some(1.0),
        'B' | 'H' => Some(10.0),
        'C' => Some(100.0),
        'D' => Some(1000.0),
        'E' => Some(10_000.0),
        'F' => Some(100_000.0),
        _ => None,
    }
}

/// Decode one SMD marking: 3 or 4 digits, R notation, or NN plus letter.
fn decode_smd(code: &str) -> Result<f64, Error> {
    let bytes: Vec<char> = code.chars().collect();
    let bad = Error::Invalid(format!(
        "smd_code {code}: expected 3 or 4 digits, R notation like 4R7, or EIA-96 like 68X"
    ));
    if bytes.len() < 3 || bytes.len() > 4 {
        return Err(bad);
    }
    let all_ok = bytes.iter().enumerate().all(|(k, c)| {
        c.is_ascii_digit()
            || *c == 'R'
            || *c == 'r'
            || (bytes.len() == 3 && k == 2 && c.is_ascii_alphabetic())
    });
    if !all_ok {
        return Err(bad);
    }
    // EIA-96: two digits then a multiplier letter.
    if bytes.len() == 3 && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit() {
        let letter = match bytes[2].to_uppercase().next() {
            Some(c) => c,
            None => return Err(bad),
        };
        if letter.is_ascii_alphabetic() {
            let mult = match eia96_multiplier(letter) {
                Some(m) => m,
                None => {
                    return Err(Error::Invalid(format!(
                        "smd_code {code}: unknown EIA-96 multiplier letter"
                    )));
                }
            };
            let idx = (as_digit_lossy(bytes[0]) * 10 + as_digit_lossy(bytes[1])) as usize;
            if idx == 0 || idx > 96 {
                return Err(Error::Invalid(format!(
                    "smd_code {code}: EIA-96 index must be 01 to 96"
                )));
            }
            let e96 = e_series_values(96);
            let base = (e96[idx - 1] * 100.0).round();
            return Ok(base * mult);
        }
    }
    let r_count = bytes.iter().filter(|c| **c == 'R' || **c == 'r').count();
    if r_count > 1 {
        return Err(Error::Invalid(format!(
            "smd_code {code}: only one R is allowed"
        )));
    }
    if r_count == 1 {
        let pos = match bytes.iter().position(|c| *c == 'R' || *c == 'r') {
            Some(p) => p,
            None => {
                return Err(Error::Invalid(format!(
                    "smd_code {code}: expected 3 or 4 digits, R notation like 4R7, or EIA-96 like 68X"
                )));
            }
        };
        let before: String = bytes[..pos].iter().collect();
        let after: String = bytes[pos + 1..].iter().collect();
        if after.is_empty() || before.len() + after.len() < 2 {
            return Err(bad);
        }
        let whole: f64 = before.parse().unwrap_or(0.0);
        let frac: f64 = after.parse().unwrap_or(0.0);
        return Ok(whole + frac / 10f64.powi(after.len() as i32));
    }
    if bytes[bytes.len() - 1].is_ascii_digit()
        && bytes[..bytes.len() - 1].iter().all(|c| c.is_ascii_digit())
    {
        let exp = as_digit_lossy(bytes[bytes.len() - 1]);
        let mantissa: String = bytes[..bytes.len() - 1].iter().collect();
        let base: f64 = match mantissa.parse() {
            Ok(v) => v,
            Err(_) => return Err(bad),
        };
        return Ok(base * 10f64.powi(exp));
    }
    Err(bad)
}

/// ASCII digit value of a character (input checked by the caller).
fn as_digit_lossy(c: char) -> i32 {
    if c.is_ascii_digit() {
        c as i32 - '0' as i32
    } else {
        0
    }
}

/// Input for [`resistor_code`].
#[derive(Debug, Clone, Default)]
pub struct ResistorCode {
    /// Colour band names, 4, 5 or 6 bands, lower case.
    pub bands: Vec<String>,
    /// SMD marking: "472", "4R7", "68X"...
    pub smd_code: Option<String>,
    /// Encode mode: a resistance in ohms, 1e-3 to 1e12.
    pub value: Option<f64>,
}

/// Answer from [`resistor_code`].
#[derive(Debug, Clone, Default)]
pub struct ResistorCodeOut {
    /// Decoded or encoded resistance in ohms.
    pub resistance: Option<f64>,
    /// Tolerance percent, colour-band decode only.
    pub tolerance: Option<f64>,
    /// Temperature coefficient ppm/K, 6-band decode only.
    pub tempco: Option<f64>,
    /// 3-digit SMD marking of the encoded value.
    pub smd_3digit: Option<String>,
    /// EIA-96 marking of the encoded value.
    pub smd_eia96: Option<String>,
    /// Caveat for unusual markings, for display.
    pub warning: Option<String>,
}

/// Encode a resistance as a 3-digit SMD marking and an EIA-96 code.
fn encode_resistor(value: f64) -> (String, String) {
    let e24 = e_series_values(24);
    let e96 = e_series_values(96);
    let nearest24 = nearest_from(&e24, value);
    let mut exp = (value.log10().floor() - 1.0).round() as i32;
    if exp < 0 {
        exp = 0;
    }
    let scaled = nearest24 / 10f64.powi(exp);
    let digits = scaled.round() as i64;
    let marking = if exp == 0 && nearest24 < 10.0 {
        let t = (nearest24 * 10.0).round() as i64;
        format!("{t}R")
    } else {
        format!("{digits}{exp}")
    };
    let dec = 10f64.powf(value.log10().floor());
    let m = value / dec;
    let mut best = (f64::MAX, 0usize);
    for (i, v) in e96.iter().enumerate() {
        let d = (v / m).ln().abs();
        if d < best.0 {
            best = (d, i);
        }
    }
    let d10 = (10.0 / m).ln().abs();
    let (idx, letter) = if d10 < best.0 {
        (0usize, 'A')
    } else {
        let letter = match exp {
            -3 => 'Z',
            -2 => 'Y',
            -1 => 'X',
            0 => 'A',
            1 => 'B',
            2 => 'C',
            3 => 'D',
            4 => 'E',
            5 => 'F',
            _ => 'A',
        };
        (best.1 + 1, letter)
    };
    (marking, format!("{idx:02}{letter}"))
}

/// Decode resistor colour bands and SMD markings, or encode a value.
///
/// # Rules
/// - Priority bands, then smd_code, then value (encode). Four or five
///   bands give digits, multiplier and tolerance; six add the tempco.
/// - Grey and white multipliers are unusual: the result carries a
///   warning to check the datasheet.
/// - Encode mode rounds to E24 and emits the 3-digit and EIA-96
///   markings.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `colour_digit`, `decode_smd`, `encode_resistor`,
///   [`e_series_values`], `nearest_from`.
/// # Do not
/// - Accept band counts other than 4, 5 or 6; 3-band parts are not a
///   standard code.
pub fn resistor_code(a: ResistorCode) -> Result<ResistorCodeOut, Error> {
    if !a.bands.is_empty() {
        let bands: Vec<String> = a.bands.iter().map(|b| b.trim().to_lowercase()).collect();
        let n = bands.len();
        if !(4..=6).contains(&n) {
            return Err(Error::Invalid(format!(
                "bands: expected 4, 5 or 6 names, got {n}"
            )));
        }
        let digits = n - if n == 6 { 3 } else { 2 };
        let mut mantissa = 0i64;
        for b in &bands[..digits] {
            mantissa = mantissa * 10
                + match colour_digit(b) {
                    Some(d) => d as i64,
                    None => {
                        return Err(Error::Invalid(format!("bands: {b} is not a digit colour")));
                    }
                };
        }
        let mult_name = &bands[digits];
        let mult = match colour_multiplier(mult_name) {
            Some(m) => m,
            None => {
                return Err(Error::Invalid(format!(
                    "bands: {mult_name} is not a multiplier colour"
                )));
            }
        };
        let tol_name = &bands[n - if n == 6 { 2 } else { 1 }];
        let tolerance = match colour_tolerance(tol_name) {
            Some(t) => t,
            None => {
                return Err(Error::Invalid(format!(
                    "bands: {tol_name} is not a tolerance colour"
                )));
            }
        };
        let mut warning = None;
        if mult_name == "grey" || mult_name == "gray" || mult_name == "white" {
            warning = Some(format!(
                "{mult_name} multiplier is unusual; check the datasheet"
            ));
        }
        let tempco = if n == 6 {
            match colour_tempco(&bands[n - 1]) {
                Some(t) => Some(t),
                None => {
                    return Err(Error::Invalid(format!(
                        "bands: {} is not a tempco colour",
                        bands[n - 1]
                    )));
                }
            }
        } else {
            None
        };
        return Ok(ResistorCodeOut {
            resistance: Some(mantissa as f64 * mult),
            tolerance: Some(tolerance),
            tempco,
            warning,
            ..ResistorCodeOut::default()
        });
    }
    if let Some(code) = &a.smd_code {
        let r = decode_smd(code.trim())?;
        return Ok(ResistorCodeOut {
            resistance: Some(r),
            ..ResistorCodeOut::default()
        });
    }
    match a.value {
        Some(v) => {
            let v = num("value", v, 1.0e-3, 1.0e12)?;
            let (smd3, eia) = encode_resistor(v);
            Ok(ResistorCodeOut {
                resistance: Some(v),
                smd_3digit: Some(smd3),
                smd_eia96: Some(eia),
                ..ResistorCodeOut::default()
            })
        }
        None => Err(Error::Invalid(
            "give bands, an smd_code, or a value to encode".to_string(),
        )),
    }
}
