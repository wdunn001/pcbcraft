//! Copper and current calculators: trace_width, via_current_resistance,
//! trace_resistance_drop.
//!
//! IPC-2221 constants and the public IPC-2152 1 oz table come from the
//! source trail in `crates/calc/data/calculators.toml`. The standards
//! themselves are paywalled; only the reproduced public tables are quoted.

use crate::consts::{ALPHA_CU, MIL_MM, OZ_UM, RHO_CU};
use crate::{Error, finite, num, opt_num};

/// Which copper the current rules apply to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopperLayer {
    /// Outer layer, more exposed to air.
    #[default]
    External,
    /// Inner layer; IPC-2221 halves the current.
    Internal,
}

/// Which current rule to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceMethod {
    /// IPC-2152 table when its published range covers the request,
    /// IPC-2221 otherwise.
    #[default]
    Auto,
    /// The published IPC-2152 1 oz external table.
    Ipc2152,
    /// The IPC-2221 closed-form curve.
    Ipc2221,
}

/// Published IPC-2152 1 oz, 10 K, external table: (ampere, mil).
const T2152: [(f64, f64); 10] = [
    (1.0, 10.0),
    (2.0, 30.0),
    (3.0, 50.0),
    (4.0, 80.0),
    (5.0, 110.0),
    (6.0, 150.0),
    (7.0, 180.0),
    (8.0, 220.0),
    (9.0, 260.0),
    (10.0, 300.0),
];

/// Log-log interpolation in the published IPC-2152 table.
fn ipc2152_width_mil(current: f64) -> Result<f64, Error> {
    for ((i0, w0), (i1, w1)) in T2152.iter().zip(T2152.iter().skip(1)) {
        if current >= *i0 && current <= *i1 {
            let f = (current.ln() - i0.ln()) / (i1.ln() - i0.ln());
            return Ok(w0 * (w1 / w0).powf(f));
        }
    }
    Err(Error::Invalid(format!(
        "the published IPC-2152 table covers 1 to 10 A, got {current}"
    )))
}

/// Input for [`trace_width`].
#[derive(Debug, Clone, Copy)]
pub struct TraceWidth {
    /// Current in amperes, 0.001 to 100.
    pub current: f64,
    /// Allowed temperature rise in kelvin, 1 to 100; default 10.
    pub temperature_rise: f64,
    /// Copper weight in oz per square foot, 0.25 to 6; default 1.
    pub copper_weight: f64,
    /// External or internal copper; default external.
    pub layer: CopperLayer,
    /// Which rule to apply; default auto.
    pub method: TraceMethod,
}

impl Default for TraceWidth {
    fn default() -> Self {
        TraceWidth {
            current: 1.0,
            temperature_rise: 10.0,
            copper_weight: 1.0,
            layer: CopperLayer::default(),
            method: TraceMethod::default(),
        }
    }
}

/// Width answer from [`trace_width`], naming the method that produced it.
#[derive(Debug, Clone, Copy)]
pub struct TraceWidthOut {
    /// Minimum width in millimetres.
    pub width_mm: f64,
    /// Minimum width in mils.
    pub width_mil: f64,
    /// Cross-section in square mils.
    pub cross_section: f64,
    /// Which rule was used: ipc2152 or ipc2221.
    pub method_used: &'static str,
    /// True when the notes must warn about a table approximation.
    pub scaled_from_1oz: bool,
}

/// Minimum width for a current and a temperature rise.
///
/// # Rules
/// - IPC-2221: I = k dT^0.44 A^0.725, k 0.048 external, 0.024 internal.
/// - The IPC-2152 table applies only to external copper at a 10 K rise
///   and 1 to 10 A; other weights scale the width by 1/oz and set
///   `scaled_from_1oz`. auto picks the table inside its range.
/// - `method_used` states which rule answered; callers must show it.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (OZ_UM, MIL_MM), `crate::num`, `crate::finite`.
/// # Do not
/// - Quote paywalled IPC text; only the reproduced public table is used.
pub fn trace_width(a: TraceWidth) -> Result<TraceWidthOut, Error> {
    let i = num("current", a.current, 0.001, 100.0)?;
    let dt = num("temperature_rise", a.temperature_rise, 1.0, 100.0)?;
    let oz = num("copper_weight", a.copper_weight, 0.25, 6.0)?;
    let external = a.layer == CopperLayer::External;
    let table_ok = external && dt == 10.0 && (1.0..=10.0).contains(&i);
    let use_2152 = match a.method {
        TraceMethod::Ipc2152 => external && dt == 10.0,
        TraceMethod::Ipc2221 => false,
        TraceMethod::Auto => table_ok,
    };
    let t_mil = oz * OZ_UM / 25.4;
    if use_2152 {
        let w_mil = ipc2152_width_mil(i)? / oz;
        let a_mil2 = w_mil * t_mil;
        return Ok(TraceWidthOut {
            width_mm: w_mil * MIL_MM,
            width_mil: w_mil,
            cross_section: a_mil2,
            method_used: "ipc2152",
            scaled_from_1oz: oz != 1.0,
        });
    }
    let k = if external { 0.048 } else { 0.024 };
    let a_mil2 = finite("cross_section", (i / (k * dt.powf(0.44))).powf(1.0 / 0.725))?;
    let w_mil = finite("width_mil", a_mil2 / t_mil)?;
    Ok(TraceWidthOut {
        width_mm: w_mil * MIL_MM,
        width_mil: w_mil,
        cross_section: a_mil2,
        method_used: "ipc2221",
        scaled_from_1oz: false,
    })
}

/// Input for [`via_current_resistance`].
#[derive(Debug, Clone, Copy)]
pub struct ViaCurrent {
    /// Drill diameter in millimetres, 0.05 to 10.
    pub drill: f64,
    /// Barrel plating thickness in micrometres, 5 to 100; default 25.
    pub plating: f64,
    /// Barrel length in millimetres, 0.1 to 10; default 1.6.
    pub length: f64,
    /// Allowed temperature rise in kelvin, 1 to 100; default 10.
    pub temperature_rise: f64,
    /// Optional target current in amperes for the parallel count.
    pub target_current: Option<f64>,
}

impl Default for ViaCurrent {
    fn default() -> Self {
        ViaCurrent {
            drill: 0.3,
            plating: 25.0,
            length: 1.6,
            temperature_rise: 10.0,
            target_current: None,
        }
    }
}

/// Via answer from [`via_current_resistance`].
#[derive(Debug, Clone, Copy)]
pub struct ViaCurrentOut {
    /// Barrel copper cross-section in square millimetres.
    pub barrel_area: f64,
    /// Current for the temperature rise, amperes.
    pub current: f64,
    /// Barrel DC resistance in ohms.
    pub resistance: f64,
    /// Vias needed for the target current, when one was given.
    pub count: Option<f64>,
}

/// Current, resistance and parallel count for a plated via.
///
/// # Rules
/// - Barrel area is the plating annulus; current uses the IPC-2221
///   internal constant, the usual via convention.
/// - count = ceil(target_current / current), present only when asked.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (RHO_CU, MIL_MM), `crate::num`, `crate::finite`.
/// # Do not
/// - Add pad-to-barrel spreading resistance; the model is a cylinder.
pub fn via_current_resistance(a: ViaCurrent) -> Result<ViaCurrentOut, Error> {
    let drill = num("drill", a.drill, 0.05, 10.0)?;
    let plating = num("plating", a.plating, 5.0, 100.0)?;
    let length = num("length", a.length, 0.1, 10.0)?;
    let dt = num("temperature_rise", a.temperature_rise, 1.0, 100.0)?;
    let target = opt_num("target_current", a.target_current, 0.001, 100.0)?;
    let d_out = drill + 2.0 * plating / 1000.0;
    let area = finite(
        "barrel_area",
        std::f64::consts::FRAC_PI_4 * (d_out * d_out - drill * drill),
    )?;
    let current = finite(
        "current",
        0.024 * dt.powf(0.44) * (area / MIL_MM.powi(2)).powf(0.725),
    )?;
    let resistance = finite("resistance", RHO_CU * (length / 1000.0) / (area * 1.0e-6))?;
    Ok(ViaCurrentOut {
        barrel_area: area,
        current,
        resistance,
        count: target.map(|t| (t / current).ceil()),
    })
}

/// Input for [`trace_resistance_drop`].
#[derive(Debug, Clone, Copy)]
pub struct TraceResistance {
    /// Width in millimetres, 0.01 to 100.
    pub width: f64,
    /// Copper weight in oz per square foot, 0.25 to 6; default 1.
    pub copper_weight: f64,
    /// Length in millimetres, 0.1 to 10000.
    pub length: f64,
    /// Current in amperes for drop and heat, 0 to 100; default 0.
    pub current: f64,
    /// Temperature in degrees Celsius, -55 to 200; default 20.
    pub temperature: f64,
}

impl Default for TraceResistance {
    fn default() -> Self {
        TraceResistance {
            width: 0.25,
            copper_weight: 1.0,
            length: 100.0,
            current: 0.0,
            temperature: 20.0,
        }
    }
}

/// Resistance, drop and heat from [`trace_resistance_drop`].
#[derive(Debug, Clone, Copy)]
pub struct TraceResistanceOut {
    /// DC resistance at the temperature, ohms.
    pub resistance: f64,
    /// Voltage drop at the current, volts.
    pub voltage_drop: f64,
    /// Heat at the current, watts.
    pub power: f64,
}

/// DC resistance of a trace with the copper temperature coefficient.
///
/// # Rules
/// - t = oz times 34.79 um; R = RHO_CU L / (w t) scaled by
///   1 + ALPHA_CU (T - 20).
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (RHO_CU, ALPHA_CU, OZ_UM), `crate::num`, `crate::finite`.
/// # Do not
/// - Trust it above about 1 MHz; skin effect thins the effective copper.
pub fn trace_resistance_drop(a: TraceResistance) -> Result<TraceResistanceOut, Error> {
    let w = num("width", a.width, 0.01, 100.0)?;
    let oz = num("copper_weight", a.copper_weight, 0.25, 6.0)?;
    let l = num("length", a.length, 0.1, 10000.0)?;
    let i = num("current", a.current, 0.0, 100.0)?;
    let t = num("temperature", a.temperature, -55.0, 200.0)?;
    let thickness = oz * OZ_UM / 1000.0;
    let r20 = finite(
        "resistance",
        RHO_CU * (l / 1000.0) / ((w * thickness) * 1.0e-6),
    )?;
    let r = r20 * (1.0 + ALPHA_CU * (t - 20.0));
    Ok(TraceResistanceOut {
        resistance: r,
        voltage_drop: i * r,
        power: i * i * r,
    })
}
