use crate::error::{check_positive, check_range, CalcError};
use crate::Accuracy;
const E12: [f64; 12] = [10.0, 12.0, 15.0, 18.0, 22.0, 27.0, 33.0, 39.0, 47.0, 56.0, 68.0, 82.0];

fn nearest_e12(value: f64) -> f64 {
    let exp = value.log10().floor();
    let mant = value / 10f64.powf(exp);
    let mut best = E12[0];
    for e in E12 {
        if (e - mant).abs() < (best - mant).abs() {
            best = e;
        }
    }
    best * 10f64.powf(exp)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecouplingOut {
    pub capacitance_f: f64,
    pub accuracy: Accuracy,
}

pub fn decoupling_capacitor(
    delta_i: f64,
    delta_v: f64,
    delta_t: f64,
) -> Result<DecouplingOut, CalcError> {
    check_range("delta_i", delta_i, 0.01, 10.0)?;
    check_range("delta_v", delta_v, 0.001, 1.0)?;
    check_range("delta_t", delta_t, 1e-9, 1e-3)?;
    Ok(DecouplingOut {
        capacitance_f: delta_i * delta_t / delta_v,
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct I2cPullupOut {
    pub resistor_ohm: f64,
    pub accuracy: Accuracy,
}

pub fn i2c_pullup(voltage: f64, capacitance: f64, rise_ns: f64) -> Result<I2cPullupOut, CalcError> {
    check_range("voltage", voltage, 1.8, 5.5)?;
    check_range("capacitance", capacitance, 10e-12, 500e-12)?;
    check_range("rise_ns", rise_ns, 20.0, 1000.0)?;
    let tr = rise_ns * 1e-9;
    let delta = voltage - 0.9 * voltage;
    let denom = 0.8473 * capacitance * delta.ln();
    Ok(I2cPullupOut {
        resistor_ohm: tr / denom,
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrystalCapsOut {
    pub capacitor_pf: f64,
    pub accuracy: Accuracy,
}

pub fn crystal_load_capacitors(cload: f64, cstray: f64) -> Result<CrystalCapsOut, CalcError> {
    check_range("cload", cload, 6.0, 30.0)?;
    check_range("cstray", cstray, 0.0, 10.0)?;
    Ok(CrystalCapsOut {
        capacitor_pf: 2.0 * (cload - cstray),
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timer555Out {
    pub frequency_hz: f64,
    pub accuracy: Accuracy,
}

pub fn timer_555(ra: f64, rb: f64, c: f64) -> Result<Timer555Out, CalcError> {
    check_range("ra", ra, 1000.0, 1e7)?;
    check_range("rb", rb, 1000.0, 1e7)?;
    check_range("c", c, 1e-9, 100e-6)?;
    Ok(Timer555Out {
        frequency_hz: 1.44 / ((ra + 2.0 * rb) * c),
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpampGainOut {
    pub gain: f64,
    pub accuracy: Accuracy,
}

pub fn opamp_gain(r1: f64, r2: f64, inverting: f64) -> Result<OpampGainOut, CalcError> {
    check_range("r1", r1, 1.0, 1e9)?;
    check_range("r2", r2, 1.0, 1e9)?;
    check_range("inverting", inverting, 0.0, 1.0)?;
    let gain = if inverting >= 0.5 {
        -r2 / r1
    } else {
        1.0 + r2 / r1
    };
    Ok(OpampGainOut {
        gain,
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LdoThermalOut {
    pub theta_c_per_w: f64,
    pub accuracy: Accuracy,
}

pub fn ldo_thermal(power: f64, ambient: f64, max_junction: f64) -> Result<LdoThermalOut, CalcError> {
    check_range("power", power, 0.01, 10.0)?;
    check_range("ambient", ambient, -40.0, 85.0)?;
    check_range("max_junction", max_junction, 25.0, 150.0)?;
    check_positive("power", power)?;
    Ok(LdoThermalOut {
        theta_c_per_w: (max_junction - ambient) / power,
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatteryLifeOut {
    pub hours: f64,
    pub accuracy: Accuracy,
}

pub fn battery_life(capacity_mah: f64, current_ma: f64) -> Result<BatteryLifeOut, CalcError> {
    check_range("capacity_mah", capacity_mah, 10.0, 50_000.0)?;
    check_range("current_ma", current_ma, 0.01, 5000.0)?;
    check_positive("current_ma", current_ma)?;
    Ok(BatteryLifeOut {
        hours: capacity_mah / current_ma,
        accuracy: Accuracy::Estimate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResistorCodeOut {
    pub resistance_ohm: f64,
    pub accuracy: Accuracy,
}

pub fn resistor_code(digits: f64) -> Result<ResistorCodeOut, CalcError> {
    check_range("digits", digits, 100.0, 999.0)?;
    let d = digits.round() as i32;
    let a = d / 100;
    let b = (d / 10) % 10;
    let c = d % 10;
    if a < 1 || a > 9 {
        return Err(CalcError::Domain {
            message: "first digit must be 1 to 9",
        });
    }
    let base = (a * 10 + b) as f64;
    let mult = 10f64.powi(c);
    Ok(ResistorCodeOut {
        resistance_ohm: base * mult,
        accuracy: Accuracy::Exact,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ESeriesOut {
    pub nearest_ohm: f64,
    pub accuracy: Accuracy,
}

pub fn e_series(value: f64) -> Result<ESeriesOut, CalcError> {
    check_range("value", value, 0.1, 1e9)?;
    check_positive("value", value)?;
    Ok(ESeriesOut {
        nearest_ohm: nearest_e12(value),
        accuracy: Accuracy::Exact,
    })
}
