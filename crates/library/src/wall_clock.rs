//! Unix time for cache freshness. The browser standard library has no clock.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch in UTC, or zero when the clock is unavailable.
pub(crate) fn unix_secs_now() -> u64 {
    pcbcraft_clock::unix_secs()
}

/// Current time as `SystemTime`, for file metadata updates on every target.
pub(crate) fn system_time_now() -> SystemTime {
    UNIX_EPOCH + std::time::Duration::from_secs(unix_secs_now())
}
