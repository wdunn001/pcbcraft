//! Per-source request pacing and HTTP 429 handling.

use crate::sources::SourceError;
use std::sync::Mutex;

/// Seconds-based clock for tests.
pub trait Clock: Send + Sync {
    fn now_secs(&self) -> u64;
}

/// Wall clock in whole seconds since the Unix epoch.
#[derive(Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Token bucket with a fixed capacity and steady refill rate.
pub struct TokenBucket {
    capacity: u64,
    tokens: f64,
    refill_per_sec: f64,
    last: u64,
    clock: Box<dyn Clock>,
}

impl TokenBucket {
    /// `capacity` tokens at most; `refill_per_minute` tokens added each minute when below capacity.
    pub fn new(capacity: u64, refill_per_minute: u64, clock: Box<dyn Clock>) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            tokens: cap as f64,
            refill_per_sec: refill_per_minute as f64 / 60.0,
            last: clock.now_secs(),
            clock,
        }
    }

    fn sync(&mut self) {
        let now = self.clock.now_secs();
        let elapsed = now.saturating_sub(self.last);
        if elapsed > 0 {
            self.tokens = (self.tokens + elapsed as f64 * self.refill_per_sec)
                .min(self.capacity as f64);
            self.last = now;
        }
    }

    /// `Ok(())` when a token was taken; otherwise seconds until one is free.
    pub fn try_take(&mut self) -> Result<(), u64> {
        self.sync();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Ok(());
        }
        let need = 1.0 - self.tokens;
        let wait = if self.refill_per_sec > 0.0 {
            (need / self.refill_per_sec).ceil() as u64
        } else {
            60
        };
        Err(wait.max(1))
    }

    /// Force the bucket empty for `secs` (for example after a 429 with Retry-After).
    pub fn pause_for(&mut self, secs: u64) {
        self.sync();
        self.tokens = 0.0;
        self.last = self.clock.now_secs().saturating_add(secs);
    }
}

/// Map a rate-limit wait into [`SourceError::RateLimited`].
pub fn rate_limited(wait_secs: u64) -> SourceError {
    SourceError::RateLimited {
        retry_after_secs: wait_secs.max(1),
    }
}

/// Parse a `Retry-After` header value into whole seconds from `now`.
pub fn retry_after_secs(header: &str, now: u64) -> Option<u64> {
    let h = header.trim();
    if h.is_empty() {
        return None;
    }
    if let Ok(secs) = h.parse::<u64>() {
        return Some(secs.max(1));
    }
    parse_http_date_wait(h, now)
}

fn parse_http_date_wait(text: &str, now: u64) -> Option<u64> {
    // Minimal subset: only delay-in-seconds form is required for distributor APIs.
    let _ = (text, now);
    None
}

/// Injectable clock for unit tests.
pub struct FixedClock(Mutex<u64>);

impl FixedClock {
    pub fn new(start: u64) -> Self {
        Self(Mutex::new(start))
    }

    pub fn advance(&self, secs: u64) {
        if let Ok(mut t) = self.0.lock() {
            *t = t.saturating_add(secs);
        }
    }
}

impl Clock for FixedClock {
    fn now_secs(&self) -> u64 {
        self.0.lock().map(|t| *t).unwrap_or(0)
    }
}
