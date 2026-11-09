//! Rate-limited upload counters keyed by publisher id.
//!
//! # Rules
//! - Window size must be 1..=2,592,000 seconds (30 days).
//! - Max uploads must be 1..=100,000.
//! - Keys must be 1..=64 bytes.
//! - The map is capped at 10,000 keys; empty queues are dropped first.
//!
//! # Dependencies
//! - crates/portal/src/error.rs (PortalError::RateLimited)
//!
//! # Do not
//! - Read the clock; callers supply timestamps in seconds.

use std::collections::{BTreeMap, VecDeque};

use crate::PortalError;

/// Tracks upload counts per publisher within a sliding time window.
/// Used to enforce rate limits on portal uploads.
///
/// # Rules
/// - window_secs must be 1..=2,592,000.
/// - max must be 1..=100,000.
///
/// # Dependencies
/// - crates/portal/src/error.rs
///
/// # Do not
/// - Read the clock; callers supply now.
pub struct UploadRateLimiter {
    window_secs: u64,
    max: u32,
    seen: BTreeMap<String, VecDeque<u64>>,
}

impl UploadRateLimiter {
    /// Create a new rate limiter.
    /// Validates window and max ranges.
    ///
    /// # Rules
    /// - window_secs must be 1..=2,592,000.
    /// - max must be 1..=100,000.
    ///
    /// # Dependencies
    /// - crates/portal/src/error.rs
    ///
    /// # Do not
    /// - Panic or unwrap.
    pub fn new(window_secs: u64, max: u32) -> Result<Self, PortalError> {
        if !(1..=2_592_000).contains(&window_secs) {
            return Err(PortalError::Invalid(
                "window must be 1..=2592000".to_string(),
            ));
        }
        if !(1..=100_000).contains(&max) {
            return Err(PortalError::Invalid("max must be 1..=100000".to_string()));
        }
        Ok(Self {
            window_secs,
            max,
            seen: BTreeMap::new(),
        })
    }

    /// Record an upload attempt or fail if the limit is reached.
    /// Drops timestamps older than the window.
    ///
    /// # Rules
    /// - key must be 1..=64 bytes.
    /// - now must be in seconds since epoch.
    ///
    /// # Dependencies
    /// - crates/portal/src/error.rs
    ///
    /// # Do not
    /// - Read the clock.
    pub fn check(&mut self, key: &str, now: u64) -> Result<u32, PortalError> {
        if key.len() > 64 {
            return Err(PortalError::Invalid("key too long".to_string()));
        }
        if self.seen.len() >= 10_000 {
            self.seen.retain(|_, v| !v.is_empty());
            if self.seen.len() >= 10_000 {
                return Err(PortalError::Storage("rate limit map full".to_string()));
            }
        }
        let entry = self.seen.entry(key.to_string()).or_default();
        entry.retain(|&t| t + self.window_secs > now);
        if entry.len() >= self.max as usize {
            let oldest = entry.front().unwrap_or(&now);
            return Err(PortalError::RateLimited {
                retry_after_secs: oldest + self.window_secs - now,
            });
        }
        entry.push_back(now);
        Ok(entry.len() as u32)
    }

    /// Count uploads in the window without recording a new one.
    /// For UploadContext.uploads_today.
    ///
    /// # Rules
    /// - now must be in seconds since epoch.
    ///
    /// # Dependencies
    /// - crates/portal/src/error.rs
    ///
    /// # Do not
    /// - Record a new upload.
    pub fn count(&self, key: &str, now: u64) -> u32 {
        if let Some(entry) = self.seen.get(key) {
            entry
                .iter()
                .filter(|&&t| t + self.window_secs > now)
                .count() as u32
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eleventh_upload_in_a_day_waits_until_the_window_moves() -> Result<(), PortalError> {
        let mut limiter = UploadRateLimiter::new(86400, 10)?;
        for i in 0..10 {
            limiter.check("pub-1", i)?;
        }
        if let Err(PortalError::RateLimited { retry_after_secs }) = limiter.check("pub-1", 10) {
            assert_eq!(retry_after_secs, 86390);
        } else {
            return Err(PortalError::Invalid("expected rate limited".to_string()));
        }
        Ok(())
    }

    #[test]
    fn after_the_window_moves_uploads_are_allowed_again() -> Result<(), PortalError> {
        let mut limiter = UploadRateLimiter::new(86400, 1)?;
        limiter.check("pub-1", 0)?;
        if limiter.check("pub-1", 1).is_ok() {
            return Err(PortalError::Invalid("expected rate limited".to_string()));
        }
        limiter.check("pub-1", 86401)?;
        Ok(())
    }

    #[test]
    fn two_publishers_have_separate_counts() -> Result<(), PortalError> {
        let mut limiter = UploadRateLimiter::new(86400, 2)?;
        limiter.check("pub-1", 0)?;
        limiter.check("pub-2", 0)?;
        limiter.check("pub-1", 0)?;
        if limiter.check("pub-1", 0).is_ok() {
            return Err(PortalError::Invalid(
                "expected rate limited for pub-1".to_string(),
            ));
        }
        limiter.check("pub-2", 0)?;
        Ok(())
    }

    #[test]
    fn a_key_over_64_bytes_is_refused() -> Result<(), PortalError> {
        let mut limiter = UploadRateLimiter::new(86400, 10)?;
        let long = "x".repeat(65);
        if limiter.check(&long, 0).is_ok() {
            return Err(PortalError::Invalid("expected invalid key".to_string()));
        }
        Ok(())
    }
}
