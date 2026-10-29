//! End-to-end stories: each `a_newcomer_*` test drives the real window like a
//! first-time user. One file per area; helpers live in `support`.

pub(crate) mod support;

mod artifacts;
mod board;
mod board3d;
mod builds;
mod checks;
mod editing;
mod layers;
mod layout;
mod library;
mod nets;
mod rules;
mod settings;
mod shell;
mod via_covering;
mod vias;
mod workbench;
