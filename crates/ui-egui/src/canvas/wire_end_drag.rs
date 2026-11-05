//! Drag one end of a drawn wire onto a different pin to reconnect it.
//!
//! When the Select tool is used to drag a wire's end vertex and the drag is
//! released near a pin, the wire end is moved onto that pin and the wire joins
//! the pin's net. This calls the engine command `wire.reconnect_end`. If the
//! release is not near a pin, nothing happens and the wire returns to its
//! original position.
//!
//! # Rules
//! - Only the first or last vertex of a wire is draggable this way.
//! - The release must be within the pin pick radius.
//!
//! # Dependencies
//! - `crates/ui-egui/src/canvas/gestures.rs` calls it from press_end.
//! - `crates/ui-egui/src/canvas.rs` provides pick_pin.
//! - `crates/engine/src/wire_reconnect.rs` implements wire.reconnect_end.
//!
//! # Do not
//! - Call this on inner vertices; they keep today's MoveSegment behaviour.

use super::*;
use pcbcraft_doc::DrawnWire;

/// True when `index` is the first or last vertex of the wire.
pub(crate) fn is_end(wire: &DrawnWire, index: usize) -> bool {
    index == 0 || index == wire.points.len().saturating_sub(1)
}

impl App {
    /// Finish dragging a wire end: if near a pin, reconnect the wire to it.
    ///
    /// # Rules
    /// - Only called on the first or last vertex of a wire.
    /// - Does nothing if no pin is under the release point.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/canvas.rs` pick_pin for the release location.
    /// - `crates/engine/src/wire_reconnect.rs` for wire.reconnect_end.
    pub(crate) fn finish_wire_end_drag(&mut self, wire: Id, index: usize, at: Point) {
        let view = self.sheet_view();
        let wire_obj = match view.drawn_wires.iter().find(|w| w.id == wire) {
            Some(w) => w,
            None => return,
        };
        if !is_end(wire_obj, index) {
            return;
        }
        let end = if index == 0 { "start" } else { "end" };
        if let Some(pin) = self.pick_pin(at, self.schematic.zoom) {
            self.run(
                "wire.reconnect_end",
                json!({"id": wire, "end": end, "pin": pin}),
            );
        }
    }
}
