//! File operations behind a trait so the browser build can swap in a
//! virtual filesystem. On desktop this crate re-exports std::fs
//! a different implementation mounts the browser storage.
//! a different implementation mounts the browser storage.
//!
//! Local rules: every crate that needs filesystem access calls these
//! functions, never std::fs directly. The lint in clippy.toml bans
//! std::fs calls for the whole workspace; this crate is the only place
