//! Pure-Rust calculator engine for the Help reference panel.
//!
//! Formulas, validity ranges, and golden vectors are driven by
//! `data/calculators.toml`.

pub mod basic;
pub mod components;
pub mod copper;
pub mod error;
pub mod impedance;
pub mod spacing;
pub mod utilities;

pub mod dispatch;
pub mod glossary;

mod catalog_generated {
    include!(concat!(env!("OUT_DIR"), "/catalog.rs"));
}

pub mod catalog {
    pub use super::catalog_generated::{CalculatorSpec, CALCULATORS};
}

mod consts;
pub use consts::{ALPHA_CU, C0, K_CU, MIL_MM, OZ_UM, RHO_CU, ZF0};

pub use error::CalcError;

/// How closely a result matches an ideal formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accuracy {
    Exact,
    Model,
    Estimate,
}

pub use basic::{
    decibel, lc_resonance, led_resistor, ohms_law, rc_rl_time_constant, reactance, series_parallel,
    voltage_divider,
};
pub use components::{
    battery_life, crystal_load_capacitors, decoupling_capacitor, e_series, i2c_pullup, ldo_thermal,
    opamp_gain, resistor_code, timer_555,
};
pub use copper::{
    thermal_via_count, trace_resistance_drop, trace_width, via_current_resistance, wire_gauge,
};
pub use impedance::{
    coplanar_waveguide, crosstalk, differential_pair, embedded_microstrip, microstrip,
    propagation_delay, stripline, width_for_target_ohms_microstrip,
};
pub use spacing::{conductor_spacing_ipc2221, creepage_clearance_iec60664};
pub use utilities::unit_converter;

pub use catalog::CALCULATORS;
pub use glossary::{search_glossary, term_by_id, GlossaryHit, Term, TERMS};
