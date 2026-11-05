//! Bus entry tool: tap a single member net off a drawn bus with a short 45-degree stub.
//!
//! The tool finds the bus wire under the cursor, picks the next unused member index,
//! and runs `wire.bus_entry` to create the stub. The tool stays armed until Esc,
//! allowing multiple taps.
//!
//! # Dependencies
//! - crates/ui-egui/src/lib.rs#App
//! - crates/engine/src/bus_entry.rs#wire.bus_entry
//!
//! # Do not
//! - Do not create a new bus wire; only tap an existing one.
use super::*;

/// Distance threshold for finding the bus wire under the cursor (mm).
const BUS_TAP_DISTANCE: f64 = 0.5;

/// Distance threshold for finding an unconnected pin near the tap point (mm).
const PIN_TAP_DISTANCE: f64 = 12.0;

impl App {
    /// Place a bus entry stub at the given position.
    ///
    /// Finds the bus wire under the cursor, picks the next unused member index,
    /// finds an unconnected pin nearby, and runs `wire.bus_entry`.
    ///
    /// # Rules
    /// - The tap point must be on a bus wire.
    /// - The member index must be unused on that bus.
    /// - There must be an unconnected pin nearby.
    ///
    /// # Do not
    /// - Do not create a new bus wire.
    pub(crate) fn place_bus_entry(&mut self, pos: Point) {
        // Find the bus wire under the cursor.
        let bus_wire = self.project.drawn_wires.iter().find(|w| {
            let head = match w.points.first() {
                Some(h) => h,
                None => return false,
            };
            // Check if this wire has a bus label near its head.
            let has_bus_label = self
                .project
                .labels
                .iter()
                .any(|l| l.kind == "bus" && l.position.distance(*head) < 0.05);
            if !has_bus_label {
                return false;
            }
            let start = match w.points.first() {
                Some(s) => s,
                None => return false,
            };
            let end = match w.points.last() {
                Some(e) => e,
                None => return false,
            };
            pcbcraft_geom::point_segment(pos, *start, *end) < BUS_TAP_DISTANCE
        });

        let Some(bus_wire) = bus_wire else {
            self.message = t!("schematic.bus_entry.no_bus");
            return;
        };

        // Find the bus label to get the bus name and member count.
        let head = match bus_wire.points.first() {
            Some(h) => h,
            None => {
                self.message = t!("schematic.bus_entry.no_label");
                return;
            }
        };
        let bus_label = self
            .project
            .labels
            .iter()
            .find(|l| l.kind == "bus" && l.position.distance(*head) < 0.05);

        let Some(bus_label) = bus_label else {
            self.message = t!("schematic.bus_entry.no_label");
            return;
        };

        // Parse the bus name to get the stem and count.
        let parts: Vec<&str> = bus_label.text.split('*').collect();
        if parts.len() != 2 {
            self.message = t!("schematic.bus_entry.bad_name");
            return;
        }
        let stem = parts[0];
        let count: usize = match parts[1].parse() {
            Ok(n) if n > 0 => n,
            _ => {
                self.message = t!("schematic.bus_entry.bad_count");
                return;
            }
        };

        // Find the next unused member index.
        let mut member = self.bus_entry_member;
        for _ in 0..count {
            let member_name = format!("{}{}", stem, member);
            let used = self.project.drawn_wires.iter().any(|w| {
                if let Some(net_id) = w.net {
                    self.project.net_name(net_id) == member_name
                } else {
                    false
                }
            });
            if !used {
                break;
            }
            member = (member + 1) % count;
        }
        self.bus_entry_member = (member + 1) % count;

        // Find an unconnected pin near the tap point.
        let pin = self.project.components.iter().find_map(|comp| {
            comp.pins.iter().find_map(|pin| {
                if pin.net.is_some() {
                    return None;
                }
                let pin_pos = match comp.pin_position(&pin.number) {
                    Some(p) => p,
                    None => return None,
                };
                if pin_pos.distance(pos) < PIN_TAP_DISTANCE {
                    Some((comp.id, pin.number.clone()))
                } else {
                    None
                }
            })
        });

        let Some((comp_id, pin_num)) = pin else {
            self.message = t!("schematic.bus_entry.no_pin");
            return;
        };

        // Run the bus entry command.
        self.run(
            "wire.bus_entry",
            json!({
                "bus": bus_wire.id,
                "member": member,
                "at": pos,
                "pin": {"component": comp_id, "pin": pin_num}
            }),
        );
    }
}
