//! Replays every golden vector in `data/calculators.toml` through the
//! calculators and cross-checks `meta::CALCULATORS` against the data
//! file. Tolerances come from the vectors themselves; a tolerance of
//! 0 means the expected value must match exactly.
use pcbcraft_calc::basic;
use pcbcraft_calc::components;
use pcbcraft_calc::copper;
use pcbcraft_calc::impedance;
use pcbcraft_calc::meta;
use pcbcraft_calc::spacing;
use pcbcraft_calc::utilities;
use serde::Deserialize;
use toml::Table;
use toml::Value;

#[derive(Deserialize)]
struct Doc {
    calculator: Vec<Calc>,
}

#[derive(Deserialize)]
struct Calc {
    id: String,
    name: String,
    group: String,
    summary: String,
    formula: String,
    source: String,
    validity: String,
    accuracy: String,
    inputs: Vec<InputDef>,
    #[serde(default)]
    vector: Vec<Vector>,
}

#[derive(Deserialize)]
struct InputDef {
    name: String,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    range: Option<[f64; 2]>,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    values: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Vector {
    inputs: Table,
    expect: Table,
    #[serde(default)]
    tolerance: Option<f64>,
}

fn doc() -> Result<Doc, String> {
    let text = include_str!("../data/calculators.toml");
    toml::from_str(text).map_err(|e| format!("data/calculators.toml: {e}"))
}

fn num_of(v: &Value) -> Option<f64> {
    match v {
        Value::Float(x) => Some(*x),
        Value::Integer(x) => Some(*x as f64),
        _ => None,
    }
}

fn need(inputs: &Table, key: &str) -> Result<f64, String> {
    inputs
        .get(key)
        .and_then(num_of)
        .ok_or_else(|| format!("{key}: missing or not a number"))
}

/// Required numeric input with the default the data file declares.
fn need_or(inputs: &Table, key: &str, default: f64) -> Result<f64, String> {
    match inputs.get(key) {
        None => Ok(default),
        Some(_) => need(inputs, key),
    }
}

fn maybe(inputs: &Table, key: &str) -> Result<Option<f64>, String> {
    match inputs.get(key) {
        None => Ok(None),
        Some(v) => num_of(v)
            .map(Some)
            .ok_or_else(|| format!("{key}: not a number")),
    }
}

fn need_i64(inputs: &Table, key: &str) -> Result<i64, String> {
    match inputs.get(key) {
        Some(Value::Integer(i)) => Ok(*i),
        Some(Value::Float(x)) if x.fract() == 0.0 => Ok(*x as i64),
        _ => Err(format!("{key}: missing or not an integer")),
    }
}

fn maybe_i64(inputs: &Table, key: &str) -> Result<Option<i64>, String> {
    if inputs.contains_key(key) {
        need_i64(inputs, key).map(Some)
    } else {
        Ok(None)
    }
}

fn flag(inputs: &Table, key: &str) -> Result<bool, String> {
    match inputs.get(key) {
        None => Ok(false),
        Some(Value::Boolean(b)) => Ok(*b),
        _ => Err(format!("{key}: not a boolean")),
    }
}

fn text(inputs: &Table, key: &str) -> Result<String, String> {
    match inputs.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(format!("{key}: missing or not text")),
    }
}

/// Required text input with the default the data file declares.
fn text_or(inputs: &Table, key: &str, default: &str) -> Result<String, String> {
    match inputs.get(key) {
        None => Ok(default.to_string()),
        Some(_) => text(inputs, key),
    }
}

fn maybe_text(inputs: &Table, key: &str) -> Result<Option<String>, String> {
    match inputs.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(format!("{key}: not text")),
    }
}

fn list(inputs: &Table, key: &str) -> Result<Vec<f64>, String> {
    match inputs.get(key) {
        Some(Value::Array(a)) => a
            .iter()
            .map(num_of)
            .collect::<Option<Vec<f64>>>()
            .ok_or_else(|| format!("{key}: array has a non-numeric entry")),
        _ => Err(format!("{key}: missing or not an array")),
    }
}

fn close(name: &str, got: f64, want: f64, tol: f64) -> Result<(), String> {
    let margin = tol.max(1e-4 * want.abs());
    if got.is_finite() && (got - want).abs() <= margin {
        Ok(())
    } else {
        Err(format!(
            "{name}: got {got}, expected {want} (margin {margin})"
        ))
    }
}

fn ck(expect: &Table, name: &str, got: f64, tol: f64) -> Result<(), String> {
    match expect.get(name).and_then(num_of) {
        Some(want) => close(name, got, want, tol),
        None => Ok(()),
    }
}

fn ck_opt(expect: &Table, name: &str, got: Option<f64>, tol: f64) -> Result<(), String> {
    match expect.get(name) {
        None => Ok(()),
        Some(w) => {
            let want = num_of(w).ok_or_else(|| format!("{name}: expectation not a number"))?;
            let got = got.ok_or_else(|| format!("{name}: calculator produced no value"))?;
            close(name, got, want, tol)
        }
    }
}

fn ck_i64(expect: &Table, name: &str, got: i64) -> Result<(), String> {
    match expect.get(name).and_then(num_of) {
        Some(want) => close(name, got as f64, want, 0.0),
        None => Ok(()),
    }
}

fn parse_component_kind(inputs: &Table) -> Result<basic::ComponentKind, String> {
    match text(inputs, "kind")?.as_str() {
        "resistor" => Ok(basic::ComponentKind::Resistor),
        "capacitor" => Ok(basic::ComponentKind::Capacitor),
        "inductor" => Ok(basic::ComponentKind::Inductor),
        other => Err(format!("kind: unknown {other}")),
    }
}

fn parse_rc_rl_kind(inputs: &Table) -> Result<basic::RcRlKind, String> {
    match text(inputs, "kind")?.as_str() {
        "rc" => Ok(basic::RcRlKind::Rc),
        "rl" => Ok(basic::RcRlKind::Rl),
        other => Err(format!("kind: unknown {other}")),
    }
}

fn parse_decibel_mode(inputs: &Table) -> Result<basic::DecibelMode, String> {
    match text(inputs, "mode")?.as_str() {
        "power_ratio" => Ok(basic::DecibelMode::PowerRatio),
        "voltage_ratio" => Ok(basic::DecibelMode::VoltageRatio),
        "dbm_to_w" => Ok(basic::DecibelMode::DbmToW),
        "w_to_dbm" => Ok(basic::DecibelMode::WToDbm),
        other => Err(format!("mode: unknown {other}")),
    }
}

fn parse_layer(inputs: &Table) -> Result<copper::CopperLayer, String> {
    match text(inputs, "layer")?.as_str() {
        "external" => Ok(copper::CopperLayer::External),
        "internal" => Ok(copper::CopperLayer::Internal),
        other => Err(format!("layer: unknown {other}")),
    }
}

fn parse_method(inputs: &Table) -> Result<copper::TraceMethod, String> {
    match text(inputs, "method")?.as_str() {
        "auto" => Ok(copper::TraceMethod::Auto),
        "ipc2152" => Ok(copper::TraceMethod::Ipc2152),
        "ipc2221" => Ok(copper::TraceMethod::Ipc2221),
        other => Err(format!("method: unknown {other}")),
    }
}

fn parse_location(inputs: &Table) -> Result<spacing::SpacingLocation, String> {
    match text(inputs, "location")?.as_str() {
        "B1" => Ok(spacing::SpacingLocation::B1),
        "B2" => Ok(spacing::SpacingLocation::B2),
        "B3" => Ok(spacing::SpacingLocation::B3),
        "B4" => Ok(spacing::SpacingLocation::B4),
        "A5" => Ok(spacing::SpacingLocation::A5),
        "A6" => Ok(spacing::SpacingLocation::A6),
        "A7" => Ok(spacing::SpacingLocation::A7),
        other => Err(format!("location: unknown {other}")),
    }
}

fn parse_ovc(inputs: &Table) -> Result<spacing::OvervoltageCategory, String> {
    match text(inputs, "overvoltage_category")?.as_str() {
        "I" => Ok(spacing::OvervoltageCategory::I),
        "II" => Ok(spacing::OvervoltageCategory::II),
        "III" => Ok(spacing::OvervoltageCategory::III),
        "IV" => Ok(spacing::OvervoltageCategory::IV),
        other => Err(format!("overvoltage_category: unknown {other}")),
    }
}

fn parse_pollution(inputs: &Table) -> Result<spacing::PollutionDegree, String> {
    match text(inputs, "pollution_degree")?.as_str() {
        "1" => Ok(spacing::PollutionDegree::D1),
        "2" => Ok(spacing::PollutionDegree::D2),
        "3" => Ok(spacing::PollutionDegree::D3),
        other => Err(format!("pollution_degree: unknown {other}")),
    }
}

fn parse_material(inputs: &Table) -> Result<spacing::MaterialGroup, String> {
    match text(inputs, "material_group")?.as_str() {
        "I" => Ok(spacing::MaterialGroup::G1),
        "II" => Ok(spacing::MaterialGroup::G2),
        "IIIa" => Ok(spacing::MaterialGroup::G3A),
        "IIIb" => Ok(spacing::MaterialGroup::G3B),
        other => Err(format!("material_group: unknown {other}")),
    }
}

fn parse_insulation(inputs: &Table) -> Result<spacing::Insulation, String> {
    match text(inputs, "insulation")?.as_str() {
        "functional" => Ok(spacing::Insulation::Functional),
        "basic" => Ok(spacing::Insulation::Basic),
        "reinforced" => Ok(spacing::Insulation::Reinforced),
        other => Err(format!("insulation: unknown {other}")),
    }
}

fn parse_timer_mode(inputs: &Table) -> Result<components::Timer555Mode, String> {
    match text(inputs, "mode")?.as_str() {
        "astable" => Ok(components::Timer555Mode::Astable),
        "monostable" => Ok(components::Timer555Mode::Monostable),
        other => Err(format!("mode: unknown {other}")),
    }
}

fn parse_topology(inputs: &Table) -> Result<components::OpampTopology, String> {
    match text(inputs, "topology")?.as_str() {
        "inverting" => Ok(components::OpampTopology::Inverting),
        "non_inverting" => Ok(components::OpampTopology::NonInverting),
        other => Err(format!("topology: unknown {other}")),
    }
}

fn parse_rating(inputs: &Table) -> Result<components::InsulationRating, String> {
    match text_or(inputs, "insulation_rating", "75C")?.as_str() {
        "60C" => Ok(components::InsulationRating::C60),
        "75C" => Ok(components::InsulationRating::C75),
        "90C" => Ok(components::InsulationRating::C90),
        other => Err(format!("insulation_rating: unknown {other}")),
    }
}

fn check_one(id: &str, i: &Table, e: &Table, t: f64) -> Result<(), String> {
    match id {
        "ohms_law" => {
            let out = basic::ohms_law(basic::OhmsLaw {
                voltage: maybe(i, "voltage")?,
                current: maybe(i, "current")?,
                resistance: maybe(i, "resistance")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "voltage", out.voltage, t)?;
            ck(e, "current", out.current, t)?;
            ck(e, "resistance", out.resistance, t)?;
            ck(e, "power", out.power, t)
        }
        "series_parallel" => {
            let out = basic::series_parallel(basic::SeriesParallel {
                kind: parse_component_kind(i)?,
                values: list(i, "values")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "series", out.series, t)?;
            ck(e, "parallel", out.parallel, t)
        }
        "voltage_divider" => {
            let out = basic::voltage_divider(basic::VoltageDivider {
                vin: need(i, "vin")?,
                r1: need(i, "r1")?,
                r2: need(i, "r2")?,
                load: maybe(i, "load")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "vout", out.vout, t)?;
            ck(e, "current", out.current, t)
        }
        "led_resistor" => {
            let out = basic::led_resistor(basic::LedResistor {
                supply: need(i, "supply")?,
                forward_voltage: need(i, "forward_voltage")?,
                current: need(i, "current")?,
                series: basic::ESeries::E24,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "resistance", out.resistance, t)?;
            ck(e, "power", out.power, t)?;
            ck(e, "nearest_standard", out.nearest_standard, t)
        }
        "rc_rl_time_constant" => {
            let out = basic::rc_rl_time_constant(basic::RcRl {
                kind: parse_rc_rl_kind(i)?,
                resistance: need(i, "resistance")?,
                capacitance: maybe(i, "capacitance")?,
                inductance: maybe(i, "inductance")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "tau", out.tau, t)?;
            ck(e, "cutoff", out.cutoff, t)
        }
        "lc_resonance" => {
            let out = basic::lc_resonance(basic::LcResonance {
                inductance: maybe(i, "inductance")?,
                capacitance: maybe(i, "capacitance")?,
                frequency: maybe(i, "frequency")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "frequency", out.frequency, t)
        }
        "reactance" => {
            let out = basic::reactance(basic::Reactance {
                frequency: need(i, "frequency")?,
                capacitance: maybe(i, "capacitance")?,
                inductance: maybe(i, "inductance")?,
            })
            .map_err(|err| err.to_string())?;
            ck_opt(e, "xc", out.xc, t)?;
            ck_opt(e, "xl", out.xl, t)
        }
        "decibel" => {
            let out = basic::decibel(basic::Decibel {
                mode: parse_decibel_mode(i)?,
                value: need(i, "value")?,
            })
            .map_err(|err| err.to_string())?;
            ck_opt(e, "db", out.db, t)?;
            ck_opt(e, "dbm", out.dbm, t)?;
            ck_opt(e, "watts", out.watts, t)
        }
        "trace_width" => {
            let out = copper::trace_width(copper::TraceWidth {
                current: need(i, "current")?,
                temperature_rise: need(i, "temperature_rise")?,
                copper_weight: need(i, "copper_weight")?,
                layer: parse_layer(i)?,
                method: parse_method(i)?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "width_mm", out.width_mm, t)?;
            ck(e, "width_mil", out.width_mil, t)?;
            ck(e, "cross_section", out.cross_section, t)
        }
        "via_current_resistance" => {
            let out = copper::via_current_resistance(copper::ViaCurrent {
                drill: need(i, "drill")?,
                plating: need(i, "plating")?,
                length: need(i, "length")?,
                temperature_rise: need(i, "temperature_rise")?,
                target_current: maybe(i, "target_current")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "barrel_area", out.barrel_area, t)?;
            ck(e, "current", out.current, t)?;
            ck(e, "resistance", out.resistance, t)?;
            ck_opt(e, "count", out.count, t)
        }
        "trace_resistance_drop" => {
            let out = copper::trace_resistance_drop(copper::TraceResistance {
                width: need(i, "width")?,
                copper_weight: need(i, "copper_weight")?,
                length: need(i, "length")?,
                current: need(i, "current")?,
                temperature: need(i, "temperature")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "resistance", out.resistance, t)?;
            ck(e, "voltage_drop", out.voltage_drop, t)?;
            ck(e, "power", out.power, t)
        }
        "microstrip" => {
            let out = impedance::microstrip(impedance::Microstrip {
                width: maybe(i, "width")?,
                height: need(i, "height")?,
                er: need(i, "er")?,
                thickness: need(i, "thickness")?,
                target: maybe(i, "target")?,
            })
            .map_err(|err| err.to_string())?;
            ck_opt(e, "z0", out.z0, t)?;
            ck_opt(e, "eeff", out.eeff, t)?;
            ck_opt(e, "delay", out.delay, t)?;
            ck_opt(e, "width_for_target", out.width_for_target, t)
        }
        "embedded_microstrip" => {
            let out = impedance::embedded_microstrip(impedance::EmbeddedMicrostrip {
                width: need(i, "width")?,
                height: need(i, "height")?,
                cover: need(i, "cover")?,
                er: need(i, "er")?,
                thickness: need(i, "thickness")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "z0", out.z0, t)?;
            ck(e, "eeff", out.eeff, t)
        }
        "stripline" => {
            let out = impedance::stripline(impedance::Stripline {
                width: need(i, "width")?,
                spacing: need(i, "spacing")?,
                er: need(i, "er")?,
                thickness: need(i, "thickness")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "z0", out.z0, t)
        }
        "coplanar_waveguide" => {
            let out = impedance::coplanar_waveguide(impedance::CoplanarWaveguide {
                width: need(i, "width")?,
                gap: need(i, "gap")?,
                height: need(i, "height")?,
                er: need(i, "er")?,
                ground_backed: flag(i, "ground_backed")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "z0", out.z0, t)?;
            ck(e, "eeff", out.eeff, t)
        }
        "differential_pair" => {
            let out = impedance::differential_pair(impedance::DifferentialPair {
                width: need(i, "width")?,
                gap: need(i, "gap")?,
                height: need(i, "height")?,
                er: need(i, "er")?,
                thickness: need(i, "thickness")?,
                target: maybe(i, "target")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "zdiff", out.zdiff, t)?;
            ck(e, "z0_single", out.z0_single, t)
        }
        "propagation_delay" => {
            let out = impedance::propagation_delay(impedance::PropagationDelay {
                eeff: need(i, "eeff")?,
                length: maybe(i, "length")?,
                skew_budget: maybe(i, "skew_budget")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "delay", out.delay, t)?;
            ck(e, "delay_inch", out.delay_inch, t)?;
            ck_opt(e, "total_delay", out.total_delay, t)?;
            ck_opt(e, "allowed_mismatch", out.allowed_mismatch, t)
        }
        "crosstalk" => {
            let out = impedance::crosstalk(impedance::Crosstalk {
                gap: need(i, "gap")?,
                height: need(i, "height")?,
                width: need(i, "width")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "coupling", out.coupling, t)?;
            ck(e, "coupling_db", out.coupling_db, t)?;
            ck(e, "three_w_gap", out.three_w_gap, t)
        }
        "conductor_spacing_ipc2221" => {
            let out = spacing::conductor_spacing_ipc2221(spacing::ConductorSpacing {
                voltage: need(i, "voltage")?,
                location: parse_location(i)?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "spacing", out.spacing, t)
        }
        "creepage_clearance_iec60664" => {
            let out = spacing::creepage_clearance_iec60664(spacing::CreepageClearance {
                working_voltage: need(i, "working_voltage")?,
                mains_voltage: need(i, "mains_voltage")?,
                overvoltage_category: parse_ovc(i)?,
                pollution_degree: parse_pollution(i)?,
                material_group: parse_material(i)?,
                insulation: parse_insulation(i)?,
                altitude: 0.0,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "impulse_voltage", out.impulse_voltage, t)?;
            ck(e, "clearance", out.clearance, t)?;
            ck(e, "creepage", out.creepage, t)
        }
        "decoupling_capacitor" => {
            let out = components::decoupling_capacitor(components::Decoupling {
                current_step: need(i, "current_step")?,
                step_time: need(i, "step_time")?,
                ripple: need(i, "ripple")?,
                esl: need_or(i, "esl", 1.0e-9)?,
                capacitance: maybe(i, "capacitance")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "capacitance_needed", out.capacitance_needed, t)?;
            ck(e, "impedance_target", out.impedance_target, t)?;
            ck_opt(e, "self_resonance", out.self_resonance, t)
        }
        "i2c_pullup" => {
            let out = components::i2c_pullup(components::I2cPullup {
                vcc: need(i, "vcc")?,
                vol_max: need(i, "vol_max")?,
                iol: need(i, "iol")?,
                rise_time: need(i, "rise_time")?,
                bus_capacitance: need(i, "bus_capacitance")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "rp_min", out.rp_min, t)?;
            ck(e, "rp_max", out.rp_max, t)
        }
        "crystal_load_capacitors" => {
            let out = components::crystal_load_capacitors(components::CrystalCaps {
                load_capacitance: need(i, "load_capacitance")?,
                stray_capacitance: need(i, "stray_capacitance")?,
                c1: maybe(i, "c1")?,
                c2: maybe(i, "c2")?,
            })
            .map_err(|err| err.to_string())?;
            ck_opt(e, "c1_c2", out.c1_c2, t)
        }
        "timer_555" => {
            let out = components::timer_555(components::Timer555 {
                mode: parse_timer_mode(i)?,
                r1: need(i, "r1")?,
                r2: maybe(i, "r2")?,
                c: need(i, "c")?,
            })
            .map_err(|err| err.to_string())?;
            ck_opt(e, "frequency", out.frequency, t)?;
            ck_opt(e, "duty_cycle", out.duty_cycle, t)?;
            ck_opt(e, "pulse_width", out.pulse_width, t)
        }
        "opamp_gain" => {
            let out = components::opamp_gain(components::OpampGain {
                topology: parse_topology(i)?,
                rf: need(i, "rf")?,
                rin: need(i, "rin")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "gain", out.gain, t)?;
            ck(e, "gain_db", out.gain_db, t)
        }
        "ldo_thermal" => {
            let out = components::ldo_thermal(components::LdoThermal {
                vin: need(i, "vin")?,
                vout: need(i, "vout")?,
                iout: need(i, "iout")?,
                iq: need(i, "iq")?,
                theta_ja: need(i, "theta_ja")?,
                ambient: need(i, "ambient")?,
                tj_max: 125.0,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "power", out.power, t)?;
            ck(e, "rise", out.rise, t)?;
            ck(e, "tj", out.tj, t)
        }
        "battery_life" => {
            let out = components::battery_life(components::BatteryLife {
                capacity: need(i, "capacity")?,
                active_current: need(i, "active_current")?,
                sleep_current: need_or(i, "sleep_current", 0.0)?,
                active_fraction: need(i, "active_fraction")?,
                usable_fraction: need(i, "usable_fraction")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "average_current", out.average_current, t)?;
            ck(e, "hours", out.hours, t)?;
            ck(e, "days", out.days, t)
        }
        "wire_gauge" => {
            let out = components::wire_gauge(components::WireGauge {
                awg: maybe_i64(i, "awg")?,
                current: maybe(i, "current")?,
                insulation_rating: parse_rating(i)?,
            })
            .map_err(|err| err.to_string())?;
            ck_opt(e, "diameter", out.diameter, t)?;
            ck_opt(e, "area", out.area, t)?;
            ck_opt(e, "resistance", out.resistance, t)?;
            ck_opt(e, "ampacity", out.ampacity, t)?;
            match (e.get("awg"), out.awg) {
                (Some(want), Some(got)) => match num_of(want) {
                    Some(w) => close("awg", got as f64, w, t),
                    None => Err("awg: expectation not a number".to_string()),
                },
                (Some(_), None) => Err("awg: calculator produced no value".to_string()),
                (None, _) => Ok(()),
            }
        }
        "resistor_code" => {
            let bands: Vec<String> = match i.get("bands") {
                None => Vec::new(),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|b| match b {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect::<Option<Vec<String>>>()
                    .ok_or_else(|| "bands: non-text entry".to_string())?,
                _ => return Err("bands: not an array".to_string()),
            };
            let out = components::resistor_code(components::ResistorCode {
                bands,
                smd_code: maybe_text(i, "smd_code")?,
                value: maybe(i, "value")?,
            })
            .map_err(|err| err.to_string())?;
            ck_opt(e, "resistance", out.resistance, t)?;
            ck_opt(e, "tolerance", out.tolerance, t)
        }
        "e_series" => {
            let series = components::Series::parse(&text(i, "series")?)
                .ok_or_else(|| "series: unknown name".to_string())?;
            let out = components::e_series(components::ESeriesInput {
                value: need(i, "value")?,
                series,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "nearest", out.nearest, t)
        }
        "unit_converter" => {
            let from = utilities::LengthUnit::parse(&text(i, "from")?)
                .ok_or_else(|| "from: unknown unit".to_string())?;
            let to = utilities::LengthUnit::parse(&text(i, "to")?)
                .ok_or_else(|| "to: unknown unit".to_string())?;
            let out = utilities::unit_converter(utilities::UnitConvert {
                value: need(i, "value")?,
                from,
                to,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "value", out.value, t)
        }
        "thermal_via_count" => {
            let out = utilities::thermal_via_count(utilities::ThermalVias {
                drill: need(i, "drill")?,
                plating: need(i, "plating")?,
                length: need(i, "length")?,
                target_rth: need(i, "target_rth")?,
                filled: flag(i, "filled")?,
            })
            .map_err(|err| err.to_string())?;
            ck(e, "rth_one_via", out.rth_one_via, t)?;
            ck_i64(e, "count", out.count)
        }
        other => Err(format!("no dispatch for calculator {other}")),
    }
}

fn replay(id: &str) -> Result<(), String> {
    let d = doc()?;
    let calc = d
        .calculator
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("no calculator {id} in the data file"))?;
    let mut failures: Vec<String> = Vec::new();
    for (n, v) in calc.vector.iter().enumerate() {
        let tol = v.tolerance.unwrap_or(1e-9);
        if let Err(err) = check_one(id, &v.inputs, &v.expect, tol) {
            failures.push(format!("#{n}: {err}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

macro_rules! golden_test {
    ($name:ident, $id:literal) => {
        #[test]
        fn $name() -> Result<(), String> {
            replay($id)
        }
    };
}

golden_test!(ohms_law_vectors, "ohms_law");
golden_test!(series_parallel_vectors, "series_parallel");
golden_test!(voltage_divider_vectors, "voltage_divider");
golden_test!(led_resistor_vectors, "led_resistor");
golden_test!(rc_rl_time_constant_vectors, "rc_rl_time_constant");
golden_test!(lc_resonance_vectors, "lc_resonance");
golden_test!(reactance_vectors, "reactance");
golden_test!(decibel_vectors, "decibel");
golden_test!(trace_width_vectors, "trace_width");
golden_test!(via_current_resistance_vectors, "via_current_resistance");
golden_test!(trace_resistance_drop_vectors, "trace_resistance_drop");
golden_test!(microstrip_vectors, "microstrip");
golden_test!(embedded_microstrip_vectors, "embedded_microstrip");
golden_test!(stripline_vectors, "stripline");
golden_test!(coplanar_waveguide_vectors, "coplanar_waveguide");
golden_test!(differential_pair_vectors, "differential_pair");
golden_test!(propagation_delay_vectors, "propagation_delay");
golden_test!(crosstalk_vectors, "crosstalk");
golden_test!(
    conductor_spacing_ipc2221_vectors,
    "conductor_spacing_ipc2221"
);
golden_test!(
    creepage_clearance_iec60664_vectors,
    "creepage_clearance_iec60664"
);
golden_test!(decoupling_capacitor_vectors, "decoupling_capacitor");
golden_test!(i2c_pullup_vectors, "i2c_pullup");
golden_test!(crystal_load_capacitors_vectors, "crystal_load_capacitors");
golden_test!(timer_555_vectors, "timer_555");
golden_test!(opamp_gain_vectors, "opamp_gain");
golden_test!(ldo_thermal_vectors, "ldo_thermal");
golden_test!(battery_life_vectors, "battery_life");
golden_test!(wire_gauge_vectors, "wire_gauge");
golden_test!(resistor_code_vectors, "resistor_code");
golden_test!(e_series_vectors, "e_series");
golden_test!(unit_converter_vectors, "unit_converter");
golden_test!(thermal_via_count_vectors, "thermal_via_count");

#[test]
fn data_file_has_every_vector_replayed() -> Result<(), String> {
    let d = doc()?;
    let total: usize = d.calculator.iter().map(|c| c.vector.len()).sum();
    if total != 119 {
        return Err(format!("data file carries {total} vectors, expected 119"));
    }
    if d.calculator.len() != meta::CALCULATORS.len() {
        return Err(format!(
            "{} calculators in the data file, {} in meta",
            d.calculator.len(),
            meta::CALCULATORS.len()
        ));
    }
    for c in &d.calculator {
        if c.vector.is_empty() {
            return Err(format!("{} has no vectors", c.id));
        }
        if meta::calculator(&c.id).is_none() {
            return Err(format!("{} missing from meta", c.id));
        }
    }
    Ok(())
}

#[test]
fn metadata_mirrors_the_data_file() -> Result<(), String> {
    let d = doc()?;
    for c in &d.calculator {
        let m = meta::calculator(&c.id).ok_or_else(|| format!("{} missing from meta", c.id))?;
        for (field, got, want) in [
            ("name", m.name, c.name.as_str()),
            ("group", m.group, c.group.as_str()),
            ("summary", m.summary, c.summary.as_str()),
            ("formula", m.formula, c.formula.as_str()),
            ("source", m.source, c.source.as_str()),
            ("validity", m.validity, c.validity.as_str()),
            ("accuracy", m.accuracy, c.accuracy.as_str()),
        ] {
            if got != want {
                return Err(format!("{}: meta {field} differs from the data file", c.id));
            }
        }
        if m.inputs.len() != c.inputs.len() {
            return Err(format!(
                "{}: {} meta inputs, {} data inputs",
                c.id,
                m.inputs.len(),
                c.inputs.len()
            ));
        }
        for (mi, di) in m.inputs.iter().zip(&c.inputs) {
            if mi.name != di.name || mi.optional != di.optional {
                return Err(format!("{}: input {} mismatched", c.id, di.name));
            }
            let want_unit = if di.values.is_some() || di.unit == "bool" {
                "enum"
            } else {
                di.unit.as_str()
            };
            if mi.unit != want_unit {
                return Err(format!("{}: input {} unit mismatched", c.id, di.name));
            }
            let (dmin, dmax) = match di.range {
                Some([lo, hi]) => (Some(lo), Some(hi)),
                None => (None, None),
            };
            if mi.min != dmin || mi.max != dmax {
                return Err(format!("{}: input {} range mismatched", c.id, di.name));
            }
            let want_values: Vec<&str> = match &di.values {
                Some(vs) => vs.iter().map(String::as_str).collect(),
                None if di.unit == "bool" => vec!["true", "false"],
                None => vec![],
            };
            if mi.values.len() != want_values.len()
                || mi.values.iter().zip(&want_values).any(|(a, b)| a != b)
            {
                return Err(format!("{}: input {} values mismatched", c.id, di.name));
            }
        }
    }
    Ok(())
}
