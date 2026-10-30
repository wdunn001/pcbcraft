//! Pure calculator engine for the Help reference (#394) and the user
//! calculators (#350). One function per calculator, no UI, no design
//! state: the command layer, the MCP server and the Help panel all read
//! formulas, ranges and labels from this crate instead of holding copies.
//!
//! Layer 0, beside geom: depends on no other pcbcraft crate and on no
//! external crates. The `toml` crate is a dev dependency only, used by
//! `tests/golden.rs` to replay the vectors.
//!
//! Local rules:
//! - `data/calculators.toml` is the single source of truth for inputs,
//!   ranges, formulas, sources, accuracy labels and golden vectors.
//!   `meta::CALCULATORS` mirrors it and the golden test checks both.
//! - Every calculator takes one input struct and returns
//!   `Result<Output, Error>`. Out-of-range inputs come back as
//!   `Error::OutOfRange` naming the closed interval; nothing panics.
//! - Result labels (exact, model or estimate) live in the metadata
//!   `accuracy` field; callers must show them next to results.
pub mod basic;
pub mod components;
pub mod consts;
pub mod copper;
pub mod glossary;
pub mod impedance;
pub mod meta;
pub mod spacing;
pub mod utilities;

pub use glossary::{search_glossary, term_by_id, GlossaryHit, Term, TERMS};

/// Why a calculator refused its inputs.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// A numeric input fell outside the closed interval the calculator
    /// accepts. The message states the interval, as the ticket requires.
    OutOfRange {
        /// Name of the offending input, as the metadata lists it.
        input: String,
        /// Lowest accepted value.
        min: f64,
        /// Highest accepted value.
        max: f64,
        /// Value that was rejected.
        given: f64,
    },
    /// Inputs were individually in range but do not form a case the
    /// calculator can solve (missing pair, contradictory pair, zero
    /// denominator, or a result that overflowed).
    Invalid(String),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::OutOfRange {
                input,
                min,
                max,
                given,
            } => write!(f, "{input} must be between {min} and {max}, got {given}"),
            Error::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Check a required numeric input against its closed interval.
pub(crate) fn num(name: &str, value: f64, min: f64, max: f64) -> Result<f64, Error> {
    if !value.is_finite() {
        return Err(Error::Invalid(format!("{name} must be a finite number")));
    }
    if value < min || value > max {
        return Err(Error::OutOfRange {
            input: name.to_string(),
            min,
            max,
            given: value,
        });
    }
    Ok(value)
}

/// Check an optional numeric input; absent passes through.
pub(crate) fn opt_num(
    name: &str,
    value: Option<f64>,
    min: f64,
    max: f64,
) -> Result<Option<f64>, Error> {
    match value {
        None => Ok(None),
        Some(v) => num(name, v, min, max).map(Some),
    }
}

/// Accept an integer input only inside the given closed interval.
pub(crate) fn int_num(name: &str, value: i64, min: i64, max: i64) -> Result<i64, Error> {
    if value < min || value > max {
        return Err(Error::OutOfRange {
            input: name.to_string(),
            min: min as f64,
            max: max as f64,
            given: value as f64,
        });
    }
    Ok(value)
}

/// Guard a computed value: a non-finite result means the inputs, though
/// individually in range, produced an overflow or a zero denominator.
pub(crate) fn finite(name: &str, value: f64) -> Result<f64, Error> {
    if !value.is_finite() {
        return Err(Error::Invalid(format!(
            "{name} is not finite for these inputs; check for a zero denominator"
        )));
    }
    Ok(value)
}
