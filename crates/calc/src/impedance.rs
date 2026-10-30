//! Impedance and signal-integrity calculators: microstrip,
//! embedded_microstrip, stripline, coplanar_waveguide, differential_pair,
//! propagation_delay, crosstalk.
//!
//! The microstrip family is one Hammerstad-Jensen formula, continuous in
//! w / h, replacing the old two-branch estimate that jumped about 12 ohm
//! at w = h (see research-283-length-impedance.md). Models are labelled
//! exact, model or estimate in the metadata; the UI must show that label.

use crate::consts::{C0, ZF0};
use crate::{Error, finite, num, opt_num};

/// Hammerstad-Jensen zero-thickness transmission line function ZL1(u).
fn zl1(u: f64) -> f64 {
    let f = 6.0 + (2.0 * std::f64::consts::PI - 6.0) * (-(30.666 / u).powf(0.7528)).exp();
    ZF0 / (2.0 * std::f64::consts::PI) * (f / u + (1.0 + (2.0 / u).powi(2)).sqrt()).ln()
}

/// Hammerstad-Jensen effective permittivity at ratio u for er.
fn eeff(u: f64, er: f64) -> f64 {
    let a = 1.0
        + ((u.powi(4) + (u / 52.0).powi(2)) / (u.powi(4) + 0.432)).ln() / 49.0
        + (1.0 + (u / 18.1).powi(3)).ln() / 18.7;
    let b = 0.564 * ((er - 0.9) / (er + 3.0)).powf(0.053);
    (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 10.0 / u).powf(-a * b)
}

/// Core Hammerstad-Jensen result: (z0, eeff with the thickness correction).
fn hj(w: f64, h: f64, er: f64, t: f64) -> (f64, f64) {
    let u = w / h;
    let (u1, ur) = if t > 0.0 {
        let tn = t / h;
        let th = (6.517 * u).sqrt().tanh();
        let du1 = tn / std::f64::consts::PI * (1.0 + 4.0 * std::f64::consts::E / tn * th * th).ln();
        let dur = du1 * (1.0 + 1.0 / (er - 1.0).sqrt().cosh()) / 2.0;
        (u + du1, u + dur)
    } else {
        (u, u)
    };
    let e = eeff(ur, er);
    let z = zl1(ur) / e.sqrt();
    let e_t = e * (zl1(u1) / zl1(ur)).powi(2);
    (z, e_t)
}

/// Delay in ps per mm for an effective permittivity.
fn delay_ps_mm(eeff: f64) -> f64 {
    eeff.sqrt() / C0 * 1.0e9
}

/// Input for [`microstrip`].
#[derive(Debug, Clone, Copy)]
pub struct Microstrip {
    /// Trace width in millimetres, 0.01 to 100; omit only for a target solve.
    pub width: Option<f64>,
    /// Dielectric height in millimetres, 0.01 to 10.
    pub height: f64,
    /// Relative permittivity, 1 to 20.
    pub er: f64,
    /// Copper thickness in millimetres, 0 to 0.5; default 0.035.
    pub thickness: f64,
    /// Optional target impedance in ohms, 1 to 1000.
    pub target: Option<f64>,
}

impl Default for Microstrip {
    fn default() -> Self {
        Microstrip {
            width: None,
            height: 1.6,
            er: 4.5,
            thickness: 0.035,
            target: None,
        }
    }
}

/// Answer from [`microstrip`].
#[derive(Debug, Clone, Copy)]
pub struct MicrostripOut {
    /// Characteristic impedance in ohms, when a width was given.
    pub z0: Option<f64>,
    /// Effective permittivity including the thickness correction.
    pub eeff: Option<f64>,
    /// Delay in picoseconds per millimetre, when a width was given.
    pub delay: Option<f64>,
    /// Width meeting the target, when a target was given.
    pub width_for_target: Option<f64>,
}

/// Surface microstrip impedance, one formula continuous across w / h = 1.
///
/// # Rules
/// - Needs a width, a target, or both; the target solves width by
///   bisection on 0.01 to 100 mm, 80 steps.
/// - Hammerstad-Jensen 1980 with the Hammerstad thickness correction.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (C0, ZF0), `crate::num`, `crate::finite`.
/// # Do not
/// - Add solder mask; it lowers Z0 by 1 to 3 ohm and is not modelled.
pub fn microstrip(a: Microstrip) -> Result<MicrostripOut, Error> {
    let w = opt_num("width", a.width, 0.01, 100.0)?;
    let h = num("height", a.height, 0.01, 10.0)?;
    let er = num("er", a.er, 1.0, 20.0)?;
    let t = num("thickness", a.thickness, 0.0, 0.5)?;
    let target = opt_num("target", a.target, 1.0, 1000.0)?;
    if w.is_none() && target.is_none() {
        return Err(Error::Invalid(
            "give a width, a target impedance, or both".to_string(),
        ));
    }
    let (z0, eeff, delay) = match w {
        Some(w) => {
            let (z, e) = hj(w, h, er, t);
            (Some(finite("z0", z)?), Some(e), Some(delay_ps_mm(e)))
        }
        None => (None, None, None),
    };
    let width_for_target = match target {
        Some(goal) => {
            let (mut lo, mut hi) = (0.01f64, 100.0f64);
            for _ in 0..80 {
                let mid = (lo + hi) / 2.0;
                if hj(mid, h, er, t).0 > goal {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            Some((lo + hi) / 2.0)
        }
        None => None,
    };
    Ok(MicrostripOut {
        z0,
        eeff,
        delay,
        width_for_target,
    })
}

/// Input for [`embedded_microstrip`].
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedMicrostrip {
    /// Trace width in millimetres, 0.01 to 100.
    pub width: f64,
    /// Dielectric height below the trace in millimetres, 0.01 to 10.
    pub height: f64,
    /// Dielectric covering the trace top in millimetres, 0 to 10.
    pub cover: f64,
    /// Relative permittivity, 1 to 20.
    pub er: f64,
    /// Copper thickness in millimetres, 0 to 0.5; default 0.035.
    pub thickness: f64,
}

impl Default for EmbeddedMicrostrip {
    fn default() -> Self {
        EmbeddedMicrostrip {
            width: 1.0,
            height: 1.6,
            cover: 0.0,
            er: 4.5,
            thickness: 0.035,
        }
    }
}

/// Impedance and effective permittivity from [`embedded_microstrip`].
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedMicrostripOut {
    /// Characteristic impedance in ohms.
    pub z0: f64,
    /// Effective permittivity with the cover.
    pub eeff: f64,
}

/// Surface microstrip impedance corrected for a dielectric cover layer.
///
/// # Rules
/// - Start from the surface Hammerstad-Jensen result, then blend the
///   effective permittivity toward er with weight 1 - exp(-2 cover / h).
/// - Engineering estimate, about 5 %; the metadata accuracy label says
///   so and the UI must repeat it.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for [`microstrip`]'s helpers `hj` and `crate::num`.
/// # Do not
/// - Use for cover >= 2 h; a stripline model is better there.
pub fn embedded_microstrip(a: EmbeddedMicrostrip) -> Result<EmbeddedMicrostripOut, Error> {
    let w = num("width", a.width, 0.01, 100.0)?;
    let h = num("height", a.height, 0.01, 10.0)?;
    let cover = num("cover", a.cover, 0.0, 10.0)?;
    let er = num("er", a.er, 1.0, 20.0)?;
    let t = num("thickness", a.thickness, 0.0, 0.5)?;
    let (z_s, e_s) = hj(w, h, er, t);
    let f = (-2.0 * cover / h).exp();
    let e_e = finite("eeff", e_s * f + er * (1.0 - f))?;
    let z0 = finite("z0", z_s * (e_s / e_e).sqrt())?;
    Ok(EmbeddedMicrostripOut { z0, eeff: e_e })
}

/// Input for [`stripline`].
#[derive(Debug, Clone, Copy)]
pub struct Stripline {
    /// Trace width in millimetres, 0.01 to 100.
    pub width: f64,
    /// Plane-to-plane distance in millimetres, 0.02 to 20.
    pub spacing: f64,
    /// Relative permittivity, 1 to 20.
    pub er: f64,
    /// Copper thickness in millimetres, 0 to 0.5; default 0.018.
    pub thickness: f64,
}

impl Default for Stripline {
    fn default() -> Self {
        Stripline {
            width: 0.2,
            spacing: 0.6,
            er: 4.2,
            thickness: 0.018,
        }
    }
}

/// Impedance and delay from [`stripline`].
#[derive(Debug, Clone, Copy)]
pub struct StriplineOut {
    /// Characteristic impedance in ohms.
    pub z0: f64,
    /// Delay in picoseconds per millimetre (eeff = er, fully embedded).
    pub delay: f64,
}

/// Stripline impedance, Steer's closed form with trace thickness.
///
/// # Rules
/// - Trace centred between two planes; W = w / b, T = t / b.
/// - Effective width shrinks by (0.35 - W)^2 / (1 + 12 T) only for
///   W < 0.35; the flat-plate term Cf corrects the finite thickness.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (C0), `crate::num`, `crate::finite`.
/// # Do not
/// - Use for an offset stripline; the symmetric form is all this models.
pub fn stripline(a: Stripline) -> Result<StriplineOut, Error> {
    let w = num("width", a.width, 0.01, 100.0)?;
    let b = num("spacing", a.spacing, 0.02, 20.0)?;
    let er = num("er", a.er, 1.0, 20.0)?;
    let t = num("thickness", a.thickness, 0.0, 0.5)?;
    let (big_w, tt) = (w / b, t / b);
    let weff = if big_w < 0.35 {
        big_w - (0.35 - big_w).powi(2) / (1.0 + 12.0 * tt)
    } else {
        big_w
    };
    let one_m_t = 1.0 - tt;
    let cf = 2.0 / std::f64::consts::PI * (1.0 / one_m_t + 1.0).ln()
        - tt / std::f64::consts::PI * ((1.0 / one_m_t).powi(2) - 1.0).ln();
    let z0 = finite(
        "z0",
        30.0 * std::f64::consts::PI / er.sqrt() * one_m_t / (weff + cf),
    )?;
    Ok(StriplineOut {
        z0,
        delay: delay_ps_mm(er),
    })
}

/// Elliptic integral ratio K(k) / K'(k) by the Hilberg approximation.
fn kk(k: f64) -> f64 {
    let kp = (1.0 - k * k).sqrt();
    if k <= std::f64::consts::FRAC_1_SQRT_2 {
        std::f64::consts::PI / (2.0 * (1.0 + kp.sqrt()) / (1.0 - kp.sqrt())).ln()
    } else {
        (2.0 * (1.0 + k.sqrt()) / (1.0 - k.sqrt())).ln() / std::f64::consts::PI
    }
}

/// Input for [`coplanar_waveguide`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CoplanarWaveguide {
    /// Centre trace width in millimetres, 0.01 to 100.
    pub width: f64,
    /// Gap from trace edge to side ground in millimetres, 0.01 to 100.
    pub gap: f64,
    /// Substrate height in millimetres, 0.01 to 10.
    pub height: f64,
    /// Relative permittivity, 1 to 20.
    pub er: f64,
    /// True for ground-backed CPW (a plane under the substrate,
    /// side grounds tied to it with vias).
    pub ground_backed: bool,
}

/// Impedance, permittivity and delay from [`coplanar_waveguide`].
#[derive(Debug, Clone, Copy)]
pub struct CoplanarWaveguideOut {
    /// Characteristic impedance in ohms.
    pub z0: f64,
    /// Effective permittivity.
    pub eeff: f64,
    /// Delay in picoseconds per millimetre.
    pub delay: f64,
}

/// Coplanar waveguide impedance, quasi-static conformal mapping.
///
/// # Rules
/// - Zero-thickness model; copper thickness is deliberately ignored.
/// - Ground-backed mode uses the tanh ratio k3 and adds its weight q
///   to the permittivity; ungrounded mode uses the sinh ratio k2.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `kk` (Hilberg ratio), `delay_ps_mm`, `crate::num`.
/// # Do not
/// - Assume narrow side grounds; the formula needs grounds much wider
///   than w + 2s.
pub fn coplanar_waveguide(a: CoplanarWaveguide) -> Result<CoplanarWaveguideOut, Error> {
    let w = num("width", a.width, 0.01, 100.0)?;
    let s = num("gap", a.gap, 0.01, 100.0)?;
    let h = num("height", a.height, 0.01, 10.0)?;
    let er = num("er", a.er, 1.0, 20.0)?;
    let pi = std::f64::consts::PI;
    let k1 = w / (w + 2.0 * s);
    let (z0, eeff) = if a.ground_backed {
        let k3 = (pi * w / (4.0 * h)).tanh() / (pi * (w + 2.0 * s) / (4.0 * h)).tanh();
        let q = kk(k3) / (kk(k1) + kk(k3));
        let e = 1.0 + q * (er - 1.0);
        (60.0 * pi / e.sqrt() / (kk(k1) + kk(k3)), e)
    } else {
        let k2 = (pi * w / (4.0 * h)).sinh() / (pi * (w + 2.0 * s) / (4.0 * h)).sinh();
        let e = 1.0 + (er - 1.0) / 2.0 * kk(k2) / kk(k1);
        (30.0 * pi / e.sqrt() / kk(k1), e)
    };
    Ok(CoplanarWaveguideOut {
        z0: finite("z0", z0)?,
        eeff: finite("eeff", eeff)?,
        delay: delay_ps_mm(eeff),
    })
}

/// Input for [`differential_pair`].
#[derive(Debug, Clone, Copy)]
pub struct DifferentialPair {
    /// Trace width in millimetres, 0.01 to 100.
    pub width: f64,
    /// Edge-to-edge gap in millimetres, 0.01 to 100.
    pub gap: f64,
    /// Dielectric height in millimetres, 0.01 to 10.
    pub height: f64,
    /// Relative permittivity, 1 to 20.
    pub er: f64,
    /// Copper thickness in millimetres, 0 to 0.5; default 0.035.
    pub thickness: f64,
    /// Optional target differential impedance in ohms, 10 to 1000.
    pub target: Option<f64>,
}

impl Default for DifferentialPair {
    fn default() -> Self {
        DifferentialPair {
            width: 0.2,
            gap: 0.2,
            height: 0.2,
            er: 4.2,
            thickness: 0.035,
            target: None,
        }
    }
}

/// Results from [`differential_pair`].
#[derive(Debug, Clone, Copy)]
pub struct DifferentialPairOut {
    /// Differential impedance in ohms.
    pub zdiff: f64,
    /// Single-ended impedance of one trace in ohms.
    pub z0_single: f64,
    /// Width meeting the target with the gap fixed, when asked.
    pub width_for_target: Option<f64>,
    /// Edge-coupling factor 2 (1 - 0.48 exp(-0.96 s / h)).
    pub coupling_factor: f64,
}

/// Differential impedance of two equal coupled surface traces.
///
/// # Rules
/// - Single-ended Hammerstad-Jensen Z0 times the edge-coupling factor
///   2 (1 - 0.48 exp(-0.96 s / h)); the factor is an unverified
///   IPC-2141 attribution, so the result is labelled an estimate.
/// - `target` solves width by bisection with the gap fixed.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for [`microstrip`]'s helper `hj`, `crate::num`, `crate::opt_num`.
/// # Do not
/// - Trust it for s < w; loosely coupled pairs (s >= w) are the model's
///   range.
pub fn differential_pair(a: DifferentialPair) -> Result<DifferentialPairOut, Error> {
    let w = num("width", a.width, 0.01, 100.0)?;
    let s = num("gap", a.gap, 0.01, 100.0)?;
    let h = num("height", a.height, 0.01, 10.0)?;
    let er = num("er", a.er, 1.0, 20.0)?;
    let t = num("thickness", a.thickness, 0.0, 0.5)?;
    let target = opt_num("target", a.target, 10.0, 1000.0)?;
    let factor = 2.0 * (1.0 - 0.48 * (-0.96 * s / h).exp());
    let z0_single = hj(w, h, er, t).0;
    let zdiff = finite("zdiff", z0_single * factor)?;
    let width_for_target = match target {
        Some(goal) => {
            let (mut lo, mut hi) = (0.01f64, 100.0f64);
            for _ in 0..80 {
                let mid = (lo + hi) / 2.0;
                if 2.0 * hj(mid, h, er, t).0 * factor > goal {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            Some((lo + hi) / 2.0)
        }
        None => None,
    };
    Ok(DifferentialPairOut {
        zdiff,
        z0_single,
        width_for_target,
        coupling_factor: factor,
    })
}

/// Input for [`propagation_delay`].
#[derive(Debug, Clone, Copy, Default)]
pub struct PropagationDelay {
    /// Effective permittivity, 1 to 20; from an impedance calculator
    /// or er for a buried trace.
    pub eeff: f64,
    /// Optional trace length in millimetres, 0 to 10000.
    pub length: Option<f64>,
    /// Optional skew budget in picoseconds, 0 to 1e6.
    pub skew_budget: Option<f64>,
}

/// Delays and length budget from [`propagation_delay`].
#[derive(Debug, Clone, Copy)]
pub struct PropagationDelayOut {
    /// Delay in picoseconds per millimetre.
    pub delay: f64,
    /// Delay in picoseconds per inch.
    pub delay_inch: f64,
    /// Delay over the given length, when a length was given.
    pub total_delay: Option<f64>,
    /// Length a skew budget allows, when a budget was given.
    pub allowed_mismatch: Option<f64>,
}

/// Signal speed from effective permittivity, and a skew budget in mm.
///
/// # Rules
/// - tpd = sqrt(eeff) / C0, exact for the given eeff; inch values use
///   the 25.4 mm definition.
/// - `length` and `skew_budget` are independent extras; give neither.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (C0), `delay_ps_mm`, `crate::opt_num`.
/// # Do not
/// - Reuse a stripline eeff for a microstrip of the same stackup;
///   take eeff from the matching impedance calculator.
pub fn propagation_delay(a: PropagationDelay) -> Result<PropagationDelayOut, Error> {
    let e = num("eeff", a.eeff, 1.0, 20.0)?;
    let len = opt_num("length", a.length, 0.0, 10_000.0)?;
    let skew = opt_num("skew_budget", a.skew_budget, 0.0, 1.0e6)?;
    let tpd = delay_ps_mm(e);
    Ok(PropagationDelayOut {
        delay: tpd,
        delay_inch: tpd * 25.4,
        total_delay: len.map(|l| finite("total_delay", tpd * l)).transpose()?,
        allowed_mismatch: skew
            .map(|s| finite("allowed_mismatch", s / tpd))
            .transpose()?,
    })
}

/// Input for [`crosstalk`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Crosstalk {
    /// Edge-to-edge gap in millimetres, 0.01 to 100.
    pub gap: f64,
    /// Dielectric height in millimetres, 0.01 to 10.
    pub height: f64,
    /// Trace width in millimetres, 0.01 to 100, for the 3W gap.
    pub width: f64,
}

/// Coupling estimates from [`crosstalk`].
#[derive(Debug, Clone, Copy)]
pub struct CrosstalkOut {
    /// Fraction of the aggressor swing coupled into the victim.
    pub coupling: f64,
    /// Same coupling in dB.
    pub coupling_db: f64,
    /// 3W rule gap in millimetres (edge to edge, centre spacing 3 w).
    pub three_w_gap: f64,
    /// Gap for -20 dB, 3 h, in millimetres.
    pub spacing_for_minus_20db: f64,
}

/// Order-of-magnitude near-end crosstalk for parallel surface traces.
///
/// # Rules
/// - coupling = 1 / (1 + (s/h)^2); dB = 20 log10 of that; the 3W gap
///   is 2 w edge to edge; -20 dB needs s = 3 h.
/// - Rule of thumb, real values land within a factor of 2.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num`, `crate::finite`.
/// # Do not
/// - Apply to stripline (couples less) or to long coupled lengths
///   comparable with the rise time.
pub fn crosstalk(a: Crosstalk) -> Result<CrosstalkOut, Error> {
    let s = num("gap", a.gap, 0.01, 100.0)?;
    let h = num("height", a.height, 0.01, 10.0)?;
    let w = num("width", a.width, 0.01, 100.0)?;
    let coupling = 1.0 / (1.0 + (s / h).powi(2));
    Ok(CrosstalkOut {
        coupling,
        coupling_db: finite("coupling_db", 20.0 * coupling.log10())?,
        three_w_gap: 2.0 * w,
        spacing_for_minus_20db: 3.0 * h,
    })
}
