//! The clock of the approvals. The store asks it for every instant it writes
//! or compares, so the escalation can be driven with a [`ManualClock`] instead
//! of waiting for real seconds. The instants are the same text SQLite writes
//! (`%Y-%m-%dT%H:%M:%fZ`), so both kinds of row compare as strings.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch, UTC.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// The wall clock of the host.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as i64)
    }
}

/// A clock that moves only when a test tells it to.
#[derive(Debug)]
pub struct ManualClock {
    now: AtomicI64,
}

impl ManualClock {
    pub fn at(now_ms: i64) -> Self {
        Self {
            now: AtomicI64::new(now_ms),
        }
    }

    pub fn advance_secs(&self, seconds: i64) {
        self.now.fetch_add(seconds * 1000, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
}

/// The text of an instant, as SQLite's `strftime('%Y-%m-%dT%H:%M:%fZ')` writes it.
pub fn iso(now_ms: i64) -> String {
    let days = now_ms.div_euclid(86_400_000);
    let millis_of_day = now_ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let hours = millis_of_day / 3_600_000;
    let minutes = millis_of_day / 60_000 % 60;
    let seconds = millis_of_day / 1000 % 60;
    let millis = millis_of_day % 1000;
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}

/// The instant of a text written by [`iso`] or by SQLite. `None` when the
/// text is not in that shape.
pub fn parse_iso(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 24 || bytes[10] != b'T' || bytes[23] != b'Z' {
        return None;
    }
    let field = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let year = field(0..4)?;
    let month = field(5..7)?;
    let day = field(8..10)?;
    let hours = field(11..13)?;
    let minutes = field(14..16)?;
    let seconds = field(17..19)?;
    let millis = field(20..23)?;
    let days = days_from_civil(year, month, day);
    Some(days * 86_400_000 + hours * 3_600_000 + minutes * 60_000 + seconds * 1000 + millis)
}

/// Howard Hinnant's civil calendar: the date of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::{iso, parse_iso, Clock, ManualClock};

    #[test]
    fn the_epoch_and_a_leap_day_are_written_like_sqlite_writes_them() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_709_164_800_000), "2024-02-29T00:00:00.000Z");
        assert_eq!(iso(1_792_540_800_000 + 123), "2026-10-21T00:00:00.123Z");
    }

    #[test]
    fn an_instant_survives_the_round_trip_through_its_text() {
        for ms in [0, 1_700_000_000_123, 1_792_713_600_999] {
            assert_eq!(parse_iso(&iso(ms)), Some(ms));
        }
        assert_eq!(parse_iso("2026-10-22 10:00:00"), None);
    }

    #[test]
    fn a_manual_clock_moves_only_when_told() {
        let clock = ManualClock::at(1_000);
        assert_eq!(clock.now_ms(), 1_000);
        clock.advance_secs(40);
        assert_eq!(clock.now_ms(), 41_000);
    }
}
