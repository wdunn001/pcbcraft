//! Shared physical constants for calculator formulas.

/// Copper resistivity at 20 C in ohm metres.
pub const RHO_CU: f64 = 1.72e-8;
/// Copper temperature coefficient per degree C.
pub const ALPHA_CU: f64 = 0.00393;
/// Copper thermal conductivity in W/(m·K).
pub const K_CU: f64 = 385.0;
/// One ounce copper thickness in micrometres.
pub const OZ_UM: f64 = 34.79;
/// One mil in millimetres.
pub const MIL_MM: f64 = 0.0254;
/// Speed of light in metres per second.
pub const C0: f64 = 299_792_458.0;
/// Impedance of free space in ohms.
pub const ZF0: f64 = 376.730_313_668_57;
