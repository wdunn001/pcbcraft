//! Physical and unit constants shared by the calculators.
//!
//! Values and their sources are listed in the header of
//! `crates/calc/data/calculators.toml`. Nothing here is a design rule;
//! these are material and unit facts the formulas assume, stated so a
//! reader can trace a number back to a handbook.
//!
//! Local rule: only add a constant here when at least two calculators
//! use it; single-use constants stay in the calculator's own module.

/// Resistivity of annealed copper at 20 C, ohm metres (CRC Handbook).
pub const RHO_CU: f64 = 1.72e-8;

/// Temperature coefficient of copper resistance, per kelvin (CRC Handbook).
pub const ALPHA_CU: f64 = 0.00393;

/// Thermal conductivity of copper, W per metre kelvin (CRC Handbook).
pub const K_CU: f64 = 385.0;

/// Copper thickness per ounce per square foot, micrometres. Fab shops
/// round this to 35 um; keep the exact 34.79 so tables interpolate.
pub const OZ_UM: f64 = 34.79;

/// One thousandth of an inch, in millimetres (exact by definition).
pub const MIL_MM: f64 = 0.0254;

/// Speed of light in vacuum, metres per second (SI definition).
pub const C0: f64 = 299_792_458.0;

/// Impedance of free space, ohms.
pub const ZF0: f64 = 376.730_313_668_57;
