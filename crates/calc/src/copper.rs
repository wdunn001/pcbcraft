use crate::consts::{K_CU, MIL_MM, OZ_UM, RHO_CU};
use crate::error::{check_range, CalcError};
use crate::Accuracy;
use std::f64::consts::PI;

const IPC2152_1OZ: &[(f64, f64)] = &[
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
];

fn ipc2152_width(current_a: f64) -> f64 {
    if current_a <= IPC2152_1OZ[0].0 {
        return IPC2152_1OZ[0].1 * (current_a / IPC2152_1OZ[0].0);
    }
    for window in IPC2152_1OZ.windows(2) {
        let (i0, w0) = window[0];
        let (i1, w1) = window[1];
        if current_a >= i0 && current_a <= i1 {
            let t = (current_a - i0) / (i1 - i0);
            return w0 + t * (w1 - w0);
        }
    }
    let (i, w) = IPC2152_1OZ[IPC2152_1OZ.len() - 1];
    w * (current_a / i)
}

fn ipc2221_width_mm(current_a: f64, dt_c: f64, external: bool) -> f64 {
    let k = if external { 0.048 } else { 0.024 };
    let area_mil2 = (current_a / (k * dt_c.powf(0.44))).powf(1.0 / 0.725);
    let area_mm2 = area_mil2 * MIL_MM * MIL_MM;
    let thickness_mm = OZ_UM * 1e-3;
    area_mm2 / thickness_mm
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceWidthStandard {
    Ipc2152Table,
    Ipc2221Estimate,
}

impl TraceWidthStandard {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ipc2152Table => "ipc2152_table",
            Self::Ipc2221Estimate => "ipc2221_estimate",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceWidthOut {
    pub width_mm: f64,
    pub standard: TraceWidthStandard,
    pub accuracy: Accuracy,
}

pub fn trace_width(current: f64, delta_t: f64, external: f64) -> Result<TraceWidthOut, CalcError> {
    check_range("current", current, 0.1, 20.0)?;
    check_range("delta_t", delta_t, 1.0, 100.0)?;
    check_range("external", external, 0.0, 1.0)?;
    let is_external = external >= 0.5;
    let (width_mm, standard) = if is_external && current <= 2.0 {
        (ipc2152_width(current), TraceWidthStandard::Ipc2152Table)
    } else {
        (
            ipc2221_width_mm(current, delta_t, is_external),
            TraceWidthStandard::Ipc2221Estimate,
        )
    };
    Ok(TraceWidthOut {
        width_mm,
        standard,
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViaOut {
    pub resistance_mohm: f64,
    pub accuracy: Accuracy,
}

pub fn via_current_resistance(
    diameter: f64,
    length: f64,
    current: f64,
) -> Result<ViaOut, CalcError> {
    check_range("diameter", diameter, 0.1, 2.0)?;
    check_range("length", length, 0.1, 10.0)?;
    check_range("current", current, 0.0, 20.0)?;
    let _ = current;
    let area = PI * (0.5 * diameter * 1e-3).powi(2);
    let r = RHO_CU * (length * 1e-3) / area;
    Ok(ViaOut {
        resistance_mohm: r * 1000.0,
        accuracy: Accuracy::Model,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceDropOut {
    pub resistance_mohm: f64,
    pub voltage_drop_mv: f64,
    pub accuracy: Accuracy,
}

pub fn trace_resistance_drop(
    length: f64,
    width: f64,
    thickness: f64,
    current: f64,
) -> Result<TraceDropOut, CalcError> {
    check_range("length", length, 0.1, 10_000.0)?;
    check_range("width", width, 0.05, 50.0)?;
    check_range("thickness", thickness, 0.01, 0.2)?;
    check_range("current", current, 0.0, 50.0)?;
    let area = width * 1e-3 * thickness * 1e-3;
    let r = RHO_CU * (length * 1e-3) / area;
    Ok(TraceDropOut {
        resistance_mohm: r * 1000.0,
        voltage_drop_mv: r * current * 1000.0,
        accuracy: Accuracy::Model,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireGaugeOut {
    pub diameter_mm: f64,
    pub accuracy: Accuracy,
}

pub fn wire_gauge(awg: f64) -> Result<WireGaugeOut, CalcError> {
    check_range("awg", awg, 10.0, 24.0)?;
    let table: &[(i32, f64)] = &[
        (10, 2.588),
        (12, 2.053),
        (14, 1.628),
        (16, 1.291),
        (18, 1.024),
        (20, 0.812),
        (22, 0.644),
        (24, 0.511),
    ];
    let awg_i = awg.round() as i32;
    for (g, d) in table {
        if *g == awg_i {
            return Ok(WireGaugeOut {
                diameter_mm: *d,
                accuracy: Accuracy::Exact,
            });
        }
    }
    Err(CalcError::Domain {
        message: "AWG must be an integer between 10 and 24",
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalViaOut {
    pub count: f64,
    pub accuracy: Accuracy,
}

pub fn thermal_via_count(
    power: f64,
    delta_t: f64,
    via_diameter: f64,
) -> Result<ThermalViaOut, CalcError> {
    check_range("power", power, 0.1, 100.0)?;
    check_range("delta_t", delta_t, 1.0, 80.0)?;
    check_range("via_diameter", via_diameter, 0.2, 0.8)?;
    let radius = 0.5 * via_diameter * 1e-3;
    let conductance = K_CU * PI * radius * radius / 1.6e-3;
    let count = (power / (conductance * delta_t)).ceil();
    Ok(ThermalViaOut {
        count,
        accuracy: Accuracy::Estimate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_width_reports_ipc_source() -> Result<(), CalcError> {
        let out = trace_width(1.0, 10.0, 1.0)?;
        assert_eq!(out.standard, TraceWidthStandard::Ipc2152Table);
        Ok(())
    }

    #[test]
    fn trace_width_rejects_out_of_range_current() {
        assert!(trace_width(0.05, 10.0, 1.0).is_err());
    }
}
