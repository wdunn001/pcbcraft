//! Golden-test dispatch from calculator id to public entry points.

use crate::basic::{
    decibel, lc_resonance, led_resistor, ohms_law, rc_rl_time_constant, reactance, series_parallel,
    voltage_divider,
};
use crate::components::{
    battery_life, crystal_load_capacitors, decoupling_capacitor, e_series, i2c_pullup, ldo_thermal,
    opamp_gain, resistor_code, timer_555,
};
use crate::copper::{
    thermal_via_count, trace_resistance_drop, trace_width, via_current_resistance, wire_gauge,
};
use crate::error::CalcError;
use crate::impedance::{
    coplanar_waveguide, crosstalk, differential_pair, embedded_microstrip, microstrip,
    propagation_delay, stripline, width_for_target_ohms_microstrip,
};
use crate::spacing::{conductor_spacing_ipc2221, creepage_clearance_iec60664};
use crate::utilities::unit_converter;
use std::collections::BTreeMap;

pub type OutputMap = BTreeMap<String, f64>;
pub type StringMap = BTreeMap<String, String>;

pub struct DispatchResult {
    pub numbers: OutputMap,
    pub strings: StringMap,
}

fn f(inputs: &toml::map::Map<String, toml::Value>, key: &str) -> Result<f64, CalcError> {
    let v = inputs.get(key).ok_or(CalcError::Domain {
        message: "missing numeric input",
    })?;
    v.as_float().ok_or(CalcError::Domain {
        message: "expected numeric input",
    })
}

fn s(inputs: &toml::map::Map<String, toml::Value>, key: &str) -> Result<&str, CalcError> {
    let v = inputs.get(key).ok_or(CalcError::Domain {
        message: "missing string input",
    })?;
    v.as_str().ok_or(CalcError::Domain {
        message: "expected string input",
    })
}

pub fn dispatch(id: &str, inputs: &toml::map::Map<String, toml::Value>) -> Result<DispatchResult, CalcError> {
    let mut numbers = OutputMap::new();
    let mut strings = StringMap::new();
    match id {
        "ohms_law" => {
            let out = ohms_law(f(inputs, "voltage")?, f(inputs, "current")?, f(inputs, "resistance")?)?;
            numbers.insert("voltage".into(), out.voltage);
            numbers.insert("current".into(), out.current);
            numbers.insert("resistance".into(), out.resistance);
        }
        "series_parallel" => {
            let out = series_parallel(f(inputs, "r1")?, f(inputs, "r2")?, f(inputs, "mode")?)?;
            numbers.insert("equivalent".into(), out.equivalent);
        }
        "voltage_divider" => {
            let out = voltage_divider(f(inputs, "vin")?, f(inputs, "r1")?, f(inputs, "r2")?)?;
            numbers.insert("vout".into(), out.vout);
        }
        "led_resistor" => {
            let out = led_resistor(f(inputs, "v_supply")?, f(inputs, "v_led")?, f(inputs, "i_led")?)?;
            numbers.insert("resistor".into(), out.resistor);
        }
        "rc_rl_time_constant" => {
            let out = rc_rl_time_constant(
                f(inputs, "resistance")?,
                f(inputs, "capacitance")?,
                f(inputs, "inductance")?,
            )?;
            numbers.insert("tau_seconds".into(), out.tau_seconds);
        }
        "lc_resonance" => {
            let out = lc_resonance(f(inputs, "inductance")?, f(inputs, "capacitance")?)?;
            numbers.insert("frequency_hz".into(), out.frequency_hz);
        }
        "reactance" => {
            let out = reactance(
                f(inputs, "frequency")?,
                f(inputs, "capacitance")?,
                f(inputs, "inductance")?,
            )?;
            numbers.insert("reactance_ohm".into(), out.reactance_ohm);
        }
        "decibel" => {
            let out = decibel(f(inputs, "ratio")?, f(inputs, "mode")?)?;
            numbers.insert("decibels".into(), out.decibels);
        }
        "trace_width" => {
            let out = trace_width(f(inputs, "current")?, f(inputs, "delta_t")?, f(inputs, "external")?)?;
            numbers.insert("width_mm".into(), out.width_mm);
            strings.insert("standard".into(), out.standard.as_str().into());
        }
        "via_current_resistance" => {
            let out = via_current_resistance(
                f(inputs, "diameter")?,
                f(inputs, "length")?,
                f(inputs, "current")?,
            )?;
            numbers.insert("resistance_mohm".into(), out.resistance_mohm);
        }
        "trace_resistance_drop" => {
            let out = trace_resistance_drop(
                f(inputs, "length")?,
                f(inputs, "width")?,
                f(inputs, "thickness")?,
                f(inputs, "current")?,
            )?;
            numbers.insert("resistance_mohm".into(), out.resistance_mohm);
            numbers.insert("voltage_drop_mv".into(), out.voltage_drop_mv);
        }
        "wire_gauge" => {
            let out = wire_gauge(f(inputs, "awg")?)?;
            numbers.insert("diameter_mm".into(), out.diameter_mm);
        }
        "thermal_via_count" => {
            let out = thermal_via_count(
                f(inputs, "power")?,
                f(inputs, "delta_t")?,
                f(inputs, "via_diameter")?,
            )?;
            numbers.insert("count".into(), out.count);
        }
        "microstrip" => {
            if inputs.contains_key("target_ohm") && f(inputs, "target_ohm")? > 0.0 {
                let out = width_for_target_ohms_microstrip(
                    f(inputs, "target_ohm")?,
                    f(inputs, "height")?,
                    f(inputs, "thickness")?,
                    f(inputs, "er")?,
                )?;
                numbers.insert("width_mm".into(), out.width_mm);
            } else {
                let out = microstrip(
                    f(inputs, "width")?,
                    f(inputs, "height")?,
                    f(inputs, "thickness")?,
                    f(inputs, "er")?,
                )?;
                numbers.insert("impedance_ohm".into(), out.impedance_ohm);
                numbers.insert("eeff".into(), out.eeff);
            }
        }
        "embedded_microstrip" => {
            let out = embedded_microstrip(
                f(inputs, "width")?,
                f(inputs, "height")?,
                f(inputs, "thickness")?,
                f(inputs, "er")?,
            )?;
            numbers.insert("impedance_ohm".into(), out.impedance_ohm);
            numbers.insert("eeff".into(), out.eeff);
        }
        "stripline" => {
            let out = stripline(
                f(inputs, "width")?,
                f(inputs, "plane_spacing")?,
                f(inputs, "thickness")?,
                f(inputs, "er")?,
            )?;
            numbers.insert("impedance_ohm".into(), out.impedance_ohm);
        }
        "coplanar_waveguide" => {
            let out = coplanar_waveguide(
                f(inputs, "width")?,
                f(inputs, "gap")?,
                f(inputs, "height")?,
                f(inputs, "er")?,
            )?;
            numbers.insert("impedance_ohm".into(), out.impedance_ohm);
        }
        "differential_pair" => {
            let out = differential_pair(
                f(inputs, "width")?,
                f(inputs, "gap")?,
                f(inputs, "height")?,
                f(inputs, "er")?,
            )?;
            numbers.insert("z_diff_ohm".into(), out.z_diff_ohm);
        }
        "propagation_delay" => {
            let out = propagation_delay(f(inputs, "length")?, f(inputs, "eeff")?)?;
            numbers.insert("delay_ps".into(), out.delay_ps);
        }
        "crosstalk" => {
            let out = crosstalk(f(inputs, "spacing")?, f(inputs, "height")?, f(inputs, "length")?)?;
            numbers.insert("next_db".into(), out.next_db);
        }
        "conductor_spacing_ipc2221" => {
            let out = conductor_spacing_ipc2221(f(inputs, "voltage")?, f(inputs, "coated")?)?;
            numbers.insert("spacing_mm".into(), out.spacing_mm);
        }
        "creepage_clearance_iec60664" => {
            let out = creepage_clearance_iec60664(f(inputs, "voltage")?, f(inputs, "pollution")?)?;
            numbers.insert("clearance_mm".into(), out.clearance_mm);
            numbers.insert("creepage_mm".into(), out.creepage_mm);
        }
        "decoupling_capacitor" => {
            let out = decoupling_capacitor(
                f(inputs, "delta_i")?,
                f(inputs, "delta_v")?,
                f(inputs, "delta_t")?,
            )?;
            numbers.insert("capacitance_f".into(), out.capacitance_f);
        }
        "i2c_pullup" => {
            let out = i2c_pullup(f(inputs, "voltage")?, f(inputs, "capacitance")?, f(inputs, "rise_ns")?)?;
            numbers.insert("resistor_ohm".into(), out.resistor_ohm);
        }
        "crystal_load_capacitors" => {
            let out = crystal_load_capacitors(f(inputs, "cload")?, f(inputs, "cstray")?)?;
            numbers.insert("capacitor_pf".into(), out.capacitor_pf);
        }
        "timer_555" => {
            let out = timer_555(f(inputs, "ra")?, f(inputs, "rb")?, f(inputs, "c")?)?;
            numbers.insert("frequency_hz".into(), out.frequency_hz);
        }
        "opamp_gain" => {
            let out = opamp_gain(f(inputs, "r1")?, f(inputs, "r2")?, f(inputs, "inverting")?)?;
            numbers.insert("gain".into(), out.gain);
        }
        "ldo_thermal" => {
            let out = ldo_thermal(f(inputs, "power")?, f(inputs, "ambient")?, f(inputs, "max_junction")?)?;
            numbers.insert("theta_c_per_w".into(), out.theta_c_per_w);
        }
        "battery_life" => {
            let out = battery_life(f(inputs, "capacity_mah")?, f(inputs, "current_ma")?)?;
            numbers.insert("hours".into(), out.hours);
        }
        "resistor_code" => {
            let out = resistor_code(f(inputs, "digits")?)?;
            numbers.insert("resistance_ohm".into(), out.resistance_ohm);
        }
        "e_series" => {
            let out = e_series(f(inputs, "value")?)?;
            numbers.insert("nearest_ohm".into(), out.nearest_ohm);
        }
        "unit_converter" => {
            let out = unit_converter(f(inputs, "value")?, s(inputs, "from_unit")?, s(inputs, "to_unit")?)?;
            numbers.insert("result".into(), out.result);
        }
        _ => {
            return Err(CalcError::Domain {
                message: "unknown calculator id",
            });
        }
    }
    Ok(DispatchResult { numbers, strings })
}
