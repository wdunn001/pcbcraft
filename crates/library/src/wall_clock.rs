//! Unix time for cache freshness. The browser standard library has no clock.

use pcbcraft_clock::SystemTime;

/// Seconds since the Unix epoch in UTC, or zero when the clock is unavailable.
pub(crate) fn unix_secs_now() -> u64 {
    pcbcraft_clock::unix_secs()
}

/// Current time as `SystemTime`, for file metadata updates on every target.
///
/// Sub-second precision matters: files touched in the same second must still order.
pub(crate) fn system_time_now() -> SystemTime {
    pcbcraft_clock::system_now()
}
