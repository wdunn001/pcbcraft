//! Unix time for cache freshness.
//!
//! Use pcbcraft_clock for time.

#[cfg(target_arch = "wasm32")]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unix_secs_now() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        UNIX_EPOCH.elapsed().unwrap_or_default().as_secs()
    }
    #[cfg(target_arch = "wasm32")]
    {
        pcbcraft_clock::unix_secs()
    }
}

pub(crate) fn system_time_now() -> SystemTime {
    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
    }
    #[cfg(target_arch = "wasm32")]
    {
        UNIX_EPOCH + Duration::from_secs(pcbcraft_clock::unix_secs())
    }
}
