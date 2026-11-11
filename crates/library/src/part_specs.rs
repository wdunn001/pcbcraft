//! Normalised part parameters with provenance, side by side diff tables,
//! the drop-in replacement check and dated prices.
//!
//! Sources use different names and units for the same parameter, so every
//! value is normalised onto a closed key list with fixed SI units and the
//! raw source text is kept beside it. Disagreement between sources is shown
//! with a confidence label instead of picking a winner.
//!
//! # Rules
//! - The parameter key list is closed: [`ParamKey`].
//! - Every [`SpecEntry`] carries [`Provenance`]: source, raw text and an
//!   observation day.
//! - [`Confidence`] is Confirmed, Agreed, Single or Conflict.
//! - Everything here is pure data: no clock calls and no network.
//! - Age is passed in as a plain whole-day count, see [`days_since`].
//!
//! # Dependencies
//! - `crates/library/src/lib.rs` (declares this module)
//! - `crates/library/src/part_sidecar.rs` (the file the sidecar stores these
//!   parameters in)
//!
//! # Do not
//! - Fetch from a source here; the source modules own that.
//! - Silence a conflict by choosing one side.
//! - Add a parameter key without fixing its unit.
