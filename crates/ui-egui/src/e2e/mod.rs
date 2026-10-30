//! End-to-end stories: each `a_newcomer_*` test drives the real window like a
//! first-time user. One file per area; helpers live in `support`.

pub(crate) mod support;

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
mod datasheet_web;
mod editing;
mod images;
mod keymap;
mod layers;
mod layout;
mod library;
mod library_browser;
mod library_dock;
mod modifiers;
mod nets;
mod pairs;
mod pours;
mod route_drag;
mod rules;
mod schematic_pages;
mod settings;
mod shell;
mod showcase;
mod toolbar;
mod tree_rows;
mod via_covering;
mod vias;
mod view_side;
mod workbench;
