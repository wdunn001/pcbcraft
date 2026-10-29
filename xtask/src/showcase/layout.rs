//! Board placement of the showcase, by functional block, and the label audit.
//!
//! Every part is listed once with its centre and rotation. The audit measures
//! where the canvas draws the reference and value labels and reports any that
//! land on another label, another part or the board edge, so a placement edit
//! can be checked without opening the app.
//!
//! # Rules
//! - Coordinates are millimetres from the top-left corner of the board.
//! - Rotation is in degrees, in steps of 90.
//! - Parts of one block share a rotation unless a pin order demands otherwise.
//!
//! # Dependencies
//! - xtask/src/showcase/design.rs
//! - crates/ui-egui/src/canvas.rs (label offsets, mirrored by the constants below)
//!
//! # Do not
//! - Move a part after routing has started.
use pcbcraft_geom::Point;

/// Board size in millimetres.
pub const WIDTH: f64 = 72.0;
pub const HEIGHT: f64 = 60.0;

/// Reference, x, y, rotation.
pub const PLACEMENT: &[(&str, f64, f64, f64)] = &[
    // USB power and protection, top left.
    ("J1", 8.4, 11.5, 90.0),
    ("R2", 11.9, 10.0, 0.0),
    ("D2", 17.5, 6.0, 270.0),
    ("F1", 22.5, 9.0, 0.0),
    ("C1", 26.7, 7.0, 90.0),
    ("R1", 15.0, 13.0, 0.0),
    // Charger.
    ("U1", 25.25, 15.5, 0.0),
    ("R3", 28.2, 18.6, 0.0),
    ("C2", 22.0, 17.6, 270.0),
    ("D1", 29.5, 12.0, 180.0),
    ("R4", 32.5, 12.0, 180.0),
    // Battery and regulator.
    ("J2", 6.0, 52.0, 0.0),
    ("U2", 17.0, 50.0, 0.0),
    ("C3", 11.5, 50.0, 90.0),
    ("C4", 22.5, 50.0, 90.0),
    ("TP1", 4.0, 44.0, 0.0),
    ("TP2", 14.5, 43.5, 0.0),
    // Microcontroller and support.
    ("U3", 40.0, 28.0, 0.0),
    ("Y1", 32.3, 24.0, 0.0),
    ("C5", 27.4, 26.0, 180.0),
    ("C6", 35.0, 18.2, 90.0),
    ("C7", 35.0, 32.0, 90.0),
    ("C8", 45.0, 32.5, 90.0),
    ("C9", 37.0, 21.0, 90.0),
    ("C10", 44.0, 21.0, 90.0),
    ("C11", 47.0, 21.0, 90.0),
    ("C12", 41.0, 14.0, 90.0),
    ("R5", 49.9, 12.3, 90.0),
    ("SW1", 55.0, 15.0, 0.0),
    ("J3", 40.5, 6.0, 90.0),
    ("U4", 53.0, 28.0, 0.0),
    ("C13", 53.6, 37.0, 0.0),
    // Sensors.
    ("U5", 66.0, 13.2, 0.0),
    ("C14", 66.0, 20.4, 0.0),
    ("C15", 70.0, 13.2, 90.0),
    ("U6", 66.0, 27.6, 0.0),
    ("C16", 66.0, 34.8, 0.0),
    ("R8", 61.5, 6.0, 180.0),
    ("R9", 64.3, 6.0, 180.0),
    ("U7", 66.0, 42.0, 0.0),
    ("C17", 70.0, 42.0, 90.0),
    // Indicators and buttons.
    ("R10", 27.0, 43.0, 270.0),
    ("R11", 33.0, 43.0, 270.0),
    ("R12", 39.25, 43.0, 270.0),
    ("R13", 45.0, 43.0, 270.0),
    ("R14", 51.0, 43.0, 270.0),
    ("R15", 57.0, 43.0, 270.0),
    ("D3", 27.0, 51.0, 270.0),
    ("D4", 33.0, 51.0, 270.0),
    ("D5", 39.25, 51.0, 270.0),
    ("D6", 45.0, 51.0, 270.0),
    ("D7", 51.0, 51.0, 270.0),
    ("D8", 57.0, 51.0, 270.0),
    ("SW2", 54.0, 5.5, 0.0),
    ("SW3", 21.0, 26.2, 180.0),
];

type Rect = (f64, f64, f64, f64);

fn overlap(a: Rect, b: Rect) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// Pixels per millimetre of the canvas when the whole board is fitted.
fn scale() -> f64 {
    729.0 / WIDTH
}

/// Label and body rectangles of every footprint, as the canvas draws them.
pub fn audit(project: &pcbcraft_doc::Project) -> Vec<String> {
    let s = scale();
    let p = project;
    let mut bodies: Vec<(String, Rect)> = Vec::new();
    let mut labels: Vec<(String, Rect)> = Vec::new();
    for f in &p.board.footprints {
        let Some(c) = p.component(f.component) else {
            continue;
        };
        let pts: Vec<Point> = f
            .drawing_points(p)
            .into_iter()
            .map(|q| q.rotate(-f.rotation))
            .collect();
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for q in &pts {
            x0 = x0.min(q.x);
            y0 = y0.min(q.y);
            x1 = x1.max(q.x);
            y1 = y1.max(q.y);
        }
        let body = (
            f.position.x + x0,
            f.position.y + y0,
            f.position.x + x1,
            f.position.y + y1,
        );
        bodies.push((c.reference.clone(), body));
        let cx = f.position.x + (x0 + x1) / 2.0;
        let rw = c.reference.chars().count() as f64 * 7.6 / s;
        let vw = c.value.chars().count() as f64 * 5.9 / s;
        labels.push((
            format!("{} label", c.reference),
            (
                cx - rw / 2.0,
                body.1 - 26.0 / s,
                cx + rw / 2.0,
                body.1 - 12.0 / s,
            ),
        ));
        labels.push((
            format!("{} value", c.reference),
            (
                cx - vw / 2.0,
                body.3 + 12.0 / s,
                cx + vw / 2.0,
                body.3 + 24.0 / s,
            ),
        ));
    }
    let mut out = Vec::new();
    for (i, (na, a)) in labels.iter().enumerate() {
        if a.0 < 0.0 || a.1 < 0.0 || a.2 > WIDTH || a.3 > HEIGHT {
            out.push(format!("{na} leaves the board"));
        }
        for (nb, b) in labels.iter().skip(i + 1) {
            if overlap(*a, *b) {
                out.push(format!("{na} overlaps {nb}"));
            }
        }
        let owner = na.split(' ').next().unwrap_or("");
        for (nb, b) in &bodies {
            if nb != owner && overlap(*a, *b) {
                out.push(format!("{na} covers part {nb}"));
            }
        }
    }
    for (i, (na, a)) in bodies.iter().enumerate() {
        for (nb, b) in bodies.iter().skip(i + 1) {
            if overlap(*a, *b) {
                out.push(format!("part {na} overlaps part {nb}"));
            }
        }
    }
    out
}
