//! Handlers for net electrical property commands.
use crate::Error;
use pcbcraft_doc::{
    Net, NetClass, Project, validate_current, validate_impedance_target,
    validate_impedance_tolerance_pct, validate_insulation, validate_line_structure,
    validate_reference_layers, validate_return_path_rule, validate_temperature_rise,
    validate_working_voltage,
};
use serde_json::{Value, json};

const CLASS_PROPERTY_KEYS: &[&str] = &[
    "impedance_target",
    "impedance_tolerance_pct",
    "line_structure",
    "reference_layers",
    "current",
    "temperature_rise",
    "working_voltage",
    "insulation",
    "return_path_rule",
];

pub fn class_property_params_touch(params: &Value) -> bool {
    CLASS_PROPERTY_KEYS.iter().any(|key| params.get(key).is_some())
}

fn missing() -> Error {
    Error::Invalid("object not found".into())
}

fn optional_f64(params: &Value, key: &str, validate: fn(f64) -> Result<(), String>) -> Result<Option<Option<f64>>, Error> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(v) => {
            let n = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| {
                Error::Invalid(format!("{key} must be a finite number or null"))
            })?;
            validate(n).map_err(Error::Invalid)?;
            Ok(Some(Some(n)))
        }
    }
}

fn optional_string(
    params: &Value,
    key: &str,
    validate: fn(&str) -> Result<(), String>,
) -> Result<Option<Option<String>>, Error> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) => {
            validate(s).map_err(Error::Invalid)?;
            Ok(Some(Some(s.clone())))
        }
        _ => Err(Error::Invalid(format!("{key} must be a string or null"))),
    }
}

fn optional_reference_layers(params: &Value, key: &str) -> Result<Option<Option<Vec<String>>>, Error> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::Array(items)) => {
            let mut layers = Vec::with_capacity(items.len());
            for item in items {
                let name = item.as_str().ok_or_else(|| {
                    Error::Invalid("each reference layer must be a string".into())
                })?;
                layers.push(name.to_string());
            }
            Ok(Some(Some(layers)))
        }
        _ => Err(Error::Invalid(
            "reference_layers must be an array of layer names or null".into(),
        )),
    }
}

fn apply_optional_f64(field: &mut Option<f64>, update: Option<Option<f64>>) {
    if let Some(value) = update {
        *field = value;
    }
}

fn apply_optional_string(field: &mut Option<String>, update: Option<Option<String>>) {
    if let Some(value) = update {
        *field = value;
    }
}

pub fn set_net_properties(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let id = params
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::Invalid("id is required".into()))?;
    let impedance_target = optional_f64(params, "impedance_target", validate_impedance_target)?;
    let impedance_tolerance_pct =
        optional_f64(params, "impedance_tolerance_pct", validate_impedance_tolerance_pct)?;
    let line_structure = optional_string(params, "line_structure", validate_line_structure)?;
    let reference_layers = optional_reference_layers(params, "reference_layers")?;
    let current = optional_f64(params, "current", validate_current)?;
    let temperature_rise =
        optional_f64(params, "temperature_rise", validate_temperature_rise)?;
    let working_voltage = optional_f64(params, "working_voltage", validate_working_voltage)?;
    let insulation = optional_string(params, "insulation", validate_insulation)?;
    let return_path_rule =
        optional_string(params, "return_path_rule", validate_return_path_rule)?;
    if impedance_target.is_none()
        && impedance_tolerance_pct.is_none()
        && line_structure.is_none()
        && reference_layers.is_none()
        && current.is_none()
        && temperature_rise.is_none()
        && working_voltage.is_none()
        && insulation.is_none()
        && return_path_rule.is_none()
    {
        return Err(Error::Invalid("give at least one property to change".into()));
    }
    if let Some(Some(layers)) = &reference_layers {
        validate_reference_layers(p, layers).map_err(Error::Invalid)?;
    }
    let net = p
        .nets
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or_else(missing)?;
    apply_net_properties(
        net,
        impedance_target,
        impedance_tolerance_pct,
        line_structure,
        reference_layers,
        current,
        temperature_rise,
        working_voltage,
        insulation,
        return_path_rule,
    )?;
    Ok(net_properties_json(p, net))
}

fn apply_net_properties(
    net: &mut Net,
    impedance_target: Option<Option<f64>>,
    impedance_tolerance_pct: Option<Option<f64>>,
    line_structure: Option<Option<String>>,
    reference_layers: Option<Option<Vec<String>>>,
    current: Option<Option<f64>>,
    temperature_rise: Option<Option<f64>>,
    working_voltage: Option<Option<f64>>,
    insulation: Option<Option<String>>,
    return_path_rule: Option<Option<String>>,
) -> Result<(), Error> {
    apply_optional_f64(&mut net.impedance_target, impedance_target);
    apply_optional_f64(&mut net.impedance_tolerance_pct, impedance_tolerance_pct);
    apply_optional_string(&mut net.line_structure, line_structure);
    if let Some(layers) = reference_layers {
        net.reference_layers = layers;
    }
    apply_optional_f64(&mut net.current, current);
    apply_optional_f64(&mut net.temperature_rise, temperature_rise);
    apply_optional_f64(&mut net.working_voltage, working_voltage);
    apply_optional_string(&mut net.insulation, insulation);
    apply_optional_string(&mut net.return_path_rule, return_path_rule);
    if net.impedance_tolerance_pct.is_some() && net.impedance_target.is_none() {
        return Err(Error::Invalid(
            "an impedance tolerance needs an impedance target on the net".into(),
        ));
    }
    Ok(())
}

pub fn apply_class_property_updates(
    p: &Project,
    class: &mut NetClass,
    params: &Value,
) -> Result<bool, Error> {
    let impedance_target = optional_f64(params, "impedance_target", validate_impedance_target)?;
    let impedance_tolerance_pct =
        optional_f64(params, "impedance_tolerance_pct", validate_impedance_tolerance_pct)?;
    let line_structure = optional_string(params, "line_structure", validate_line_structure)?;
    let reference_layers = optional_reference_layers(params, "reference_layers")?;
    let current = optional_f64(params, "current", validate_current)?;
    let temperature_rise =
        optional_f64(params, "temperature_rise", validate_temperature_rise)?;
    let working_voltage = optional_f64(params, "working_voltage", validate_working_voltage)?;
    let insulation = optional_string(params, "insulation", validate_insulation)?;
    let return_path_rule =
        optional_string(params, "return_path_rule", validate_return_path_rule)?;
    let touched = impedance_target.is_some()
        || impedance_tolerance_pct.is_some()
        || line_structure.is_some()
        || reference_layers.is_some()
        || current.is_some()
        || temperature_rise.is_some()
        || working_voltage.is_some()
        || insulation.is_some()
        || return_path_rule.is_some();
    if let Some(layers) = reference_layers {
        match layers {
            None => class.reference_layers.clear(),
            Some(list) => {
                validate_reference_layers(p, &list).map_err(Error::Invalid)?;
                class.reference_layers = list;
            }
        }
    }
    apply_optional_f64(&mut class.impedance_target, impedance_target);
    apply_optional_f64(&mut class.impedance_tolerance_pct, impedance_tolerance_pct);
    apply_optional_string(&mut class.line_structure, line_structure);
    apply_optional_f64(&mut class.current, current);
    apply_optional_f64(&mut class.temperature_rise, temperature_rise);
    apply_optional_f64(&mut class.working_voltage, working_voltage);
    apply_optional_string(&mut class.insulation, insulation);
    apply_optional_string(&mut class.return_path_rule, return_path_rule);
    if class.impedance_tolerance_pct.is_some() && class.impedance_target.is_none() {
        return Err(Error::Invalid(
            "an impedance tolerance needs an impedance target on the class".into(),
        ));
    }
    Ok(touched)
}

pub fn net_properties_json(p: &Project, net: &Net) -> Value {
    json!({
        "id": net.id,
        "impedance_target": net.impedance_target,
        "impedance_tolerance_pct": net.impedance_tolerance_pct,
        "line_structure": net.line_structure,
        "reference_layers": net.reference_layers,
        "current": net.current,
        "temperature_rise": net.temperature_rise,
        "working_voltage": net.working_voltage,
        "insulation": net.insulation,
        "return_path_rule": net.return_path_rule,
        "effective": {
            "impedance_target": p.net_effective_impedance_target(net),
            "impedance_tolerance_pct": p.net_effective_impedance_tolerance_pct(net),
            "line_structure": p.net_effective_line_structure(net),
            "reference_layers": p.net_effective_reference_layers(net),
            "current": p.net_effective_current(net),
            "temperature_rise": p.net_effective_temperature_rise(net),
            "working_voltage": p.net_effective_working_voltage(net),
            "insulation": p.net_effective_insulation(net),
            "return_path_rule": p.net_effective_return_path_rule(net),
        }
    })
}

pub fn class_properties_json(class: &NetClass) -> Value {
    json!({
        "impedance_target": class.impedance_target,
        "impedance_tolerance_pct": class.impedance_tolerance_pct,
        "line_structure": class.line_structure,
        "reference_layers": class.reference_layers,
        "current": class.current,
        "temperature_rise": class.temperature_rise,
        "working_voltage": class.working_voltage,
        "insulation": class.insulation,
        "return_path_rule": class.return_path_rule,
    })
}
