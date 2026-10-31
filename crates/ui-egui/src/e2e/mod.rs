//! End-to-end stories: each `a_newcomer_*` test drives the real window like a
//! first-time user. One file per area; helpers live in `support`.

pub(crate) mod support;

mod a11y_gate;
mod audit;

mod artifacts;
mod assistant;
mod board;
mod board3d;
mod board_side;
mod builds;
mod checks;
mod compare;
mod context_menus;
mod drill_table;
mod editing;
mod export;
mod i18n;
mod images;
mod keymap;
mod layers;
mod layout;
mod library;
mod library_browser;
mod library_dock;
mod minimap;
mod modifiers;
mod nets;
mod pairs;
mod pnp_machine;
mod pours;
mod route_drag;
mod rules;
mod schematic_pages;
mod settings;
mod sheet_rows;
mod shell;
mod showcase;
mod sourcing;
mod toolbar;
mod tree_rows;
mod via_covering;
mod vias;
mod view_side;
mod workbench;
