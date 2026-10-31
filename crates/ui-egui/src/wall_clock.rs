//! Unix seconds for UI timestamps. The browser standard library has no clock.

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
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}
