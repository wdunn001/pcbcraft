use crate::error::{check_range, CalcError};
use crate::Accuracy;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConductorSpacingOut {
    pub spacing_mm: f64,
    pub accuracy: Accuracy,
}

pub fn conductor_spacing_ipc2221(voltage: f64, coated: f64) -> Result<ConductorSpacingOut, CalcError> {
    check_range("voltage", voltage, 0.0, 1000.0)?;
    check_range("coated", coated, 0.0, 1.0)?;
    let spacing_mm = if coated >= 0.5 {
        if voltage <= 15.0 {
            0.1
        } else if voltage <= 30.0 {
            0.1
        } else if voltage <= 50.0 {
            0.2
        } else if voltage <= 100.0 {
            0.3
        } else if voltage <= 150.0 {
            0.4
        } else {
            0.1 + voltage / 300.0
        }
    } else if voltage <= 150.0 {
        0.8
    } else if voltage <= 300.0 {
        1.5
    } else {
        2.0 + voltage / 500.0
    };
    Ok(ConductorSpacingOut {
        spacing_mm,
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreepageOut {
    pub clearance_mm: f64,
    pub creepage_mm: f64,
    pub accuracy: Accuracy,
}

pub fn creepage_clearance_iec60664(voltage: f64, pollution: f64) -> Result<CreepageOut, CalcError> {
    check_range("voltage", voltage, 0.0, 1000.0)?;
    check_range("pollution", pollution, 1.0, 3.0)?;
    let pol = pollution.round() as i32;
    let (clearance_mm, creepage_mm) = match (voltage as i32, pol) {
        (v, 2) if v <= 50 => (0.5, 0.8),
        (v, 2) if v <= 250 => (1.5, 2.5),
        (v, 3) if v <= 400 => (3.0, 4.0),
        _ => (voltage / 200.0 + 0.5, voltage / 150.0 + 1.0),
    };
    Ok(CreepageOut {
        clearance_mm,
        creepage_mm,
        accuracy: Accuracy::Estimate,
    })
}
