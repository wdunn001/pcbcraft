//! The one place the program reads the clock.
//!
//! The browser's standard library has no clock: `SystemTime::now` and
//! `Instant::now` panic there ("time not implemented on this platform"). This
//! crate wraps the `web-time` crate, which is the standard library on desktop
//! and the page's own clock in the browser, so every other crate calls these
//! functions and never the standard ones.
//!
//! Local rules: `clippy.toml` bans `std::time::SystemTime::now` and
//! `std::time::Instant::now` for the whole workspace; the calls below are the
//! only ones allowed, and only because on desktop `web-time` is the standard
//! library. Use [`Instant`] and [`SystemTime`] from here for field and
//! argument types so one value can be compared with another.
//!
//! # Dependencies
//! - `clippy.toml` (the ban)
//! - `xtask/src/main.rs` (layer 0, no workspace dependencies)
//!
//! # Do not
//! - Call `std::time::SystemTime::now` or `std::time::Instant::now` anywhere.
//! - Add a workspace dependency; this crate sits below every other.

#![allow(clippy::disallowed_methods)]

pub use web_time::{Instant, SystemTime};

/// The current monotonic time, safe in the browser.
///
/// # Rules
/// - Use for timeouts, frame timing and elapsed-time measures only.
///
/// # Do not
/// - Convert to a calendar date; use [`unix_secs`] for that.
pub fn instant_now() -> Instant {
    Instant::now()
}

/// The current wall-clock time, safe in the browser.
///
/// # Do not
/// - Call `SystemTime::now` directly; the lint rejects it.
pub fn system_now() -> SystemTime {
    SystemTime::now()
}

/// Whole seconds since 1970-01-01 UTC; 0 if the clock reads before 1970.
///
/// # Rules
/// - Never panics; a clock set before the epoch gives 0.
pub fn unix_secs() -> u64 {
    system_now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Nanoseconds since 1970-01-01 UTC; 0 if the clock reads before 1970.
pub fn unix_nanos() -> u128 {
    system_now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    /// The clock reads a date after this code was written and never goes backwards.
    #[test]
    fn the_clock_reads_a_current_time_and_advances() {
        assert!(super::unix_secs() > 1_780_000_000);
        let first = super::instant_now();
        let second = super::instant_now();
        assert!(second >= first);
    }
}
