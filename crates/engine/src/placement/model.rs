//! The plain data every placement step works on.
//!
//! Context: the solver, the legaliser and the annealer all read a `Model`, a
//! flat copy of the parts and nets that needs no `Project`. That keeps the math
//! testable from text fixtures and keeps the project code in one place,
//! `build_model`, which slice 1121-a fills.
//!
//! Local rules:
//! - Units are millimetres. A cell box is centred on the cell position.
//! - A pin offset is stored for turn 0. The offset in use is the stored one
//!   turned `turns` quarter turns: one turn maps `(x, y)` to `(-y, x)`.
//! - A part is a cell; indexes into `Model::cells` are the identity used by
//!   every step, so cell order is part of the contract (reference, then id).
//!
//! Dependencies: `crates/engine/src/placement/mod.rs` (settings and results).
use super::{PlacementSettings, not_built};
use crate::scope::Scope;
use pcbcraft_doc::Project;

/// An axis aligned rectangle in millimetres.
///
/// # Rules
/// - `x0 <= x1` and `y0 <= y1`.
///
/// # Dependencies
/// - crates/engine/src/placement/legalize.rs (board and keepouts)
///
/// # Do not
/// - Store a rotated rectangle; placement boxes are always axis aligned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x0: f64,
    /// Bottom edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Top edge.
    pub y1: f64,
}

/// One part as the placement steps see it: a centre, a half size box, a turn
/// count and a fixed flag.
///
/// # Rules
/// - `half_x` and `half_y` are for turn 0; use `Cell::half` for the box now.
/// - A fixed cell is never moved by any step. Locked parts, parts out of scope
///   and parts on the board edge are fixed.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Footprint` is what a cell is built from)
///
/// # Do not
/// - Move a cell with `fixed` set.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// The footprint id this cell stands for.
    pub id: u64,
    /// Centre x.
    pub x: f64,
    /// Centre y.
    pub y: f64,
    /// Half width of the box at turn 0.
    pub half_x: f64,
    /// Half height of the box at turn 0.
    pub half_y: f64,
    /// Quarter turns applied since the model was built, 0 to 3.
    pub turns: u8,
    /// True when no step may move this cell.
    pub fixed: bool,
}

/// One pin of a net: which cell it sits on and where, for turn 0.
///
/// # Rules
/// - `dx` and `dy` are measured from the cell centre.
///
/// # Dependencies
/// - crates/engine/src/placement/model.rs (`NetSpring` owns a list of these)
///
/// # Do not
/// - Store a world position; the cell position is added when it is needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pin {
    /// Index into `Model::cells`.
    pub cell: usize,
    /// Pin offset from the cell centre, x, for turn 0.
    pub dx: f64,
    /// Pin offset from the cell centre, y, for turn 0.
    pub dy: f64,
}

/// A net seen as a spring bundle: its pins and one weight.
///
/// # Rules
/// - `weight` is `power_net_weight` for power and ground nets and
///   `signal_net_weight` for the rest; the builder decides which.
/// - Nets with fewer than two cells are dropped by the builder.
///
/// # Dependencies
/// - crates/engine/src/placement/quadratic.rs (turns it into springs)
///
/// # Do not
/// - Use `weight` in the wirelength that is reported; reports use plain HPWL.
#[derive(Clone, Debug, PartialEq)]
pub struct NetSpring {
    /// The net id in the project (0 in fixtures that have none).
    pub net: u64,
    /// Spring strength of the whole net before the 1/(k-1) clique split.
    pub weight: f64,
    /// The pins, in the order the clique pairs are taken.
    pub pins: Vec<Pin>,
}

/// The whole board as a placement problem.
///
/// # Rules
/// - `board` is the outline rectangle; the edge clearance is applied by the
///   steps that need it, not stored here.
/// - `keepouts` are rectangles no cell box may overlap; they never move.
///
/// # Dependencies
/// - crates/engine/src/placement/mod.rs (`place_global`, `PlacementSettings`)
///
/// # Do not
/// - Reorder `cells` or `nets` after building; indexes are identities.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// Parts in the fixed order (reference, then footprint id).
    pub cells: Vec<Cell>,
    /// Nets in net id order.
    pub nets: Vec<NetSpring>,
    /// The board rectangle.
    pub board: Rect,
    /// Rectangles that parts may not overlap.
    pub keepouts: Vec<Rect>,
}

/// Turn an offset `turns` quarter turns: each turn maps `(x, y)` to `(-y, x)`.
///
/// # Rules
/// - `turns` is taken modulo 4.
///
/// # Do not
/// - Use trigonometry here; quarter turns are exact swaps and sign flips.
pub fn turn_offset(dx: f64, dy: f64, turns: u8) -> (f64, f64) {
    match turns % 4 {
        0 => (dx, dy),
        1 => (-dy, dx),
        2 => (-dx, -dy),
        _ => (dy, -dx),
    }
}

impl Cell {
    /// The half width and half height of the box at the current turn.
    ///
    /// # Rules
    /// - An odd turn count swaps the two numbers.
    pub fn half(&self) -> (f64, f64) {
        if self.turns % 2 == 1 {
            (self.half_y, self.half_x)
        } else {
            (self.half_x, self.half_y)
        }
    }
}

impl Model {
    /// The world position of a pin, turned with its cell.
    ///
    /// # Rules
    /// - Returns the origin when the cell index is out of range.
    pub fn pin_world(&self, pin: &Pin) -> (f64, f64) {
        match self.cells.get(pin.cell) {
            Some(c) => {
                let (ox, oy) = turn_offset(pin.dx, pin.dy, c.turns);
                (c.x + ox, c.y + oy)
            }
            None => (0.0, 0.0),
        }
    }

    /// Half perimeter wirelength of one net, in millimetres.
    ///
    /// # Rules
    /// - Width plus height of the box around the pins; 0 for fewer than two.
    /// - Same number as `crate::wirelength::hpwl` gives for the same points.
    pub fn net_hpwl(&self, net: &NetSpring) -> f64 {
        if net.pins.len() < 2 {
            return 0.0;
        }
        let (mut lx, mut ly) = (f64::INFINITY, f64::INFINITY);
        let (mut hx, mut hy) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for pin in &net.pins {
            let (x, y) = self.pin_world(pin);
            lx = lx.min(x);
            hx = hx.max(x);
            ly = ly.min(y);
            hy = hy.max(y);
        }
        (hx - lx) + (hy - ly)
    }

    /// Total half perimeter wirelength of every net, unweighted.
    ///
    /// # Rules
    /// - This is the number reports show; net weights are never applied.
    pub fn hpwl_of(&self) -> f64 {
        self.nets.iter().map(|n| self.net_hpwl(n)).sum()
    }
}

/// True when two boxes overlap by more than `eps` on both axes.
///
/// # Rules
/// - Boxes are given as centre and half size. `clearance` is added to the
///   overlap, so two boxes closer than `clearance` count as overlapping.
/// - Boxes that only touch do not overlap (`clearance` 0).
///
/// # Do not
/// - Compare with `==`; the eps is what keeps rounding dust from counting.
pub fn boxes_overlap(
    a: (f64, f64, f64, f64),
    b: (f64, f64, f64, f64),
    clearance: f64,
    eps: f64,
) -> bool {
    let ox = a.2 + b.2 + clearance - (a.0 - b.0).abs();
    let oy = a.3 + b.3 + clearance - (a.1 - b.1).abs();
    ox > eps && oy > eps
}

/// Build the model for the parts in `scope` (slice 1121-a).
///
/// Why: every step needs flat data, and this is the only place that reads the
/// project. A part is fixed when `Scope::may_move` is false or when its box
/// comes within `edge_anchor_mm` of the board edge. A net takes the power
/// weight when a pin on it is power_in or power_out, or its name is a rail name
/// (GND, VCC, VDD, VSS, VBUS, a name starting with +, or digit V digit).
///
/// # Rules
/// - Cells sorted by (reference, footprint id); nets by net id.
/// - Half size is the larger distance from the part origin to any pad or drawn
///   point, per axis, from `Footprint::drawing_points` as `layout.rs` does.
/// - Nets with fewer than two cells are dropped; nets above `max_net_pins`
///   pins are kept in the model and skipped later by the solver.
/// - Refuse more than `max_cells` parts. Pure: the project is not changed.
///
/// # Dependencies
/// - crates/engine/src/scope.rs (`Scope::may_move`)
/// - crates/engine/src/layout.rs (`part_of` shows how the extent is read)
/// - crates/doc/src/lib.rs (`Footprint`, `Pad`, pin `electrical_type`)
///
/// # Do not
/// - Mutate the project or read the clock.
pub fn build_model(
    _project: &Project,
    _scope: &Scope,
    _settings: &PlacementSettings,
) -> Result<Model, String> {
    Err(not_built("slice-1121-a"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_net_model(turns: u8) -> Model {
        let cell = |x: f64, fixed: bool, turns: u8| Cell {
            id: 1,
            x,
            y: 0.0,
            half_x: 1.0,
            half_y: 0.5,
            turns,
            fixed,
        };
        Model {
            cells: vec![cell(0.0, true, 0), cell(10.0, false, turns)],
            nets: vec![NetSpring {
                net: 1,
                weight: 1.0,
                pins: vec![
                    Pin {
                        cell: 0,
                        dx: 0.0,
                        dy: 0.0,
                    },
                    Pin {
                        cell: 1,
                        dx: 2.0,
                        dy: 0.0,
                    },
                ],
            }],
            board: Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 40.0,
                y1: 40.0,
            },
            keepouts: vec![],
        }
    }

    #[test]
    fn a_quarter_turn_moves_the_pin_and_swaps_the_box() {
        let m = one_net_model(0);
        assert!((m.hpwl_of() - 12.0).abs() < 1e-12);
        let t = one_net_model(1);
        assert!((t.hpwl_of() - 12.0).abs() < 1e-12);
        assert_eq!(t.pin_world(&t.nets[0].pins[1]), (10.0, 2.0));
        assert_eq!(t.cells[1].half(), (0.5, 1.0));
        assert_eq!(turn_offset(2.0, 0.0, 2), (-2.0, -0.0));
        assert_eq!(turn_offset(2.0, 0.0, 3), (0.0, -2.0));
    }

    #[test]
    fn touching_boxes_do_not_overlap_and_a_clearance_makes_them() {
        let a = (0.0, 0.0, 1.0, 0.5);
        let b = (2.0, 0.0, 1.0, 0.5);
        assert!(!boxes_overlap(a, b, 0.0, 1e-9));
        assert!(boxes_overlap(a, b, 0.1, 1e-9));
        assert!(boxes_overlap(a, (1.5, 0.0, 1.0, 0.5), 0.0, 1e-9));
    }
}
