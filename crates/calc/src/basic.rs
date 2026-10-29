use crate::error::{check_finite, check_positive, check_range, CalcError};
use crate::Accuracy;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OhmsLawOut {
    pub voltage: f64,
    pub current: f64,
    pub resistance: f64,
    pub accuracy: Accuracy,
}

/// Solve Ohm's law for the missing quantity (pass zero for the unknown).
pub fn ohms_law(voltage: f64, current: f64, resistance: f64) -> Result<OhmsLawOut, CalcError> {
    check_range("voltage", voltage, 0.0, 1e6)?;
    check_range("current", current, 0.0, 1e4)?;
    check_range("resistance", resistance, 0.0, 1e9)?;
    let unknowns = [voltage == 0.0, current == 0.0, resistance == 0.0]
        .into_iter()
        .filter(|b| *b)
        .count();
    if unknowns != 1 {
        return Err(CalcError::Domain {
            message: "provide exactly one zero unknown among voltage, current, resistance",
        });
    }
    let (v, i, r) = if voltage == 0.0 {
        check_positive("current", current)?;
        check_positive("resistance", resistance)?;
        (current * resistance, current, resistance)
    } else if current == 0.0 {
        check_positive("voltage", voltage)?;
        check_positive("resistance", resistance)?;
        (voltage, voltage / resistance, resistance)
    } else {
        check_positive("voltage", voltage)?;
        check_positive("current", current)?;
        (voltage, current, voltage / current)
    };
    Ok(OhmsLawOut {
        voltage: v,
        current: i,
        resistance: r,
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeriesParallelOut {
    pub equivalent: f64,
    pub accuracy: Accuracy,
}

pub fn series_parallel(r1: f64, r2: f64, mode: f64) -> Result<SeriesParallelOut, CalcError> {
    check_range("r1", r1, 0.001, 1e9)?;
    check_range("r2", r2, 0.001, 1e9)?;
    check_range("mode", mode, 1.0, 2.0)?;
    let equivalent = if mode >= 1.5 {
        1.0 / (1.0 / r1 + 1.0 / r2)
    } else {
        r1 + r2
    };
    Ok(SeriesParallelOut {
        equivalent,
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoltageDividerOut {
    pub vout: f64,
    pub accuracy: Accuracy,
}

pub fn voltage_divider(vin: f64, r1: f64, r2: f64) -> Result<VoltageDividerOut, CalcError> {
    check_range("vin", vin, 0.0, 1e6)?;
    check_range("r1", r1, 0.001, 1e9)?;
    check_range("r2", r2, 0.001, 1e9)?;
    Ok(VoltageDividerOut {
        vout: vin * r2 / (r1 + r2),
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LedResistorOut {
    pub resistor: f64,
    pub accuracy: Accuracy,
}

pub fn led_resistor(v_supply: f64, v_led: f64, i_led: f64) -> Result<LedResistorOut, CalcError> {
    check_range("v_supply", v_supply, 0.5, 48.0)?;
    check_range("v_led", v_led, 0.5, 5.0)?;
    check_range("i_led", i_led, 0.001, 0.1)?;
    if v_supply <= v_led {
        return Err(CalcError::Domain {
            message: "supply must exceed LED forward voltage",
        });
    }
    Ok(LedResistorOut {
        resistor: (v_supply - v_led) / i_led,
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeConstantOut {
    pub tau_seconds: f64,
    pub accuracy: Accuracy,
}

pub fn rc_rl_time_constant(
    resistance: f64,
    capacitance: f64,
    inductance: f64,
) -> Result<TimeConstantOut, CalcError> {
    check_range("resistance", resistance, 0.001, 1e9)?;
    check_range("capacitance", capacitance, 0.0, 1.0)?;
    check_range("inductance", inductance, 0.0, 10.0)?;
    let tau = if capacitance > 0.0 && inductance == 0.0 {
        resistance * capacitance
    } else if inductance > 0.0 && capacitance == 0.0 {
        inductance / resistance
    } else {
        return Err(CalcError::Domain {
            message: "provide either capacitance or inductance, not both",
        });
    };
    Ok(TimeConstantOut {
        tau_seconds: tau,
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LcResonanceOut {
    pub frequency_hz: f64,
    pub accuracy: Accuracy,
}

pub fn lc_resonance(inductance: f64, capacitance: f64) -> Result<LcResonanceOut, CalcError> {
    check_range("inductance", inductance, 1e-12, 10.0)?;
    check_range("capacitance", capacitance, 1e-15, 1.0)?;
    Ok(LcResonanceOut {
        frequency_hz: 1.0 / (2.0 * PI * (inductance * capacitance).sqrt()),
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReactanceOut {
    pub reactance_ohm: f64,
    pub accuracy: Accuracy,
}

pub fn reactance(frequency: f64, capacitance: f64, inductance: f64) -> Result<ReactanceOut, CalcError> {
    check_range("frequency", frequency, 0.1, 1e12)?;
    check_range("capacitance", capacitance, 0.0, 1.0)?;
    check_range("inductance", inductance, 0.0, 10.0)?;
    let x = if capacitance > 0.0 && inductance == 0.0 {
        -1.0 / (2.0 * PI * frequency * capacitance)
    } else if inductance > 0.0 && capacitance == 0.0 {
        2.0 * PI * frequency * inductance
    } else {
        return Err(CalcError::Domain {
            message: "provide either capacitance or inductance, not both",
        });
    };
    Ok(ReactanceOut {
        reactance_ohm: x,
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecibelOut {
    pub decibels: f64,
    pub accuracy: Accuracy,
}

pub fn decibel(ratio: f64, mode: f64) -> Result<DecibelOut, CalcError> {
    check_range("ratio", ratio, 1e-12, 1e12)?;
    check_range("mode", mode, 1.0, 2.0)?;
    check_positive("ratio", ratio)?;
    let db = if mode >= 1.5 {
        20.0 * ratio.log10()
    } else {
        10.0 * ratio.log10()
    };
    Ok(DecibelOut {
        decibels: db,
        accuracy: Accuracy::Exact,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohms_law_rejects_nonpositive_resistance() -> Result<(), CalcError> {
        let err = ohms_law(5.0, 1.0, 0.0).expect_err("need one unknown");
        let _ = err;
        let err = ohms_law(0.0, 1.0, -1.0);
        assert!(err.is_err());
        Ok(())
    }
}
