//! Per-device rate limiting and the per-device stream cap.
//!
//! Signature verification already stops an unauthenticated caller; this is
//! about a legitimate but malfunctioning client — a phone retrying in a tight
//! loop would otherwise turn a bug into a self-inflicted denial of service.
//! Fixed-window counters, read and write priced separately, plus a cap on
//! concurrent event streams per device.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::time::unix_seconds;

pub const READ_PER_MINUTE: u32 = 120;
pub const WRITE_PER_MINUTE: u32 = 30;
pub const MAX_STREAMS_PER_DEVICE: usize = 3;
const WINDOW_SECONDS: f64 = 60.0;
const EVICT_AFTER_SECONDS: f64 = 120.0;
const MAX_TRACKED_KEYS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Read,
    Write,
}

struct Bucket {
    window_started: f64,
    used: u32,
}

pub struct RateLimiter {
    read_per_minute: u32,
    write_per_minute: u32,
    buckets: Mutex<HashMap<(String, Class), Bucket>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            read_per_minute: READ_PER_MINUTE,
            write_per_minute: WRITE_PER_MINUTE,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Consume one unit for `key` in `class`, or return the seconds to wait.
    /// The pairing route uses the same write class keyed by `pair:<ip>`.
    pub fn allow(&self, key: &str, class: Class) -> Result<(), u64> {
        let now = unix_seconds();
        let limit = match class {
            Class::Read => self.read_per_minute,
            Class::Write => self.write_per_minute,
        };
        let mut buckets = self
            .buckets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        buckets.retain(|_, bucket| now - bucket.window_started < EVICT_AFTER_SECONDS);
        while buckets.len() > MAX_TRACKED_KEYS {
            if let Some(oldest) = buckets
                .iter()
                .min_by(|a, b| a.1.window_started.total_cmp(&b.1.window_started))
                .map(|(key, _)| key.clone())
            {
                buckets.remove(&oldest);
            } else {
                break;
            }
        }
        let bucket = buckets
            .entry((key.to_string(), class))
            .or_insert_with(|| Bucket {
                window_started: now,
                used: 0,
            });
        if now - bucket.window_started >= WINDOW_SECONDS {
            bucket.window_started = now;
            bucket.used = 0;
        }
        if bucket.used >= limit {
            let retry_after = (WINDOW_SECONDS - (now - bucket.window_started)).max(0.0);
            return Err(retry_after as u64 + 1);
        }
        bucket.used += 1;
        Ok(())
    }
}

/// Counts open event streams per device. SSE holds a worker and a subscriber,
/// so an unbounded number is a resource-exhaustion lever.
#[derive(Default)]
pub struct StreamRegistry {
    open: Mutex<HashMap<String, usize>>,
}

impl StreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn acquire(&self, device_id: &str) -> bool {
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = open.get(device_id).copied().unwrap_or(0);
        if current >= MAX_STREAMS_PER_DEVICE {
            return false;
        }
        open.insert(device_id.to_string(), current + 1);
        true
    }

    pub fn release(&self, device_id: &str) {
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let remaining = open.get(device_id).copied().unwrap_or(1).saturating_sub(1);
        if remaining == 0 {
            open.remove(device_id);
        } else {
            open.insert(device_id.to_string(), remaining);
        }
    }

    pub fn total(&self) -> usize {
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_write_budget_runs_out_and_reports_a_retry() {
        let limiter = RateLimiter::new();
        for _ in 0..WRITE_PER_MINUTE {
            assert!(limiter.allow("dev-1", Class::Write).is_ok());
        }
        let retry = limiter.allow("dev-1", Class::Write).unwrap_err();
        assert!((1..=61).contains(&retry), "retry-after {retry}");
        // A different device has its own budget.
        assert!(limiter.allow("dev-2", Class::Write).is_ok());
        // Reads are priced separately.
        assert!(limiter.allow("dev-1", Class::Read).is_ok());
    }

    #[test]
    fn the_stream_cap_is_per_device() {
        let registry = StreamRegistry::new();
        for _ in 0..MAX_STREAMS_PER_DEVICE {
            assert!(registry.acquire("dev-1"));
        }
        assert!(!registry.acquire("dev-1"));
        assert!(registry.acquire("dev-2"));
        assert_eq!(registry.total(), MAX_STREAMS_PER_DEVICE + 1);
        registry.release("dev-1");
        assert!(registry.acquire("dev-1"));
    }
}
