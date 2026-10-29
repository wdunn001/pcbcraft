//! Basic circuit calculators: ohms_law, series_parallel, voltage_divider,
//! led_resistor, rc_rl_time_constant, lc_resonance, reactance, decibel.
//!
//! Formulas, ranges and vectors come from `crates/calc/data/calculators.toml`
//! (ee-thesaurus research). Everything here is ideal-element arithmetic at
//! DC or RMS; validity text in the metadata says where that stops holding.

use crate::{Error, finite, num, opt_num};

/// Input for [`ohms_law`]: exactly two of the three must be present.
#[derive(Debug, Clone, Copy, Default)]
pub struct OhmsLaw {
    /// Voltage in volts, 0 to 1e6.
    pub voltage: Option<f64>,
    /// Current in amperes, 0 to 1e6.
    pub current: Option<f64>,
    /// Resistance in ohms, 1e-9 to 1e12.
    pub resistance: Option<f64>,
}

/// Solved quantities from [`ohms_law`].
#[derive(Debug, Clone, Copy)]
pub struct OhmsLawOut {
    /// Voltage in volts.
    pub voltage: f64,
    /// Current in amperes.
    pub current: f64,
    /// Resistance in ohms.
    pub resistance: f64,
    /// Power in watts.
    pub power: f64,
}

/// Ohm's law and Joule heating from any two of voltage, current, resistance.
///
/// # Rules
/// - Exactly two inputs must be present; the third and the power follow.
/// - A zero denominator (0 V with 0 A, or 0 V with 0 ohm style pairs)
///   comes back as `Error::Invalid`, never as an infinite value.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::opt_num`, `crate::finite`.
/// # Do not
/// - Use for reactive loads; reach for `reactance` instead.
pub fn ohms_law(a: OhmsLaw) -> Result<OhmsLawOut, Error> {
    let v = opt_num("voltage", a.voltage, 0.0, 1.0e6)?;
    let i = opt_num("current", a.current, 0.0, 1.0e6)?;
    let r = opt_num("resistance", a.resistance, 1.0e-9, 1.0e12)?;
    let given = v.is_some() as u8 + i.is_some() as u8 + r.is_some() as u8;
    if given != 2 {
        return Err(Error::Invalid(
            "give exactly two of voltage, current and resistance".to_string(),
        ));
    }
    let (v, i, r) = match (v, i, r) {
        (Some(v), None, Some(r)) => (v, finite("current", v / r)?, r),
        (None, Some(i), Some(r)) => (finite("voltage", i * r)?, i, r),
        (Some(v), Some(i), None) => (v, i, finite("resistance", v / i)?),
        _ => return Err(Error::Invalid("internal pairing".to_string())),
    };
    Ok(OhmsLawOut {
        voltage: v,
        current: i,
        resistance: r,
        power: v * i,
    })
}

/// Which kind of lumped element `series_parallel` combines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    /// Resistor, ohms.
    Resistor,
    /// Capacitor, farads.
    Capacitor,
    /// Inductor, henries.
    Inductor,
}

/// Input for [`series_parallel`].
#[derive(Debug, Clone)]
pub struct SeriesParallel {
    /// Element kind; capacitors combine in the reverse direction.
    pub kind: ComponentKind,
    /// Two to 16 values in the kind's unit, each 1e-15 to 1e12.
    pub values: Vec<f64>,
}

/// Series and parallel combination from [`series_parallel`].
#[derive(Debug, Clone, Copy)]
pub struct SeriesParallelOut {
    /// Combined value in series.
    pub series: f64,
    /// Combined value in parallel.
    pub parallel: f64,
}

/// Combine 2 to 16 ideal resistors, capacitors or inductors.
///
/// # Rules
/// - R and L: series is the sum, parallel is the reciprocal sum.
///   C is the reverse. Values must be inside 1e-15 to 1e12.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::Error`.
/// # Do not
/// - Pass fewer than two or more than 16 values; the call is refused.
pub fn series_parallel(a: SeriesParallel) -> Result<SeriesParallelOut, Error> {
    if a.values.len() < 2 || a.values.len() > 16 {
        return Err(Error::Invalid(format!(
            "values must hold 2 to 16 entries, got {}",
            a.values.len()
        )));
    }
    let mut vals = Vec::with_capacity(a.values.len());
    for v in &a.values {
        vals.push(num("values", *v, 1.0e-15, 1.0e12)?);
    }
    let sum: f64 = vals.iter().sum();
    let recip: f64 = vals.iter().map(|v| 1.0 / v).sum();
    let inv = finite("parallel", 1.0 / recip)?;
    let (series, parallel) = match a.kind {
        ComponentKind::Resistor | ComponentKind::Inductor => (sum, inv),
        ComponentKind::Capacitor => (inv, sum),
    };
    Ok(SeriesParallelOut { series, parallel })
}

/// Input for [`voltage_divider`].
#[derive(Debug, Clone, Copy, Default)]
pub struct VoltageDivider {
    /// Input voltage in volts, 0 to 1e5.
    pub vin: f64,
    /// Top resistor in ohms, 1e-3 to 1e12.
    pub r1: f64,
    /// Bottom resistor in ohms, 1e-3 to 1e12.
    pub r2: f64,
    /// Optional load resistor across the output, 1e-3 to 1e12.
    pub load: Option<f64>,
}

/// Solved quantities from [`voltage_divider`].
#[derive(Debug, Clone, Copy)]
pub struct VoltageDividerOut {
    /// Output voltage in volts.
    pub vout: f64,
    /// Current through r1 in amperes.
    pub current: f64,
    /// r2 in parallel with the load, ohms.
    pub r2_effective: f64,
    /// Power in r1, watts.
    pub power_r1: f64,
    /// Power in the effective bottom resistor, watts.
    pub power_r2: f64,
}

/// Two-resistor divider with Thevenin loading from an optional load.
///
/// # Rules
/// - With a load, r2 is replaced by r2 || load before dividing.
/// - Power in r2 is computed with the effective bottom resistance.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::opt_num`, `crate::finite`.
/// # Do not
/// - Model a non-resistive load; the model is a resistor to ground.
pub fn voltage_divider(a: VoltageDivider) -> Result<VoltageDividerOut, Error> {
    let vin = num("vin", a.vin, 0.0, 1.0e5)?;
    let r1 = num("r1", a.r1, 1.0e-3, 1.0e12)?;
    let r2 = num("r2", a.r2, 1.0e-3, 1.0e12)?;
    let load = opt_num("load", a.load, 1.0e-3, 1.0e12)?;
    let r2e = match load {
        Some(rl) => finite("r2_effective", 1.0 / (1.0 / r2 + 1.0 / rl))?,
        None => r2,
    };
    let i = finite("current", vin / (r1 + r2e))?;
    Ok(VoltageDividerOut {
        vout: i * r2e,
        current: i,
        r2_effective: r2e,
        power_r1: i * i * r1,
        power_r2: i * i * r2e,
    })
}

/// E-series used by [`led_resistor`] for its nearest standard value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ESeries {
    /// 6 values per decade.
    E6,
    /// 12 values per decade.
    E12,
    /// 24 values per decade.
    #[default]
    E24,
    /// 96 values per decade.
    E96,
}

impl ESeries {
    /// Values per decade for this series.
    pub fn count(self) -> usize {
        match self {
            ESeries::E6 => 6,
            ESeries::E12 => 12,
            ESeries::E24 => 24,
            ESeries::E96 => 96,
        }
    }
}

/// Input for [`led_resistor`].
#[derive(Debug, Clone, Copy, Default)]
pub struct LedResistor {
    /// Supply voltage in volts, 0.1 to 1000.
    pub supply: f64,
    /// Total LED forward voltage in volts, 0.1 to 50.
    pub forward_voltage: f64,
    /// Wanted current in amperes, 1e-6 to 10.
    pub current: f64,
    /// Series for the nearest standard value; defaults to E24.
    pub series: ESeries,
}

/// Resistor, power and nearest standard value from [`led_resistor`].
#[derive(Debug, Clone, Copy)]
pub struct LedResistorOut {
    /// Exact resistor value in ohms.
    pub resistance: f64,
    /// Resistor power in watts.
    pub power: f64,
    /// Nearest value in the chosen E-series, ohms.
    pub nearest_standard: f64,
    /// Current actually flowing with the standard value, amperes.
    pub current_with_standard: f64,
}

/// Series resistor for an LED from supply, forward drop and current.
///
/// # Rules
/// - Refuses when forward_voltage is at or above supply (no headroom).
/// - The nearest standard value uses log distance in `e_series`.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::finite`, `crate::components::nearest_e_series`.
/// # Do not
/// - Treat the result as final; forward voltage moves with current.
pub fn led_resistor(a: LedResistor) -> Result<LedResistorOut, Error> {
    let vs = num("supply", a.supply, 0.1, 1000.0)?;
    let vf = num("forward_voltage", a.forward_voltage, 0.1, 50.0)?;
    let i = num("current", a.current, 1.0e-6, 10.0)?;
    if vf >= vs {
        return Err(Error::Invalid(format!(
            "forward_voltage {vf} must be below supply {vs} or no resistor value works"
        )));
    }
    let drop = vs - vf;
    let r = finite("resistance", drop / i)?;
    let std = crate::components::nearest_e_series(r, a.series.count());
    Ok(LedResistorOut {
        resistance: r,
        power: drop * i,
        nearest_standard: std,
        current_with_standard: drop / std,
    })
}

/// Which first-order network `rc_rl_time_constant` solves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RcRlKind {
    /// Resistor and capacitor.
    #[default]
    Rc,
    /// Resistor and inductor.
    Rl,
}

/// Input for [`rc_rl_time_constant`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RcRl {
    /// Network kind.
    pub kind: RcRlKind,
    /// Resistance in ohms, 1e-3 to 1e12.
    pub resistance: f64,
    /// Capacitance in farads, 1e-15 to 1; required for rc.
    pub capacitance: Option<f64>,
    /// Inductance in henries, 1e-12 to 1e3; required for rl.
    pub inductance: Option<f64>,
}

/// Time constant and corner frequency from [`rc_rl_time_constant`].
#[derive(Debug, Clone, Copy)]
pub struct RcRlOut {
    /// Time constant in seconds.
    pub tau: f64,
    /// Minus 3 dB corner frequency in hertz.
    pub cutoff: f64,
    /// Time to 63 percent, one tau, seconds.
    pub t_63: f64,
    /// Time to 95 percent, three tau, seconds.
    pub t_95: f64,
    /// Time to 99 percent, five tau, seconds.
    pub t_99: f64,
}

/// Single-pole time constant and cutoff for an rc or rl network.
///
/// # Rules
/// - rc needs capacitance, rl needs inductance; the other is ignored.
/// - tau = R C or L / R; cutoff = 1 / (2 pi tau).
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::opt_num`, `crate::finite`.
/// # Do not
/// - Apply to multi-pole networks; only the single-pole corner is modelled.
pub fn rc_rl_time_constant(a: RcRl) -> Result<RcRlOut, Error> {
    let r = num("resistance", a.resistance, 1.0e-3, 1.0e12)?;
    let tau = match a.kind {
        RcRlKind::Rc => {
            let c = opt_num("capacitance", a.capacitance, 1.0e-15, 1.0)?
                .ok_or_else(|| Error::Invalid("rc needs a capacitance".to_string()))?;
            r * c
        }
        RcRlKind::Rl => {
            let l = opt_num("inductance", a.inductance, 1.0e-12, 1.0e3)?
                .ok_or_else(|| Error::Invalid("rl needs an inductance".to_string()))?;
            l / r
        }
    };
    let tau = finite("tau", tau)?;
    let cutoff = finite("cutoff", 1.0 / (2.0 * std::f64::consts::PI * tau))?;
    Ok(RcRlOut {
        tau,
        cutoff,
        t_63: tau,
        t_95: 3.0 * tau,
        t_99: 5.0 * tau,
    })
}

/// Input for [`lc_resonance`]: exactly two of the three.
#[derive(Debug, Clone, Copy, Default)]
pub struct LcResonance {
    /// Inductance in henries, 1e-12 to 1e3.
    pub inductance: Option<f64>,
    /// Capacitance in farads, 1e-15 to 1.
    pub capacitance: Option<f64>,
    /// Resonant frequency in hertz, 1e-3 to 1e12.
    pub frequency: Option<f64>,
}

/// Solved quantities from [`lc_resonance`].
#[derive(Debug, Clone, Copy)]
pub struct LcResonanceOut {
    /// Resonant frequency in hertz.
    pub frequency: f64,
    /// Inductance in henries.
    pub inductance: f64,
    /// Capacitance in farads.
    pub capacitance: f64,
    /// sqrt(L / C) in ohms, present only when both L and C are known.
    pub characteristic_impedance: Option<f64>,
}

/// Lossless LC resonance; solves whichever of L, C, f is missing.
///
/// # Rules
/// - Exactly two inputs must be present.
/// - characteristic_impedance is Some only when both L and C are known.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::opt_num`, `crate::finite`.
/// # Do not
/// - Expect the real part's self-resonance or parasitics here.
pub fn lc_resonance(a: LcResonance) -> Result<LcResonanceOut, Error> {
    let l = opt_num("inductance", a.inductance, 1.0e-12, 1.0e3)?;
    let c = opt_num("capacitance", a.capacitance, 1.0e-15, 1.0)?;
    let f = opt_num("frequency", a.frequency, 1.0e-3, 1.0e12)?;
    let given = l.is_some() as u8 + c.is_some() as u8 + f.is_some() as u8;
    if given != 2 {
        return Err(Error::Invalid(
            "give exactly two of inductance, capacitance and frequency".to_string(),
        ));
    }
    let pi2 = 2.0 * std::f64::consts::PI;
    let (l, c, f, z0) = match (l, c, f) {
        (Some(l), Some(c), None) => {
            let f = finite("frequency", 1.0 / (pi2 * (l * c).sqrt()))?;
            (
                l,
                c,
                f,
                Some(finite("characteristic_impedance", (l / c).sqrt())?),
            )
        }
        (Some(l), None, Some(f)) => {
            let c = finite("capacitance", 1.0 / (pi2 * f * (pi2 * f) * l))?;
            (l, c, f, None)
        }
        (None, Some(c), Some(f)) => {
            let l = finite("inductance", 1.0 / (pi2 * f * (pi2 * f) * c))?;
            (l, c, f, None)
        }
        _ => return Err(Error::Invalid("internal pairing".to_string())),
    };
    Ok(LcResonanceOut {
        frequency: f,
        inductance: l,
        capacitance: c,
        characteristic_impedance: z0,
    })
}

/// Input for [`reactance`]: frequency plus at least one of C and L.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reactance {
    /// Frequency in hertz, 1e-3 to 1e12.
    pub frequency: f64,
    /// Capacitance in farads, 1e-15 to 1.
    pub capacitance: Option<f64>,
    /// Inductance in henries, 1e-12 to 1e3.
    pub inductance: Option<f64>,
}

/// Reactances from [`reactance`]; absent parts come back as None.
#[derive(Debug, Clone, Copy)]
pub struct ReactanceOut {
    /// Capacitive reactance in ohms, negative in the phasor convention.
    pub xc: Option<f64>,
    /// Inductive reactance in ohms.
    pub xl: Option<f64>,
}

/// Ideal reactance of a capacitor or inductor at one frequency.
///
/// # Rules
/// - Xc = 1 / (2 pi f C); Xl = 2 pi f L. Magnitudes are reported.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::opt_num`, `crate::finite`.
/// # Do not
/// - Use near a part's self-resonance; ESR and SRF are not modelled.
pub fn reactance(a: Reactance) -> Result<ReactanceOut, Error> {
    let f = num("frequency", a.frequency, 1.0e-3, 1.0e12)?;
    let c = opt_num("capacitance", a.capacitance, 1.0e-15, 1.0)?;
    let l = opt_num("inductance", a.inductance, 1.0e-12, 1.0e3)?;
    if c.is_none() && l.is_none() {
        return Err(Error::Invalid(
            "give a capacitance or an inductance".to_string(),
        ));
    }
    let pi2 = 2.0 * std::f64::consts::PI;
    Ok(ReactanceOut {
        xc: c.map(|c| finite("xc", 1.0 / (pi2 * f * c))).transpose()?,
        xl: l.map(|l| finite("xl", pi2 * f * l)).transpose()?,
    })
}

/// Which conversion `decibel` performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecibelMode {
    /// Power ratio to dB.
    PowerRatio,
    /// Voltage ratio to dB (equal impedances).
    VoltageRatio,
    /// Watts to dBm.
    WToDbm,
    /// dBm to watts.
    DbmToW,
}

/// Input for [`decibel`].
#[derive(Debug, Clone, Copy)]
pub struct Decibel {
    /// Conversion to run.
    pub mode: DecibelMode,
    /// Input: a ratio, watts or dBm depending on the mode.
    pub value: f64,
}

/// Converted quantities from [`decibel`]; irrelevant fields are None.
#[derive(Debug, Clone, Copy, Default)]
pub struct DecibelOut {
    /// Decibels, for the ratio modes.
    pub db: Option<f64>,
    /// Linear ratio, for the ratio modes.
    pub ratio: Option<f64>,
    /// dBm, for the dBm modes.
    pub dbm: Option<f64>,
    /// Watts, for the dBm modes.
    pub watts: Option<f64>,
}

/// Convert between ratios, dB, dBm and watts.
///
/// # Rules
/// - Ratio modes need a positive value; dBm modes accept down to -300.
/// - dBm = 10 log10(P / 1 mW); P = 1 mW times 10^(dBm / 10).
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::finite`, `crate::Error`.
/// # Do not
/// - Mix the voltage form with unequal source and load impedances.
pub fn decibel(a: Decibel) -> Result<DecibelOut, Error> {
    let v = num("value", a.value, -300.0, 1.0e12)?;
    match a.mode {
        DecibelMode::PowerRatio | DecibelMode::VoltageRatio => {
            if v <= 0.0 {
                return Err(Error::Invalid(format!(
                    "value must be above 0 for a ratio, got {v}"
                )));
            }
            let factor = if a.mode == DecibelMode::PowerRatio {
                10.0
            } else {
                20.0
            };
            Ok(DecibelOut {
                db: Some(finite("db", factor * v.log10())?),
                ratio: Some(v),
                ..DecibelOut::default()
            })
        }
        DecibelMode::WToDbm => Ok(DecibelOut {
            dbm: Some(finite("dbm", 10.0 * (v * 1.0e3).log10())?),
            watts: Some(v),
            ..DecibelOut::default()
        }),
        DecibelMode::DbmToW => Ok(DecibelOut {
            watts: Some(finite("watts", 1.0e-3 * 10.0f64.powf(v / 10.0))?),
            dbm: Some(v),
            ..DecibelOut::default()
        }),
    }
}
