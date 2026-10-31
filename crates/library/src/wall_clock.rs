//! Unix time for cache freshness. The browser standard library has no clock.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch in UTC, or zero when the clock is unavailable.
pub(crate) fn unix_secs_now() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Current time as `SystemTime`, for file metadata updates on every target.
pub(crate) fn system_time_now() -> SystemTime {
    #[cfg(target_arch = "wasm32")]
    {
        UNIX_EPOCH + std::time::Duration::from_secs(unix_secs_now())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
    }
}
