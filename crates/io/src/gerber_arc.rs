//! Gerber arc export policy: true arcs or chord flattening with a report.
use pcbcraft_doc::{FabRules, Project};
use pcbcraft_geom::{Arc, Point};
use serde_json::Value;

/// Default chord tolerance when the caller does not set `max_deviation`.
pub const DEFAULT_MAX_DEVIATION_MM: f64 = 0.01;
/// Radii at the arc ends must agree within this tolerance (millimetres).
const ARC_RADIUS_TOLERANCE_MM: f64 = 1e-6;

/// Whether fabrication Gerber writers emit G02/G03 or chorded G01 segments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GerberArcOptions {
    pub arcs: bool,
    pub max_deviation: f64,
}

impl GerberArcOptions {
    pub fn default_for(project: &Project, settings: &Value) -> Self {
        let fab = project.board.rules.as_ref();
        let arcs = settings
            .get("arcs")
            .and_then(Value::as_bool)
            .or_else(|| fab.and_then(FabRules::arcs))
            .unwrap_or(true);
        let max_deviation = settings
            .get("max_deviation")
            .and_then(Value::as_f64)
            .filter(|d| d.is_finite() && *d > 0.0 && *d <= 1.0)
            .unwrap_or(DEFAULT_MAX_DEVIATION_MM);
        Self {
            arcs,
            max_deviation,
        }
    }
}

/// Counts for the short arc report written beside Gerber output.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GerberArcReport {
    pub arcs_written: u64,
    pub arcs_flattened: u64,
    pub worst_deviation_mm: f64,
}

impl GerberArcReport {
    pub fn record_written(&mut self) {
        self.arcs_written = self.arcs_written.saturating_add(1);
    }

    pub fn record_flattened(&mut self, worst: f64) {
        self.arcs_flattened = self.arcs_flattened.saturating_add(1);
        if worst.is_finite() {
            self.worst_deviation_mm = self.worst_deviation_mm.max(worst);
        }
    }

    pub fn to_text(&self) -> String {
        format!(
            "Gerber arc export\narcs written: {}\narcs flattened: {}\nworst deviation mm: {:.6}\n",
            self.arcs_written, self.arcs_flattened, self.worst_deviation_mm
        )
    }
}

/// Check a Gerber arc command against RS-274X expectations before it is written.
pub fn validate_gerber_arc(
    start: Point,
    end: Point,
    center: Point,
    clockwise: bool,
) -> Result<(), String> {
    let r_start = start.distance(center);
    let r_end = end.distance(center);
    if (r_start - r_end).abs() > ARC_RADIUS_TOLERANCE_MM {
        return Err(format!(
            "gerber arc end radii disagree by {:.6} mm",
            (r_start - r_end).abs()
        ));
    }
    if r_start < 1e-9 {
        return Err("gerber arc radius is zero".into());
    }
    let chord = start.distance(end);
    if chord < 1e-9 {
        return Err("gerber arc chord is zero".into());
    }
    let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
    let dist = center.distance(mid);
    if dist < 1e-9 {
        return Err("gerber arc centre lies on the chord".into());
    }
    let _ = clockwise;
    Ok(())
}

pub fn flatten_arc_worst(arc: &Arc, max_deviation: f64) -> f64 {
    pcbcraft_doc::arc_chord_worst_error(arc, max_deviation)
}
