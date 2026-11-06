//! Moving a power or ground port with the wires that touch it.
//!
//! A power port is a `Label` of kind `power`. It names a net and sits at a
//! point. Drawn wires reach it by ending on that point, so a port that moves
//! alone leaves its wire behind. This one rule moves the port and the wire
//! ends that touch it together.
//!
//! # Rules
//! - Only a power port moves this way; a junction or a plain label does not.
//! - A wire of the port's net (or a wire with no net) that ends on the port
//!   gets its end moved to the new point. A straight segment stays straight:
//!   a bend vertex keeps it square instead of leaving a slanted line.
//! - A wire that would shrink to one vertex is removed.
//! - A junction on the port point refuses the move.
//! - Dropping the port on a pin or wire of another net is refused, so two
//!   nets never join by accident. A pin or wire with no net there joins the
//!   port's net.
//!
//! # Dependencies
//! - crates/engine/src/power_port.rs (the typed command)
//! - crates/ui-egui/src/canvas/gestures.rs (drag preview)
//!
//! # Do not
//! - Move a port by editing `Label::position` alone.

use crate::{Id, PinRef, Project};
use pcbcraft_geom::Point;

/// Two points this close are the same spot, in millimetres.
const SAME_SPOT: f64 = 0.05;

fn same(a: Point, b: Point) -> bool {
    a.distance(b) < SAME_SPOT
}

impl Project {
    /// Move the power port `id` to `to` and carry the wire ends that touch it.
    ///
    /// # Rules
    /// - See the module rules. On an error the project is unchanged.
    /// - Returns the pins with no net that now sit under the port. The caller
    ///   joins them to the port net with its own pin assignment, because a pin
    ///   net must stay in step with its pad.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs (the page lookups keep the search on the port's
    ///   own page)
    ///
    /// # Do not
    /// - Call it for a port that is already at `to`.
    pub fn move_power_port(&mut self, id: Id, to: Point) -> Result<Vec<PinRef>, String> {
        if !to.valid() {
            return Err("port position is out of range".into());
        }
        let index = self
            .labels
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| "port not found".to_string())?;
        let port = self.labels[index].clone();
        if port.kind != "power" {
            return Err("only a power or ground port moves with its wires".into());
        }
        let from = port.position;
        if same(from, to) {
            return Err("the port is already there".into());
        }
        let page = self.schematic_label_page(&port);
        let net = port.net;
        if self.labels.iter().any(|l| {
            l.kind == "junction"
                && l.net == net
                && (same(l.position, from) || same(l.position, to))
                && self.schematic_label_page(l) == page
        }) {
            return Err("a junction sits on the port; move the junction first".into());
        }
        let wire_ids: Vec<Id> = self
            .drawn_wires
            .iter()
            .filter(|w| self.schematic_drawn_wire_page(w) == page || w.net.is_none())
            .map(|w| w.id)
            .collect();
        for w in self.drawn_wires.iter().filter(|w| wire_ids.contains(&w.id)) {
            let touches = w.points.iter().any(|q| same(*q, to))
                || w.points
                    .windows(2)
                    .any(|s| pcbcraft_geom::point_segment(to, s[0], s[1]) < SAME_SPOT);
            if touches && w.net.is_some_and(|n| n != net) {
                return Err("the port would join a wire of another net".into());
            }
        }
        let mut pins_there = vec![];
        for c in self.components.iter() {
            if c.sheet != page {
                continue;
            }
            for pin in c.pins.iter() {
                if c.pin_position(&pin.number).is_some_and(|q| same(q, to)) {
                    if pin.net.is_some_and(|n| n != net) {
                        return Err("the port would join a pin of another net".into());
                    }
                    if pin.net.is_none() {
                        pins_there.push(PinRef {
                            component: c.id,
                            pin: pin.number.clone(),
                            unit: None,
                        });
                    }
                }
            }
        }
        let mut doomed = vec![];
        let mut changed = vec![];
        for w in self.drawn_wires.iter().filter(|w| wire_ids.contains(&w.id)) {
            if w.net.is_some_and(|n| n != net) {
                continue;
            }
            let mut points = w.points.clone();
            let mut hit = false;
            for end_first in [true, false] {
                if points.len() < 2 {
                    break;
                }
                let (v, n) = if end_first {
                    (0, 1)
                } else {
                    (points.len() - 1, points.len() - 2)
                };
                if !same(points[v], from) {
                    continue;
                }
                hit = true;
                let old = points[v];
                let near = points[n];
                if same(near, to) {
                    points.remove(v);
                    continue;
                }
                let square = (old.x - near.x).abs() < 1e-9 || (old.y - near.y).abs() < 1e-9;
                let aligned = (to.x - near.x).abs() < 1e-9 || (to.y - near.y).abs() < 1e-9;
                let mut run = vec![];
                if square && !aligned {
                    let bend = if (old.y - near.y).abs() < 1e-9 {
                        Point::new(to.x, near.y)
                    } else {
                        Point::new(near.x, to.y)
                    };
                    run.push(bend);
                }
                run.push(to);
                if end_first {
                    points.remove(0);
                    for q in run {
                        points.insert(0, q);
                    }
                } else {
                    points.pop();
                    points.extend(run);
                }
            }
            if !hit {
                continue;
            }
            if points.len() < 2 {
                doomed.push(w.id);
            } else {
                changed.push((w.id, points));
            }
        }
        for id in &doomed {
            let gone = self.drawn_wires.iter().find(|w| w.id == *id);
            if gone.is_some_and(|w| {
                self.labels
                    .iter()
                    .any(|l| l.kind == "junction" && w.points.iter().any(|q| same(*q, l.position)))
            }) {
                return Err("the port wire holds a junction; move the port elsewhere".into());
            }
        }
        for (id, points) in changed {
            if let Some(w) = self.drawn_wires.iter_mut().find(|w| w.id == id) {
                w.points = points;
            }
        }
        self.drawn_wires.retain(|w| !doomed.contains(&w.id));
        for w in self.drawn_wires.iter_mut() {
            if w.net.is_none() && w.points.iter().any(|q| same(*q, to)) {
                w.net = Some(net);
            }
        }
        self.labels[index].position = to;
        Ok(pins_there)
    }
}
