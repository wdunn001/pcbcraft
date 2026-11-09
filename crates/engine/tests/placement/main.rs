//! Acceptance tests for automatic part placement (issues 1121 and 1122).
//!
//! Every test that a later slice must make pass is written here and marked
//! `#[ignore = "built in slice-<issue>-<letter>"]`; the slice implements the
//! function, removes the ignore and runs the test. Numbers come from the text
//! fixtures in `tests/fixtures/placement/`, each cross-checked by hand or by an
//! independent script as described in the pull request that added them.
//!
//! Run them with `cargo test -p pcbcraft-engine --test placement`.
mod fixture;
mod global;
mod polish;
