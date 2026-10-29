//! Native circuit simulation: a component value parser and a direct-current solver for
//! resistors, voltage sources and current sources on a plain netlist.
//!
//! # Rules
//! - Pure Rust with no dependencies. Nothing here knows about the project, the engine or the UI.
//! - Every size is bounded (`MAX_NODES`, `MAX_ELEMENTS`); every loop is bounded; no recursion.
//! - A bad input or a singular circuit is a plain error sentence naming the element or node, never NaN.
//!
//! # Dependencies
//! - docs/adr-0001-simulation.md the decision record this crate is written against
//!
//! # Do not
//! - Do not import egui, the engine or the document crate; the layer check in xtask enforces it.
//! - Do not run an external program; netlist files are the hand-off (see the decision record).
pub mod check;
pub mod net;
pub mod solve;
pub mod value;

pub use check::check;
pub use net::{Element, MAX_ELEMENTS, MAX_NODES, Netlist};
pub use solve::{Solution, SourceCurrent, solve_dc};
pub use value::parse_value;
