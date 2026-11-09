//! Impedance and signal-integrity calculators: microstrip,
//! embedded_microstrip, stripline, coplanar_waveguide, differential_pair,
//! propagation_delay, crosstalk.
//!
//! The microstrip family is one Hammerstad-Jensen formula, continuous in
//! w / h, replacing the old two-branch estimate that jumped about 12 ohm
//! at w = h (see research-283-length-impedance.md). Models are labelled
//! exact, model or estimate in the metadata; the UI must show that label.

use crate::consts::{C0, OZ_UM, RHO_CU, ZF0};
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

/// Input for [`rlgc`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Rlgc {
    /// Trace width in millimetres, 0.01 to 100.
    pub width: f64,
    /// Dielectric height in millimetres, 0.01 to 10.
    pub height: f64,
    /// Relative permittivity, 1 to 20.
    pub er: f64,
    /// Dissipation factor (loss tangent), 0 to 0.5.
    pub df: f64,
    /// Copper weight in oz per square foot, 0.25 to 6.
    pub copper_weight: f64,
    /// Signal frequency in hertz, 1 to 1e12.
    pub frequency: f64,
}

/// Per-unit-length parameters and loss from [`rlgc`].
#[derive(Debug, Clone, Copy)]
pub struct RlgcOut {
    /// Resistance per metre, ohms per metre.
    pub r_per_m: f64,
    /// Inductance per metre, henries per metre.
    pub l_per_m: f64,
    /// Conductance per metre, siemens per metre.
    pub g_per_m: f64,
    /// Capacitance per metre, farads per metre.
    pub c_per_m: f64,
    /// Characteristic impedance in ohms.
    pub z0: f64,
    /// Attenuation in decibels per metre.
    pub loss_db_per_m: f64,
    /// Attenuation in decibels per inch.
    pub loss_db_per_inch: f64,
}

/// Per-unit-length RLGC parameters and signal loss at a frequency.
///
/// # Rules
/// - Inputs are millimetres, ounces of copper per square foot and hertz. Width,
///   height and copper thickness become metres before any arithmetic, and the
///   rest is SI, so no length in millimetres meets a length in metres.
/// - Z0 and eeff come from the Hammerstad-Jensen microstrip model with its
///   thickness correction (`hj`). C = sqrt(eeff)/(C0*Z0), L = Z0*sqrt(eeff)/C0
///   and G = 2*pi*f*C*df.
/// - R is the DC resistance rho/(w*t) while the skin depth
///   delta = sqrt(rho/(pi*f*mu0)) is at least half the copper thickness, and
///   rho/(w*t) * t/(2*delta) above that (current on both faces of the strip).
///   The two branches meet at t = 2*delta, so R never jumps.
/// - Loss alpha = R/(2*Z0) + G*Z0/2 in nepers per metre; dB per metre is
///   8.686 times that and dB per inch is 0.0254 times dB per metre.
/// - Dielectric loss uses the full quasi-TEM capacitance, as if the dielectric
///   filled the whole field. For FR4 microstrip that is about 15 to 20 percent
///   above the filling-factor form.
/// - Surface roughness, dispersion and radiation are not modelled, so loss is
///   optimistic for rough copper and approximate above a few GHz.
///
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::num` and `crate::finite`.
/// - `crates/calc/src/consts.rs` for `C0`, `RHO_CU` and `OZ_UM`.
///
/// # Do not
/// - Mix millimetres with metres inside this function.
/// - Trust the result above about 10 GHz; the model is quasi-TEM.
pub fn rlgc(a: Rlgc) -> Result<RlgcOut, Error> {
    let w_mm = num("width", a.width, 0.01, 100.0)?;
    let h_mm = num("height", a.height, 0.01, 10.0)?;
    let er = num("er", a.er, 1.0, 20.0)?;
    let df = num("df", a.df, 0.0, 0.5)?;
    let oz = num("copper_weight", a.copper_weight, 0.25, 6.0)?;
    let f = num("frequency", a.frequency, 1.0, 1.0e12)?;
    let pi = std::f64::consts::PI;
    let mu0 = 4.0e-7 * pi;
    // Everything below is SI: metres, ohms, hertz.
    let w = w_mm * 1.0e-3;
    let h = h_mm * 1.0e-3;
    let t = oz * OZ_UM * 1.0e-6;
    let (z0, eeff) = hj(w, h, er, t);
    let c_per_m = finite("c_per_m", eeff.sqrt() / (C0 * z0))?;
    let l_per_m = finite("l_per_m", z0 * eeff.sqrt() / C0)?;
    let g_per_m = 2.0 * pi * f * c_per_m * df;
    let r_dc = RHO_CU / (w * t);
    let delta = finite("delta", (RHO_CU / (pi * f * mu0)).sqrt())?;
    let r_per_m = if t > 2.0 * delta {
        finite("r_per_m", r_dc * t / (2.0 * delta))?
    } else {
        r_dc
    };
    let alpha = r_per_m / (2.0 * z0) + g_per_m * z0 / 2.0;
    let loss_db_per_m = finite("loss_db_per_m", 8.686 * alpha)?;
    Ok(RlgcOut {
        r_per_m,
        l_per_m,
        g_per_m,
        c_per_m,
        z0,
        loss_db_per_m,
        loss_db_per_inch: loss_db_per_m * 0.0254,
    })
}

#[cfg(test)]
mod rlgc_tests {
    use super::*;

    fn trace(frequency: f64) -> Rlgc {
        Rlgc {
            width: 1.0,
            height: 1.6,
            er: 4.5,
            df: 0.02,
            copper_weight: 1.0,
            frequency,
        }
    }

    #[test]
    fn low_frequency_resistance_is_the_dc_value() -> Result<(), Error> {
        let out = rlgc(trace(1.0e3))?;
        let dc = RHO_CU / (1.0e-3 * OZ_UM * 1.0e-6);
        assert!((out.r_per_m - dc).abs() <= 1.0e-12 * dc);
        Ok(())
    }

    #[test]
    fn resistance_grows_with_the_square_root_of_frequency_in_the_skin_regime() -> Result<(), Error>
    {
        let lo = rlgc(trace(1.0e9))?;
        let hi = rlgc(trace(4.0e9))?;
        let ratio = hi.r_per_m / lo.r_per_m;
        assert!((ratio - 2.0).abs() < 1.0e-9, "ratio {ratio}");
        Ok(())
    }

    #[test]
    fn resistance_is_continuous_where_the_skin_depth_reaches_half_the_copper() -> Result<(), Error>
    {
        let mu0 = 4.0e-7 * std::f64::consts::PI;
        let t = OZ_UM * 1.0e-6;
        let delta = t / 2.0;
        let f = RHO_CU / (std::f64::consts::PI * mu0 * delta * delta);
        let below = rlgc(trace(f * 0.999_999))?;
        let above = rlgc(trace(f * 1.000_001))?;
        let rel = (above.r_per_m - below.r_per_m).abs() / below.r_per_m;
        assert!(rel < 1.0e-5, "jump {rel}");
        Ok(())
    }

    #[test]
    fn one_ounce_trace_at_one_gigahertz_has_about_four_ohms_per_metre() -> Result<(), Error> {
        let out = rlgc(trace(1.0e9))?;
        assert!((out.r_per_m - 4.120_160).abs() < 1.0e-3, "{}", out.r_per_m);
        Ok(())
    }

    #[test]
    fn loss_per_inch_is_the_per_metre_loss_times_the_inch_in_metres() -> Result<(), Error> {
        let out = rlgc(trace(2.0e9))?;
        assert!((out.loss_db_per_inch - out.loss_db_per_m * 0.0254).abs() < 1.0e-12);
        Ok(())
    }

    #[test]
    fn loss_and_reactive_parts_follow_the_stated_relations() -> Result<(), Error> {
        let out = rlgc(trace(1.0e9))?;
        let alpha = out.r_per_m / (2.0 * out.z0) + out.g_per_m * out.z0 / 2.0;
        assert!((out.loss_db_per_m - 8.686 * alpha).abs() < 1.0e-9);
        // Z0 = sqrt(L/C) for a lossless line.
        let z = (out.l_per_m / out.c_per_m).sqrt();
        assert!((z - out.z0).abs() < 1.0e-6 * out.z0);
        Ok(())
    }

    #[test]
    fn out_of_range_inputs_are_refused_with_the_interval() {
        let mut bad = trace(1.0e9);
        bad.width = 0.0;
        let msg = rlgc(bad).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("width") && msg.contains("0.01"), "{msg}");
        let mut bad = trace(1.0e9);
        bad.frequency = 0.0;
        let msg = rlgc(bad).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("frequency"), "{msg}");
        let mut bad = trace(1.0e9);
        bad.df = f64::NAN;
        let msg = rlgc(bad).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("df"), "{msg}");
    }
}
