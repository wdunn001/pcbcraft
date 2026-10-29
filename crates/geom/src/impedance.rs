//! Transmission-line impedance models for printed circuit traces.
//! Lengths are millimetres, impedances ohms, delays picoseconds.
//!
//! Microstrip uses the Hammerstad and Jensen (1980) closed form with the
//! strip thickness correction. It is one expression for every width, so the
//! curve has no step and impedance falls steadily as the trace widens.
//! Coupled microstrip uses the same authors' even and odd mode model (zero
//! thickness). Stripline uses the exact zero thickness solution for a strip
//! centred between two planes, widened by the isolated strip thickness
//! allowance.
//!
//! Golden values in the tests were produced by the scripts in
//! `crates/geom/oracle`: single microstrip from scikit-rf `MLine`, coupled
//! and stripline cross-checked against finite-difference field solves.
use std::f64::consts::{E, PI};

/// Impedance of free space in ohms.
const ZF0: f64 = 376.730_313_668_57;
/// Speed of light in millimetres per picosecond.
pub const LIGHT_MM_PER_PS: f64 = 0.299_792_458;

/// A single-ended microstrip estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Microstrip {
    /// Characteristic impedance in ohms.
    pub ohms: f64,
    /// Effective relative permittivity.
    pub eeff: f64,
}

/// Even and odd mode results for an edge-coupled pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coupled {
    pub z_even: f64,
    pub z_odd: f64,
    pub ee_even: f64,
    pub ee_odd: f64,
}

impl Coupled {
    /// Differential impedance of a symmetric pair, twice the odd mode.
    pub fn z_diff(&self) -> f64 {
        2.0 * self.z_odd
    }
    /// Common mode impedance, half the even mode.
    pub fn z_common(&self) -> f64 {
        self.z_even / 2.0
    }
}

fn dims_ok(values: &[f64]) -> bool {
    values.iter().all(|v| v.is_finite() && *v > 0.0)
}

fn zl1(u: f64) -> f64 {
    let f = 6.0 + (2.0 * PI - 6.0) * (-(30.666 / u).powf(0.7528)).exp();
    ZF0 / (2.0 * PI) * (f / u + (1.0 + (2.0 / u).powi(2)).sqrt()).ln()
}

fn b_er(er: f64) -> f64 {
    0.564 * ((er - 0.9) / (er + 3.0)).powf(0.053)
}

fn eeff_u(u: f64, er: f64) -> f64 {
    let a = 1.0
        + ((u.powi(4) + (u / 52.0).powi(2)) / (u.powi(4) + 0.432)).ln() / 49.0
        + (1.0 + (u / 18.1).powi(3)).ln() / 18.7;
    (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 10.0 / u).powf(-a * b_er(er))
}

/// Single microstrip of `width` on a dielectric of `height` with copper
/// `thickness` (zero allowed) and relative permittivity `er`. Returns none
/// for non-finite or non-positive inputs, or an `er` below 1.
pub fn microstrip(width: f64, height: f64, thickness: f64, er: f64) -> Option<Microstrip> {
    if !dims_ok(&[width, height, er]) || er < 1.0 || !thickness.is_finite() || thickness < 0.0 {
        return None;
    }
    let u = width / height;
    let tn = thickness / height;
    let (u1, ur) = if tn > 0.0 {
        let th = (6.517 * u).sqrt().tanh();
        let du1 = tn / PI * (1.0 + 4.0 * E / tn * th * th).ln();
        let dur = du1 * (1.0 + 1.0 / (er - 1.0).sqrt().cosh()) / 2.0;
        (u + du1, u + dur)
    } else {
        (u, u)
    };
    let ee = eeff_u(ur, er);
    let ohms = zl1(ur) / ee.sqrt();
    let ratio = zl1(u1) / zl1(ur);
    let eeff = ee * ratio * ratio;
    (ohms.is_finite() && eeff.is_finite()).then_some(Microstrip { ohms, eeff })
}

/// Edge-coupled microstrip pair, each trace `width` wide, `gap` apart edge to
/// edge, on a dielectric of `height`. Zero thickness; the model is stated for
/// 0.1 <= w/h <= 10 and gap/h >= 0.01.
pub fn coupled_microstrip(width: f64, gap: f64, height: f64, er: f64) -> Option<Coupled> {
    if !dims_ok(&[width, gap, height, er]) || er < 1.0 {
        return None;
    }
    let u = width / height;
    let g = gap / height;
    let ee0 = eeff_u(u, er);
    let z0 = zl1(u) / ee0.sqrt();
    let v = u * (20.0 + g * g) / (10.0 + g * g) + g * (-g).exp();
    let ae = 1.0
        + ((v.powi(4) + (v / 52.0).powi(2)) / (v.powi(4) + 0.432)).ln() / 49.0
        + (1.0 + (v / 18.1).powi(3)).ln() / 18.7;
    let ee_even = (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 10.0 / v).powf(-ae * b_er(er));
    let q1 = 0.8695 * u.powf(0.194);
    let q2 = 1.0 + 0.7519 * g + 0.189 * g.powf(2.31);
    let q3 = 0.1975
        + (16.6 + (8.4 / g).powi(6)).powf(-0.387)
        + (g.powi(10) / (1.0 + (g / 3.4).powi(10))).ln() / 241.0;
    let q4 = 2.0 * q1 / (q2 * ((-g).exp() * u.powf(q3) + (2.0 - (-g).exp()) * u.powf(-q3)));
    let z_even = z0 * (ee0 / ee_even).sqrt() / (1.0 - z0 / ZF0 * ee0.sqrt() * q4);
    let ao = 0.7287 * (ee0 - (er + 1.0) / 2.0) * (1.0 - (-0.179 * u).exp());
    let bo = 0.747 * er / (0.15 + er);
    let co = bo - (bo - 0.207) * (-0.414 * u).exp();
    let dd = 0.593 + 0.694 * (-0.562 * u).exp();
    let ee_odd = ((er + 1.0) / 2.0 + ao - ee0) * (-co * g.powf(dd)).exp() + ee0;
    let q5 = 1.794 + 1.14 * (1.0 + 0.638 / (g + 0.517 * g.powf(2.43))).ln();
    let q6 = 0.2305
        + (g.powi(10) / (1.0 + (g / 5.8).powi(10))).ln() / 281.3
        + (1.0 + 0.598 * g.powf(1.154)).ln() / 5.1;
    let q7 = (10.0 + 190.0 * g * g) / (1.0 + 82.3 * g.powi(3));
    let q8 = (-6.5 - 0.95 * g.ln() - (g / 0.15).powi(5)).exp();
    let q9 = q7.ln() * (q8 + 1.0 / 16.5);
    let q10 = (q2 * q4 - q5 * (u.ln() * q6 * u.powf(-q9)).exp()) / q2;
    let z_odd = z0 * (ee0 / ee_odd).sqrt() / (1.0 - z0 / ZF0 * ee0.sqrt() * q10);
    let out = Coupled {
        z_even,
        z_odd,
        ee_even,
        ee_odd,
    };
    [z_even, z_odd, ee_even, ee_odd]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
        .then_some(out)
}

fn ellipk(m: f64) -> f64 {
    let (mut a, mut b) = (1.0f64, (1.0 - m).max(0.0).sqrt());
    for _ in 0..60 {
        if (a - b).abs() < 1e-15 {
            break;
        }
        let next = (a + b) / 2.0;
        b = (a * b).sqrt();
        a = next;
    }
    PI / (2.0 * a)
}

/// Symmetric stripline: a strip `width` wide and `thickness` thick centred
/// between two planes `spacing` apart (plane to plane) in a dielectric of
/// relative permittivity `er`.
pub fn stripline(width: f64, spacing: f64, thickness: f64, er: f64) -> Option<f64> {
    if !dims_ok(&[width, spacing, er])
        || er < 1.0
        || !thickness.is_finite()
        || thickness < 0.0
        || thickness >= spacing
    {
        return None;
    }
    let dw = if thickness > 0.0 {
        thickness / PI * (1.0 + (4.0 * PI * width / thickness).ln())
    } else {
        0.0
    };
    let k = 1.0 / (PI * (width + dw) / (2.0 * spacing)).cosh();
    let ohms = 30.0 * PI / er.sqrt() * ellipk(k * k) / ellipk(1.0 - k * k);
    ohms.is_finite().then_some(ohms)
}

/// Width whose impedance, from the decreasing function `ohms_at`, comes
/// closest to `target`, found by bisection on 0.01 to 100 mm. Returns none
/// when the target lies outside what that range can reach or the model
/// refuses an input.
pub fn width_for_ohms<F: Fn(f64) -> Option<f64>>(target: f64, ohms_at: F) -> Option<f64> {
    let (mut lo, mut hi) = (0.01f64, 100.0f64);
    if !target.is_finite() || target <= 0.0 {
        return None;
    }
    let (at_lo, at_hi) = (ohms_at(lo)?, ohms_at(hi)?);
    if target > at_lo || target < at_hi {
        return None;
    }
    for _ in 0..80 {
        let mid = (lo + hi) / 2.0;
        if ohms_at(mid)? > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

/// Propagation delay in picoseconds for a copper length in millimetres.
pub fn delay_ps(length_mm: f64, eeff: f64) -> f64 {
    length_mm * eeff.max(1.0).sqrt() / LIGHT_MM_PER_PS
}

/// A plain note when a microstrip width to height ratio lies outside the
/// range the models are stated for, or none when it is inside.
pub fn validity(width: f64, height: f64) -> Option<String> {
    let u = width / height;
    if !u.is_finite() {
        return Some("width or dielectric height is not usable".into());
    }
    if u < 0.1 {
        return Some(format!(
            "width is {u:.2} of the dielectric height; the model is stated for 0.1 to 10, expect a few percent more error"
        ));
    }
    if u > 10.0 {
        return Some(format!(
            "width is {u:.1} times the dielectric height; the model is stated for 0.1 to 10, expect a few percent more error"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    type T = Result<(), String>;
    fn rel(a: f64, b: f64) -> f64 {
        (a / b - 1.0).abs()
    }
    fn some<V>(v: Option<V>) -> Result<V, String> {
        v.ok_or_else(|| "model refused a valid input".to_string())
    }

    // scikit-rf 2.2.0 MLine, hammerstadjensen, no dispersion.
    // (w, h, t, er, ohms, eeff)
    const SINGLE: &[(f64, f64, f64, f64, f64, f64)] = &[
        (0.16000000000000003, 1.6, 0.035, 4.5, 144.5712, 2.83958),
        (0.4, 1.6, 0.035, 4.5, 115.7217, 2.95232),
        (0.8, 1.6, 0.035, 4.5, 92.6194, 3.05513),
        (1.6, 1.6, 0.035, 4.5, 69.3959, 3.19477),
        (1.6096000000000001, 1.6, 0.035, 4.5, 69.1991, 3.19621),
        (3.2, 1.6, 0.035, 4.5, 47.7861, 3.38782),
        (6.4, 1.6, 0.035, 4.5, 30.1137, 3.6204),
        (12.8, 1.6, 0.035, 4.5, 17.564, 3.86472),
        (0.16000000000000003, 1.6, 0.035, 4.4, 145.9117, 2.78764),
        (0.4, 1.6, 0.035, 4.4, 116.8189, 2.89712),
        (0.8, 1.6, 0.035, 4.4, 93.5134, 2.99699),
        (1.6, 1.6, 0.035, 4.4, 70.0805, 3.13265),
        (1.6096000000000001, 1.6, 0.035, 4.4, 69.8819, 3.13405),
        (3.2, 1.6, 0.035, 4.4, 48.2704, 3.32018),
        (6.4, 1.6, 0.035, 4.4, 30.4277, 3.54606),
        (12.8, 1.6, 0.035, 4.4, 17.752, 3.78329),
        (0.08000000000000002, 0.8, 0.018, 3.5, 159.7595, 2.31809),
        (0.2, 0.8, 0.018, 3.5, 128.2555, 2.39867),
        (0.4, 0.8, 0.018, 3.5, 102.8874, 2.47241),
        (0.8, 0.8, 0.018, 3.5, 77.3001, 2.57251),
        (0.8048000000000001, 0.8, 0.018, 3.5, 77.0829, 2.57354),
        (1.6, 0.8, 0.018, 3.5, 53.4089, 2.71049),
        (3.2, 0.8, 0.018, 3.5, 33.7796, 2.87623),
        (6.4, 0.8, 0.018, 3.5, 19.7695, 3.0499),
        (0.05, 0.5, 0.035, 9.8, 97.0617, 5.27096),
        (0.125, 0.5, 0.035, 9.8, 78.8881, 5.61288),
        (0.25, 0.5, 0.035, 9.8, 63.6371, 5.93094),
        (0.5, 0.5, 0.035, 9.8, 47.846, 6.33615),
        (0.503, 0.5, 0.035, 9.8, 47.7109, 6.34019),
        (1.0, 0.5, 0.035, 9.8, 32.9379, 6.86372),
        (2.0, 0.5, 0.035, 9.8, 20.701, 7.48463),
        (4.0, 0.5, 0.035, 9.8, 12.0283, 8.13025),
        (0.025400000000000002, 0.254, 0.018, 2.2, 175.746, 1.60376),
        (0.0635, 0.254, 0.018, 2.2, 145.4449, 1.6482),
        (0.127, 0.254, 0.018, 2.2, 119.1087, 1.69072),
        (0.254, 0.254, 0.018, 2.2, 91.112, 1.74567),
        (0.25552400000000003, 0.254, 0.018, 2.2, 90.8695, 1.74622),
        (0.508, 0.254, 0.018, 2.2, 64.0037, 1.81668),
        (1.016, 0.254, 0.018, 2.2, 41.0871, 1.89924),
        (2.032, 0.254, 0.018, 2.2, 24.3462, 1.98408),
    ];

    const WIDTHS: &[(f64, f64)] = &[
        (30.0, 6.4334),
        (40.0, 4.2447),
        (50.0, 2.9659),
        (60.0, 2.1395),
        (65.0, 1.8304),
        (70.0, 1.5709),
        (75.0, 1.3511),
        (90.0, 0.8647),
        (100.0, 0.6423),
    ];

    const COUPLED: &[(f64, f64, f64, f64, f64, f64)] = &[
        (0.8, 0.8, 1.6, 4.5, 118.6776, 68.2719),
        (1.6, 0.4, 1.6, 4.5, 90.1735, 45.4304),
        (0.64, 0.4, 0.8, 3.5, 107.5152, 63.7029),
        (0.15, 0.15, 0.2, 4.5, 95.0304, 64.2841),
    ];

    const STRIPLINE: &[(f64, f64, f64, f64, f64)] = &[
        (0.14, 1.68, 0.035, 4.5, 87.4585),
        (0.28, 1.68, 0.035, 4.5, 71.6551),
        (0.56, 1.68, 0.035, 4.5, 54.9479),
        (1.12, 1.68, 0.035, 4.5, 38.5189),
        (0.2, 0.5, 0.018, 3.6, 54.8854),
    ];

    #[test]
    fn single_microstrip_matches_scikit_rf() -> T {
        for &(w, h, t, er, z, ee) in SINGLE {
            let m = some(microstrip(w, h, t, er))?;
            assert!(
                rel(m.ohms, z) < 1e-4,
                "w {w} h {h} er {er}: {} vs {z}",
                m.ohms
            );
            assert!(rel(m.eeff, ee) < 1e-4, "w {w}: eeff {} vs {ee}", m.eeff);
        }
        Ok(())
    }

    #[test]
    fn curve_is_continuous_and_falling_through_w_equals_h() -> T {
        let mut previous = f64::INFINITY;
        let mut w = 0.05;
        while w < 20.0 {
            let z = some(microstrip(w, 1.6, 0.035, 4.5))?.ohms;
            assert!(z < previous, "impedance must fall as width grows, at {w}");
            if previous.is_finite() {
                assert!(previous - z < 6.0, "no step near w {w}: {previous} to {z}");
            }
            previous = z;
            w += 0.01;
        }
        let below = some(microstrip(1.599, 1.6, 0.035, 4.5))?.ohms;
        let above = some(microstrip(1.601, 1.6, 0.035, 4.5))?.ohms;
        assert!((below - above).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn wide_microstrip_uses_effective_permittivity() -> T {
        let m = some(microstrip(3.2, 1.6, 0.035, 4.5))?;
        assert!(m.eeff > 3.3 && m.eeff < 3.5);
        assert!((m.ohms - 47.8).abs() < 1.0);
        Ok(())
    }

    #[test]
    fn widths_for_targets_match_the_independent_solve() -> T {
        let mut last = 0.0;
        for &(target, w) in WIDTHS {
            let got = some(width_for_ohms(target, |x| {
                microstrip(x, 1.6, 0.035, 4.5).map(|m| m.ohms)
            }))?;
            assert!((got - w).abs() < 2e-3, "{target} ohm: {got} vs {w}");
            if last > 0.0 {
                assert!(got < last, "higher impedance needs a narrower trace");
            }
            last = got;
        }
        Ok(())
    }

    #[test]
    fn sixty_and_seventy_ohm_targets_give_different_widths() -> T {
        let f = |t| width_for_ohms(t, |x| microstrip(x, 1.6, 0.035, 4.5).map(|m| m.ohms));
        let (a, b, c) = (some(f(60.0))?, some(f(65.0))?, some(f(70.0))?);
        assert!(a > b + 0.2 && b > c + 0.2, "{a} {b} {c}");
        Ok(())
    }

    #[test]
    fn unreachable_target_gives_none() {
        let f = |x: f64| microstrip(x, 1.6, 0.035, 4.5).map(|m| m.ohms);
        assert!(width_for_ohms(2.0, f).is_none());
        assert!(width_for_ohms(900.0, f).is_none());
        assert!(width_for_ohms(f64::NAN, f).is_none());
    }

    #[test]
    fn coupled_microstrip_matches_golden_and_field_solve() -> T {
        for &(w, s, h, er, ze, zo) in COUPLED {
            let c = some(coupled_microstrip(w, s, h, er))?;
            assert!(rel(c.z_even, ze) < 1e-4 && rel(c.z_odd, zo) < 1e-4);
            assert!(c.z_odd < c.z_even && c.ee_odd < c.ee_even);
            assert!((c.z_diff() - 2.0 * zo).abs() < 0.05);
        }
        // Finite-difference field solve, Richardson extrapolated from
        // h/16 and h/32 grids: w/h 1, s/h 0.25, er 4.5, h 1.6 mm.
        let c = some(coupled_microstrip(1.6, 0.4, 1.6, 4.5))?;
        assert!(rel(c.z_even, 89.7) < 0.02, "even {}", c.z_even);
        assert!(rel(c.z_odd, 45.5) < 0.02, "odd {}", c.z_odd);
        Ok(())
    }

    #[test]
    fn widely_spaced_pair_approaches_the_single_line() -> T {
        let single = some(microstrip(1.0, 1.0, 0.0, 4.5))?.ohms;
        let c = some(coupled_microstrip(1.0, 20.0, 1.0, 4.5))?;
        assert!(rel(c.z_even, single) < 0.03 && rel(c.z_odd, single) < 0.03);
        Ok(())
    }

    #[test]
    fn stripline_matches_golden_and_field_solve() -> T {
        for &(w, b, t, er, z) in STRIPLINE {
            let got = some(stripline(w, b, t, er))?;
            assert!(rel(got, z) < 1e-4, "w {w}: {got} vs {z}");
        }
        // Finite-difference field solve, Richardson extrapolated, b 1.68 mm,
        // t 0.035 mm, er 4.5.
        for (w, z) in [(0.14, 87.25), (0.28, 71.60), (0.56, 54.88), (1.12, 38.38)] {
            let got = some(stripline(w, 1.68, 0.035, 4.5))?;
            assert!(rel(got, z) < 0.015, "w {w}: {got} vs {z}");
        }
        Ok(())
    }

    #[test]
    fn bad_inputs_are_refused() {
        assert!(microstrip(0.0, 1.6, 0.035, 4.5).is_none());
        assert!(microstrip(1.0, 1.6, -1.0, 4.5).is_none());
        assert!(microstrip(f64::NAN, 1.6, 0.035, 4.5).is_none());
        assert!(microstrip(1.0, 1.6, 0.035, 0.5).is_none());
        assert!(coupled_microstrip(1.0, 0.0, 1.6, 4.5).is_none());
        assert!(stripline(1.0, 1.0, 1.0, 4.5).is_none());
    }

    #[test]
    fn delay_and_validity() {
        assert!((delay_ps(100.0, 3.0) - 577.7).abs() < 0.5);
        assert!(validity(0.05, 1.6).is_some());
        assert!(validity(1.0, 1.6).is_none());
        assert!(validity(20.0, 1.6).is_some());
    }
}
