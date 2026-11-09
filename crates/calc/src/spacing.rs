//! Clearance and creepage calculators: `conductor_spacing_ipc2221`
//! (board-level copper to copper spacing) and
//! `creepage_clearance_iec60664` (safety insulation coordination).
//!
//! IPC-2221 is a design standard, not a safety standard; anything a
//! person can touch goes through the IEC 60664-1 lookup instead. Both
//! tables here are reproduced from secondary sources and the metadata
//! carries the verify labels the UI must show.

use crate::{Error, num};

/// Where the conductors sit, IPC-2221B table 6-1 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpacingLocation {
    /// Internal conductors, hermetically sealed.
    B1,
    /// External uncoated, up to 3050 m altitude.
    #[default]
    B2,
    /// External uncoated, above 3050 m altitude.
    B3,
    /// External with polymer coating.
    B4,
    /// Assembly conformal coated.
    A5,
    /// Component lead, uncoated.
    A6,
    /// Component lead, coated.
    A7,
}

/// Rows of IPC-2221B table 6-1: (voltage ceiling V peak, seven columns).
const SPACING_ROWS: [(f64, [f64; 7]); 9] = [
    (15.0, [0.05, 0.1, 0.1, 0.05, 0.13, 0.13, 0.13]),
    (30.0, [0.05, 0.1, 0.1, 0.05, 0.13, 0.25, 0.13]),
    (50.0, [0.1, 0.6, 0.6, 0.13, 0.13, 0.4, 0.13]),
    (100.0, [0.1, 0.6, 1.5, 0.13, 0.13, 0.5, 0.13]),
    (150.0, [0.2, 0.6, 3.2, 0.4, 0.4, 0.8, 0.4]),
    (170.0, [0.2, 1.25, 3.2, 0.4, 0.4, 0.8, 0.4]),
    (250.0, [0.2, 1.25, 6.4, 0.4, 0.4, 0.8, 0.4]),
    (300.0, [0.2, 2.5, 12.5, 0.4, 0.4, 0.8, 0.8]),
    (500.0, [0.25, 2.5, 12.5, 0.8, 0.8, 1.5, 0.8]),
];

/// Above 500 V, millimetres per volt, one entry per column.
const SPACING_SLOPES: [f64; 7] = [0.0025, 0.005, 0.025, 0.00305, 0.00305, 0.00305, 0.00305];

/// Column index of a location.
fn column(loc: SpacingLocation) -> usize {
    match loc {
        SpacingLocation::B1 => 0,
        SpacingLocation::B2 => 1,
        SpacingLocation::B3 => 2,
        SpacingLocation::B4 => 3,
        SpacingLocation::A5 => 4,
        SpacingLocation::A6 => 5,
        SpacingLocation::A7 => 6,
    }
}

/// Input for [`conductor_spacing_ipc2221`].
#[derive(Debug, Clone, Copy)]
pub struct ConductorSpacing {
    /// Peak working voltage, 0 to 100000 V.
    pub voltage: f64,
    /// Board location class; default B2.
    pub location: SpacingLocation,
}

impl Default for ConductorSpacing {
    fn default() -> Self {
        ConductorSpacing {
            voltage: 50.0,
            location: SpacingLocation::B2,
        }
    }
}

/// Answer from [`conductor_spacing_ipc2221`].
#[derive(Debug, Clone, Copy)]
pub struct ConductorSpacingOut {
    /// Minimum copper to copper spacing in millimetres.
    pub spacing: f64,
    /// Table row that applied, for display.
    pub row_label: &'static str,
    /// Plain-language description of the location class.
    pub location_description: &'static str,
}

/// Minimum IPC-2221B spacing for a peak voltage and location class.
///
/// # Rules
/// - Pick the first table row whose ceiling is at least the voltage;
///   above 500 V use spacing = voltage times the column's slope.
/// - Table values are the standard's own, reproduced by three sources.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `SPACING_ROWS`, `SPACING_SLOPES`, `column`, `crate::num`.
/// # Do not
/// - Treat this as a safety answer; mains and touchable circuits use
///   [`creepage_clearance_iec60664`].
pub fn conductor_spacing_ipc2221(a: ConductorSpacing) -> Result<ConductorSpacingOut, Error> {
    let v = num("voltage", a.voltage, 0.0, 100_000.0)?;
    let col = column(a.location);
    let mut spacing = v * SPACING_SLOPES[col];
    let mut row_label = ">500";
    for (ceiling, row) in SPACING_ROWS {
        if v <= ceiling {
            spacing = row[col];
            row_label = if ceiling <= 15.0 {
                "<=15"
            } else {
                match ceiling as i64 {
                    30 => "<=30",
                    50 => "<=50",
                    100 => "<=100",
                    150 => "<=150",
                    170 => "<=170",
                    250 => "<=250",
                    300 => "<=300",
                    _ => "<=500",
                }
            };
            break;
        }
    }
    Ok(ConductorSpacingOut {
        spacing,
        row_label,
        location_description: location_description(a.location),
    })
}

/// Plain-language description of a location class.
fn location_description(loc: SpacingLocation) -> &'static str {
    match loc {
        SpacingLocation::B1 => "internal conductors, hermetically sealed",
        SpacingLocation::B2 => "external uncoated, up to 3050 m",
        SpacingLocation::B3 => "external uncoated, above 3050 m",
        SpacingLocation::B4 => "external, polymer coated",
        SpacingLocation::A5 => "assembly, conformal coated",
        SpacingLocation::A6 => "component lead, uncoated",
        SpacingLocation::A7 => "component lead, coated",
    }
}

/// Overvoltage (installation) category, IEC 60664-1 table F.1 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OvervoltageCategory {
    /// Equipment not connected to the mains.
    I,
    /// Ordinary appliances.
    #[default]
    II,
    /// Fixed distribution equipment.
    III,
    /// Equipment at the service entrance.
    IV,
}

/// Pollution degree of the micro-environment, table F.4 / F.5 selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PollutionDegree {
    /// No conductive dust (sealed).
    D1,
    /// Non-conductive dust, occasional condensation.
    #[default]
    D2,
    /// Conductive dust or frequent condensation.
    D3,
}

/// Insulating material group by CTI, table F.5 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaterialGroup {
    /// CTI 600 and above.
    G1,
    /// CTI 400 to 599.
    G2,
    /// CTI 175 to 399.
    #[default]
    G3A,
    /// CTI 100 to 174.
    G3B,
}

/// Kind of insulation to size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Insulation {
    /// Circuits with no protection against shock.
    Functional,
    /// Basic protection.
    #[default]
    Basic,
    /// Double or reinforced protection.
    Reinforced,
}

/// Table F.1: (mains voltage ceiling V RMS, impulse kV for OVC I..IV).
const IMPULSE_ROWS: [(f64, [f64; 4]); 6] = [
    (50.0, [0.33, 0.5, 0.8, 1.5]),
    (100.0, [0.5, 0.8, 1.5, 2.5]),
    (150.0, [0.8, 1.5, 2.5, 4.0]),
    (300.0, [1.5, 2.5, 4.0, 6.0]),
    (600.0, [2.5, 4.0, 6.0, 8.0]),
    (1000.0, [4.0, 6.0, 8.0, 12.0]),
];

/// The F.1 impulse ladder, used to step reinforced insulation up.
const IMPULSE_LADDER: [f64; 9] = [0.33, 0.5, 0.8, 1.5, 2.5, 4.0, 6.0, 8.0, 12.0];

/// Table F.2: clearance mm (inhomogeneous field, to 2000 m) per impulse kV.
const CLEARANCE_TABLE: [(f64, f64); 9] = [
    (0.33, 0.01),
    (0.5, 0.04),
    (0.8, 0.1),
    (1.5, 0.5),
    (2.5, 1.5),
    (4.0, 3.0),
    (6.0, 5.5),
    (8.0, 8.0),
    (12.0, 14.0),
];

/// Table F.8 altitude factors applied above 2000 m.
fn altitude_factor(altitude_m: f64) -> f64 {
    if altitude_m > 4000.0 {
        1.48
    } else if altitude_m > 3000.0 {
        1.29
    } else if altitude_m > 2000.0 {
        1.14
    } else {
        1.0
    }
}

/// Table F.5: (working voltage ceiling, d1, d2 I II III, d3 I II III) mm.
const CREEPAGE_ROWS: [(f64, [f64; 7]); 14] = [
    (50.0, [0.18, 0.6, 0.85, 1.2, 1.5, 1.7, 1.9]),
    (63.0, [0.2, 0.63, 0.9, 1.25, 1.6, 1.8, 2.0]),
    (80.0, [0.22, 0.67, 0.95, 1.3, 1.7, 1.9, 2.1]),
    (100.0, [0.25, 0.71, 1.0, 1.4, 1.8, 2.0, 2.2]),
    (125.0, [0.28, 0.75, 1.05, 1.5, 1.9, 2.1, 2.4]),
    (160.0, [0.32, 0.8, 1.1, 1.6, 2.0, 2.2, 2.5]),
    (200.0, [0.42, 1.0, 1.4, 2.0, 2.5, 2.8, 3.2]),
    (250.0, [0.56, 1.25, 1.8, 2.5, 3.2, 3.6, 4.0]),
    (320.0, [0.75, 1.6, 2.2, 3.2, 4.0, 4.5, 5.0]),
    (400.0, [1.0, 2.0, 2.8, 4.0, 5.0, 5.6, 6.3]),
    (500.0, [1.3, 2.5, 3.6, 5.0, 6.3, 7.1, 8.0]),
    (630.0, [1.8, 3.2, 4.5, 6.3, 8.0, 9.0, 10.0]),
    (800.0, [2.4, 4.0, 5.6, 8.0, 10.0, 11.0, 12.5]),
    (1000.0, [3.2, 5.0, 7.1, 10.0, 12.5, 14.0, 16.0]),
];

/// Creepage column index for a pollution degree and material group.
fn creepage_column(pollution: PollutionDegree, group: MaterialGroup) -> usize {
    match pollution {
        PollutionDegree::D1 => 0,
        PollutionDegree::D2 => match group {
            MaterialGroup::G1 => 1,
            MaterialGroup::G2 => 2,
            _ => 3,
        },
        PollutionDegree::D3 => match group {
            MaterialGroup::G1 => 4,
            MaterialGroup::G2 => 5,
            _ => 6,
        },
    }
}

/// Input for [`creepage_clearance_iec60664`].
#[derive(Debug, Clone, Copy)]
pub struct CreepageClearance {
    /// Working voltage RMS, 10 to 1000 V.
    pub working_voltage: f64,
    /// Nominal mains voltage line to neutral RMS, 50 to 1000 V.
    pub mains_voltage: f64,
    /// Overvoltage category; default II.
    pub overvoltage_category: OvervoltageCategory,
    /// Pollution degree; default 2.
    pub pollution_degree: PollutionDegree,
    /// Material group from CTI; default IIIa.
    pub material_group: MaterialGroup,
    /// Insulation kind; default basic.
    pub insulation: Insulation,
    /// Altitude in metres, 0 to 5000; default 2000.
    pub altitude: f64,
}

impl Default for CreepageClearance {
    fn default() -> Self {
        CreepageClearance {
            working_voltage: 230.0,
            mains_voltage: 230.0,
            overvoltage_category: OvervoltageCategory::II,
            pollution_degree: PollutionDegree::D2,
            material_group: MaterialGroup::G3A,
            insulation: Insulation::Basic,
            altitude: 2000.0,
        }
    }
}

/// Answer from [`creepage_clearance_iec60664`].
#[derive(Debug, Clone, Copy)]
pub struct CreepageClearanceOut {
    /// Rated impulse voltage in kilovolts.
    pub impulse_voltage: f64,
    /// Minimum clearance through air in millimetres.
    pub clearance: f64,
    /// Minimum creepage along the surface in millimetres.
    pub creepage: f64,
}

/// IEC 60664-1 clearance and creepage lookup for low-voltage designs.
///
/// # Rules
/// - Impulse voltage comes from the mains ceiling and category; the
///   reinforced case takes the next step up the F.1 ladder.
/// - Clearance = F.2 value, raised to the pollution floor (0.2 mm
///   degree 2, 0.8 mm degree 3) and scaled by the F.8 altitude factor.
/// - Final creepage is max(table creepage, reinforced doubling,
///   clearance); creepage is never below clearance.
/// # Dependencies
/// - `crates/calc/src/lib.rs` for `IMPULSE_ROWS`, `CLEARANCE_TABLE`, `CREEPAGE_ROWS`,
///   `altitude_factor`, `creepage_column`, `crate::num`.
/// # Do not
/// - Use for anything a product safety standard (IEC 62368-1 class of
///   protection) governs without checking that standard's tables.
pub fn creepage_clearance_iec60664(a: CreepageClearance) -> Result<CreepageClearanceOut, Error> {
    let vw = num("working_voltage", a.working_voltage, 10.0, 1000.0)?;
    let vm = num("mains_voltage", a.mains_voltage, 50.0, 1000.0)?;
    let alt = num("altitude", a.altitude, 0.0, 5000.0)?;
    let cat = match a.overvoltage_category {
        OvervoltageCategory::I => 0,
        OvervoltageCategory::II => 1,
        OvervoltageCategory::III => 2,
        OvervoltageCategory::IV => 3,
    };
    let mut impulse = IMPULSE_ROWS[IMPULSE_ROWS.len() - 1].1[cat];
    for (ceiling, row) in IMPULSE_ROWS {
        if vm <= ceiling {
            impulse = row[cat];
            break;
        }
    }
    if a.insulation == Insulation::Reinforced {
        for step in IMPULSE_LADDER {
            if step > impulse + 1e-12 {
                impulse = step;
                break;
            }
        }
    }
    let mut clearance = CLEARANCE_TABLE[CLEARANCE_TABLE.len() - 1].1;
    for (kv, mm) in CLEARANCE_TABLE {
        if (kv - impulse).abs() < 1e-12 {
            clearance = mm;
            break;
        }
    }
    clearance = match a.pollution_degree {
        PollutionDegree::D1 => clearance,
        PollutionDegree::D2 => clearance.max(0.2),
        PollutionDegree::D3 => clearance.max(0.8),
    };
    clearance *= altitude_factor(alt);
    let col = creepage_column(a.pollution_degree, a.material_group);
    let mut creepage = CREEPAGE_ROWS[CREEPAGE_ROWS.len() - 1].1[col];
    for (ceiling, row) in CREEPAGE_ROWS {
        if vw <= ceiling {
            creepage = row[col];
            break;
        }
    }
    if a.insulation == Insulation::Reinforced {
        creepage *= 2.0;
    }
    Ok(CreepageClearanceOut {
        impulse_voltage: impulse,
        clearance,
        creepage: creepage.max(clearance),
    })
}
