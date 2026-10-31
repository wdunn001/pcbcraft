//! Unix seconds for UI timestamps. The browser standard library has no clock.

/// Seconds since the Unix epoch in UTC, or zero when the clock is unavailable.
pub(crate) fn unix_secs_now() -> u64 {
    pcbcraft_clock::unix_secs()
}
