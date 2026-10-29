use crate::consts::ZF0;
use crate::error::{check_positive, check_range, CalcError};
use crate::Accuracy;
use std::f64::consts::{E, PI};

const LIGHT_MM_PER_PS: f64 = 0.299_792_458;

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

fn microstrip_core(width: f64, height: f64, thickness: f64, er: f64) -> Option<(f64, f64)> {
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
    (ohms.is_finite() && eeff.is_finite()).then_some((ohms, eeff))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MicrostripOut {
    pub impedance_ohm: f64,
    pub eeff: f64,
    pub accuracy: Accuracy,
}

pub fn microstrip(
    width: f64,
    height: f64,
    thickness: f64,
    er: f64,
) -> Result<MicrostripOut, CalcError> {
    check_range("width", width, 0.01, 100.0)?;
    check_range("height", height, 0.05, 10.0)?;
    check_range("thickness", thickness, 0.0, 0.2)?;
    check_range("er", er, 1.0, 20.0)?;
    let (impedance_ohm, eeff) = microstrip_core(width, height, thickness, er).ok_or(
        CalcError::Domain {
            message: "microstrip inputs refused by the model",
        },
    )?;
    Ok(MicrostripOut {
        impedance_ohm,
        eeff,
        accuracy: Accuracy::Model,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthForOhmsOut {
    pub width_mm: f64,
    pub accuracy: Accuracy,
}

pub fn width_for_target_ohms_microstrip(
    target_ohm: f64,
    height: f64,
    thickness: f64,
    er: f64,
) -> Result<WidthForOhmsOut, CalcError> {
    check_positive("target_ohm", target_ohm)?;
    check_range("height", height, 0.05, 10.0)?;
    check_range("thickness", thickness, 0.0, 0.2)?;
    check_range("er", er, 1.0, 20.0)?;
    let width = width_for_ohms(target_ohm, |w| microstrip_core(w, height, thickness, er).map(|(z, _)| z))
        .ok_or(CalcError::Domain {
            message: "target impedance is outside the searchable width range",
        })?;
    Ok(WidthForOhmsOut {
        width_mm: width,
        accuracy: Accuracy::Model,
    })
}

fn width_for_ohms<F: Fn(f64) -> Option<f64>>(target: f64, ohms_at: F) -> Option<f64> {
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

pub fn embedded_microstrip(
    width: f64,
    height: f64,
    thickness: f64,
    er: f64,
) -> Result<MicrostripOut, CalcError> {
    check_range("width", width, 0.01, 100.0)?;
    check_range("height", height, 0.05, 10.0)?;
    check_range("thickness", thickness, 0.0, 0.2)?;
    check_range("er", er, 1.0, 20.0)?;
    let eff_h = height * 0.6;
    let eff_er = er * 1.05;
    microstrip(width, eff_h, thickness, eff_er)
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StriplineOut {
    pub impedance_ohm: f64,
    pub accuracy: Accuracy,
}

pub fn stripline(
    width: f64,
    plane_spacing: f64,
    thickness: f64,
    er: f64,
) -> Result<StriplineOut, CalcError> {
    check_range("width", width, 0.01, 50.0)?;
    check_range("plane_spacing", plane_spacing, 0.1, 10.0)?;
    check_range("thickness", thickness, 0.0, 0.2)?;
    check_range("er", er, 1.0, 20.0)?;
    if thickness >= plane_spacing {
        return Err(CalcError::Domain {
            message: "strip thickness must be less than plane spacing",
        });
    }
    let dw = if thickness > 0.0 {
        thickness / PI * (1.0 + (4.0 * PI * width / thickness).ln())
    } else {
        0.0
    };
    let k = 1.0 / (PI * (width + dw) / (2.0 * plane_spacing));
    let k = k.cosh();
    let ohms = 30.0 * PI / er.sqrt() * ellipk(k * k) / ellipk(1.0 - k * k);
    if !ohms.is_finite() {
        return Err(CalcError::Domain {
            message: "stripline model refused these inputs",
        });
    }
    Ok(StriplineOut {
        impedance_ohm: ohms,
        accuracy: Accuracy::Model,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CpwOut {
    pub impedance_ohm: f64,
    pub accuracy: Accuracy,
}

pub fn coplanar_waveguide(
    width: f64,
    gap: f64,
    height: f64,
    er: f64,
) -> Result<CpwOut, CalcError> {
    check_range("width", width, 0.05, 50.0)?;
    check_range("gap", gap, 0.05, 5.0)?;
    check_range("height", height, 0.05, 10.0)?;
    check_range("er", er, 1.0, 20.0)?;
    let base = microstrip(width, height, 0.035, er)?;
    Ok(CpwOut {
        impedance_ohm: base.impedance_ohm * (1.0 + 0.3 * gap / width),
        accuracy: Accuracy::Model,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffPairOut {
    pub z_diff_ohm: f64,
    pub accuracy: Accuracy,
}

pub fn differential_pair(
    width: f64,
    gap: f64,
    height: f64,
    er: f64,
) -> Result<DiffPairOut, CalcError> {
    check_range("width", width, 0.05, 10.0)?;
    check_range("gap", gap, 0.05, 2.0)?;
    check_range("height", height, 0.05, 10.0)?;
    check_range("er", er, 1.0, 20.0)?;
    let z0 = microstrip(width, height, 0.0, er)?.impedance_ohm;
    let factor = 0.58 - 0.03 * (gap / height).min(3.0);
    Ok(DiffPairOut {
        z_diff_ohm: 2.0 * z0 * factor,
        accuracy: Accuracy::Model,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelayOut {
    pub delay_ps: f64,
    pub accuracy: Accuracy,
}

pub fn propagation_delay(length: f64, eeff: f64) -> Result<DelayOut, CalcError> {
    check_range("length", length, 0.01, 10_000.0)?;
    check_range("eeff", eeff, 1.0, 20.0)?;
    Ok(DelayOut {
        delay_ps: length * eeff.max(1.0).sqrt() / LIGHT_MM_PER_PS,
        accuracy: Accuracy::Model,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrosstalkOut {
    pub next_db: f64,
    pub accuracy: Accuracy,
}

pub fn crosstalk(spacing: f64, height: f64, length: f64) -> Result<CrosstalkOut, CalcError> {
    check_range("spacing", spacing, 0.05, 10.0)?;
    check_range("height", height, 0.05, 10.0)?;
    check_range("length", length, 1.0, 1000.0)?;
    let ratio = spacing / height;
    check_positive("spacing/height", ratio)?;
    let next_db =
        -20.0 * ratio.log10() - 10.0 * (length / 25.4).log10();
    Ok(CrosstalkOut {
        next_db,
        accuracy: Accuracy::Estimate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(a: f64, b: f64) -> f64 {
        (a / b - 1.0).abs()
    }

    #[test]
    fn microstrip_is_continuous_at_w_over_h_one() -> Result<(), CalcError> {
        let below = microstrip(1.599, 1.6, 0.035, 4.5)?.impedance_ohm;
        let above = microstrip(1.601, 1.6, 0.035, 4.5)?.impedance_ohm;
        assert!((below - above).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn microstrip_reproduces_hammerstad_reference_triplet() -> Result<(), CalcError> {
        let cases = [(1.0, 85.10), (2.0, 62.15), (0.5, 108.36)];
        for (w, target) in cases {
            let z = microstrip(w, 1.6, 0.035, 4.5)?.impedance_ohm;
            assert!(rel(z, target) < 0.01, "w {w}: got {z} want {target}");
        }
        Ok(())
    }

    #[test]
    fn microstrip_fifty_ohm_width_matches_reference() -> Result<(), CalcError> {
        let w = width_for_target_ohms_microstrip(50.0, 1.6, 0.035, 4.5)?.width_mm;
        assert!(rel(w, 2.966) < 0.01);
        Ok(())
    }
}
