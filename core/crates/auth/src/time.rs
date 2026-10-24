//! Wall-clock seconds, the unit every timestamp in the contracts uses.

use std::time::{SystemTime, UNIX_EPOCH};

/// Unix seconds as a float (the trust store keeps fractional timestamps).
/// A clock before the epoch is reported as zero rather than panicking.
pub fn unix_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}
