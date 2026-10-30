//! Optional electrical properties on nets and net classes, with class defaults
//! and per-net overrides.
use crate::{Net, Project};

pub const IMPEDANCE_TARGET_MIN_OHM: f64 = 10.0;
pub const IMPEDANCE_TARGET_MAX_OHM: f64 = 200.0;
pub const IMPEDANCE_TOLERANCE_MAX_PCT: f64 = 50.0;
pub const CURRENT_MIN_A: f64 = 0.001;
pub const CURRENT_MAX_A: f64 = 500.0;
pub const TEMPERATURE_RISE_MIN_K: f64 = 1.0;
pub const TEMPERATURE_RISE_MAX_K: f64 = 100.0;
pub const WORKING_VOLTAGE_MIN_V: f64 = 10.0;
pub const WORKING_VOLTAGE_MAX_V: f64 = 1000.0;
pub const MAX_REFERENCE_LAYERS: usize = 2;

pub const LINE_STRUCTURES: &[&str] = &[
    "stripline",
    "microstrip",
    "microstrip_mask",
    "coplanar",
    "broadside",
];
pub const INSULATION_CLASSES: &[&str] = &["functional", "basic", "reinforced"];
pub const RETURN_PATH_RULES: &[&str] = &["continuous", "no_split", "same_net_plane"];

fn in_set(value: &str, allowed: &[&str]) -> bool {
    allowed.contains(&value)
}

pub fn validate_impedance_target(value: f64) -> Result<(), String> {
    if !value.is_finite() || value < IMPEDANCE_TARGET_MIN_OHM || value > IMPEDANCE_TARGET_MAX_OHM {
        return Err(format!(
            "impedance target must be between {IMEDANCE_TARGET_MIN_OHM} and {IMPEDANCE_TARGET_MAX_OHM} ohm"
        ));
    }
    Ok(())
}

pub fn validate_impedance_tolerance_pct(value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=IMPEDANCE_TOLERANCE_MAX_PCT).contains(&value) {
        return Err(format!(
            "impedance tolerance must be between 0 and {IMPEDANCE_TOLERANCE_MAX_PCT} percent"
        ));
    }
    Ok(())
}

pub fn validate_line_structure(value: &str) -> Result<(), String> {
    if in_set(value, LINE_STRUCTURES) {
        Ok(())
    } else {
        Err("line structure must be stripline, microstrip, microstrip_mask, coplanar or broadside".into())
    }
}

pub fn validate_insulation(value: &str) -> Result<(), String> {
    if in_set(value, INSULATION_CLASSES) {
        Ok(())
    } else {
        Err("insulation must be functional, basic or reinforced".into())
    }
}

pub fn validate_return_path_rule(value: &str) -> Result<(), String> {
    if in_set(value, RETURN_PATH_RULES) {
        Ok(())
    } else {
        Err("return path rule must be continuous, no_split or same_net_plane".into())
    }
}

pub fn validate_current(value: f64) -> Result<(), String> {
    if !value.is_finite() || value < CURRENT_MIN_A || value > CURRENT_MAX_A {
        return Err(format!(
            "current must be between {CURRENT_MIN_A} and {CURRENT_MAX_A} ampere"
        ));
    }
    Ok(())
}

pub fn validate_temperature_rise(value: f64) -> Result<(), String> {
    if !value.is_finite() || value < TEMPERATURE_RISE_MIN_K || value > TEMPERATURE_RISE_MAX_K {
        return Err(format!(
            "temperature rise must be between {TEMPERATURE_RISE_MIN_K} and {TEMPERATURE_RISE_MAX_K} kelvin"
        ));
    }
    Ok(())
}

pub fn validate_working_voltage(value: f64) -> Result<(), String> {
    if !value.is_finite() || value < WORKING_VOLTAGE_MIN_V || value > WORKING_VOLTAGE_MAX_V {
        return Err(format!(
            "working voltage must be between {WORKING_VOLTAGE_MIN_V} and {WORKING_VOLTAGE_MAX_V} volt"
        ));
    }
    Ok(())
}

pub fn validate_reference_layers(project: &Project, layers: &[String]) -> Result<(), String> {
    if layers.len() > MAX_REFERENCE_LAYERS {
        return Err(format!(
            "at most {MAX_REFERENCE_LAYERS} reference layers are allowed"
        ));
    }
    for name in layers {
        if name.is_empty() || name.len() > 64 {
            return Err("each reference layer name must be 1 to 64 bytes".into());
        }
        let Some(row) = project.board.layer_records.iter().find(|r| r.name == *name) else {
            return Err(format!("unknown layer record {name}"));
        };
        if row.kind != "copper" {
            return Err(format!("reference layer {name} must be a copper row"));
        }
    }
    Ok(())
}

impl Project {
    pub fn net_effective_impedance_target(&self, net: &Net) -> Option<f64> {
        net.impedance_target
            .or_else(|| self.net_class(net.id).and_then(|c| c.impedance_target))
    }

    pub fn net_effective_impedance_tolerance_pct(&self, net: &Net) -> Option<f64> {
        if net.impedance_tolerance_pct.is_some() {
            net.impedance_tolerance_pct
        } else {
            self.net_class(net.id)
                .and_then(|c| c.impedance_tolerance_pct)
        }
    }

    pub fn net_effective_line_structure(&self, net: &Net) -> Option<&str> {
        net.line_structure
            .as_deref()
            .or_else(|| {
                self.net_class(net.id)
                    .and_then(|c| c.line_structure.as_deref())
            })
    }

    pub fn net_effective_reference_layers(&self, net: &Net) -> &[String] {
        if let Some(layers) = &net.reference_layers {
            layers.as_slice()
        } else {
            self.net_class(net.id)
                .map(|c| c.reference_layers.as_slice())
                .unwrap_or(&[])
        }
    }

    pub fn net_effective_current(&self, net: &Net) -> Option<f64> {
        net.current
            .or_else(|| self.net_class(net.id).and_then(|c| c.current))
    }

    pub fn net_effective_temperature_rise(&self, net: &Net) -> Option<f64> {
        net.temperature_rise
            .or_else(|| self.net_class(net.id).and_then(|c| c.temperature_rise))
    }

    pub fn net_effective_working_voltage(&self, net: &Net) -> Option<f64> {
        net.working_voltage
            .or_else(|| self.net_class(net.id).and_then(|c| c.working_voltage))
    }

    pub fn net_effective_insulation(&self, net: &Net) -> Option<&str> {
        net.insulation
            .as_deref()
            .or_else(|| self.net_class(net.id).and_then(|c| c.insulation.as_deref()))
    }

    pub fn net_effective_return_path_rule(&self, net: &Net) -> Option<&str> {
        net.return_path_rule
            .as_deref()
            .or_else(|| {
                self.net_class(net.id)
                    .and_then(|c| c.return_path_rule.as_deref())
            })
    }
}
