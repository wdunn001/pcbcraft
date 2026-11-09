//! Utility calculators: the unit converter (length plus copper weight
//! as a thickness) and the thermal via count estimate.
//!
//! Conversions here are exact by definition; the thermal model is an
//! order-of-magnitude conduction estimate and its metadata says so.

use crate::consts::{K_CU, MIL_MM, OZ_UM};
use crate::{Error, finite, num};

/// Length or copper-weight unit for [`unit_converter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LengthUnit {
    /// Millimetre.
    #[default]
    Mm,
    /// Thousandth of an inch.
    Mil,
    /// Inch.
    Inch,
    /// Micrometre.
    Um,
    /// Copper weight, ounces per square foot, as a thickness.
    Oz,
}

impl LengthUnit {
    /// Parse a unit name; None for anything else.
    pub fn parse(name: &str) -> Option<LengthUnit> {
        match name {
            "mm" => Some(LengthUnit::Mm),
            "mil" => Some(LengthUnit::Mil),
            "inch" => Some(LengthUnit::Inch),
            "um" => Some(LengthUnit::Um),
            "oz" => Some(LengthUnit::Oz),
            _ => None,
        }
    }

    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            LengthUnit::Mm => "mm",
            LengthUnit::Mil => "mil",
            LengthUnit::Inch => "inch",
            LengthUnit::Um => "um",
            LengthUnit::Oz => "oz",
        }
    }

    /// Millimetres per unit; 1 oz of copper is 34.79 um thick.
    fn to_mm(self) -> f64 {
        match self {
            LengthUnit::Mm => 1.0,
            LengthUnit::Mil => MIL_MM,
            LengthUnit::Inch => 25.4,
            LengthUnit::Um => 0.001,
            LengthUnit::Oz => OZ_UM * 0.001,
        }
    }
}

/// Input for [`unit_converter`].
#[derive(Debug, Clone, Copy)]
pub struct UnitConvert {
    /// Value to convert, -1e9 to 1e9.
    pub value: f64,
    /// Unit it is given in.
    pub from: LengthUnit,
    /// Unit to produce.
    pub to: LengthUnit,
}

impl Default for UnitConvert {
    fn default() -> Self {
        UnitConvert {
            value: 1.0,
            from: LengthUnit::Mil,
            to: LengthUnit::Mm,
        }
    }
}

/// Answer from [`unit_converter`].
#[derive(Debug, Clone, Copy)]
pub struct UnitConvertOut {
    /// The converted value in the requested unit.
    pub value: f64,
}

/// Exact conversion between mil, mm, inch, um and oz/ft2 copper.
///
/// # Rules
/// - Go through millimetres; oz is accepted as a copper thickness
///   (1 oz = 34.79 um) and converts only to and from length units.
/// - Same-unit input returns the value unchanged.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (MIL_MM, OZ_UM), `LengthUnit::to_mm`.
/// # Do not
/// - Convert oz to a mass or a sheet area; this table only knows the
///   equivalent copper thickness.
pub fn unit_converter(a: UnitConvert) -> Result<UnitConvertOut, Error> {
    let v = num("value", a.value, -1.0e9, 1.0e9)?;
    if a.from == a.to {
        return Ok(UnitConvertOut { value: v });
    }
    Ok(UnitConvertOut {
        value: finite("value", v * a.from.to_mm() / a.to.to_mm())?,
    })
}

/// Input for [`thermal_via_count`].
#[derive(Debug, Clone, Copy)]
pub struct ThermalVias {
    /// Via drill diameter, 0.1 to 1.0 mm.
    pub drill: f64,
    /// Barrel plating thickness, 10 to 50 um; default 25.
    pub plating: f64,
    /// Via length (board thickness), 0.1 to 5.0 mm; default 1.6.
    pub length: f64,
    /// Target thermal resistance, 0.1 to 100 K/W.
    pub target_rth: f64,
    /// Filled barrel: count the full drill area, not the annulus.
    pub filled: bool,
}

impl Default for ThermalVias {
    fn default() -> Self {
        ThermalVias {
            drill: 0.3,
            plating: 25.0,
            length: 1.6,
            target_rth: 5.0,
            filled: false,
        }
    }
}

/// Answer from [`thermal_via_count`].
#[derive(Debug, Clone, Copy)]
pub struct ThermalViasOut {
    /// Conduction resistance of one via in K/W.
    pub rth_one_via: f64,
    /// Vias needed in parallel to reach the target.
    pub count: i64,
    /// Side of the square array that holds the count.
    pub array_hint: i64,
}

/// How many plated vias under a pad reach a target thermal resistance.
///
/// # Rules
/// - Copper area A is the barrel annulus, or the full outer circle
///   when filled; Rth = length / (K_CU A), count = ceil(Rth / target),
///   array hint = ceil(sqrt(count)).
/// - Order of magnitude; spreading resistance is ignored.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `crate::consts` (K_CU), `crate::num`, `crate::finite`.
/// # Do not
/// - Stack more than about 16 vias expecting linear gains; the pad
///   spreading resistance flattens the curve.
pub fn thermal_via_count(a: ThermalVias) -> Result<ThermalViasOut, Error> {
    let d = num("drill", a.drill, 0.1, 1.0)?;
    let p = num("plating", a.plating, 10.0, 50.0)?;
    let l = num("length", a.length, 0.1, 5.0)?;
    let target = num("target_rth", a.target_rth, 0.1, 100.0)?;
    let outer = d + 2.0 * p * 0.001;
    let area_mm2 = if a.filled {
        std::f64::consts::FRAC_PI_4 * outer * outer
    } else {
        std::f64::consts::FRAC_PI_4 * (outer * outer - d * d)
    };
    let rth = finite("rth_one_via", (l / 1000.0) / (K_CU * area_mm2 * 1.0e-6))?;
    let count = (rth / target).ceil() as i64;
    Ok(ThermalViasOut {
        rth_one_via: rth,
        count,
        array_hint: (count.max(1) as f64).sqrt().ceil() as i64,
    })
}
