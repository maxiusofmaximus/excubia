//! The calendar's `.ics` exchange (CALENDARIO-F4c): the RFC 5545 subset the calendar keeps,
//! read and written with no crate of its own. A file is text from outside, so the reader
//! bounds it (line length, nesting, events) and nothing in it is run or fetched.
//!
//! `reader` turns the bytes into components, `import` turns each event into a draft or a
//! reason to skip it, `export` writes the company's events. Times live in `time`, text in `text`.

mod export;
mod import;
mod reader;
mod text;
mod time;

#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod reader_tests;
#[cfg(test)]
pub(crate) mod test_files;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod time_tests;

use serde::Serialize;
use thiserror::Error;

pub use export::export_calendar;
pub use import::{read_calendar, Draft, ImportReport, Parsed};

/// The most `VEVENT`s one file may hold.
pub const MAX_VEVENTS: usize = 500;
/// The most events one export holds; a larger one asks the caller to narrow its window.
pub const MAX_EXPORT_EVENTS: usize = 2_000;
/// The longest physical line of a file, in bytes.
const MAX_LINE_BYTES: usize = 8 * 1024;
/// The deepest nesting of components a file may have, the calendar itself included.
const MAX_DEPTH: usize = 64;
/// The longest UID a file may give an event, in bytes.
const UID_MAX_BYTES: usize = 255;
/// The UID an event exported from the host carries: its id, then this suffix.
pub(crate) const HOST_UID_SUFFIX: &str = "@excubia";
/// The most skipped events an import report names; the rest are only counted.
const DETAILS_MAX: usize = 50;

/// Why an event of a file is not kept. The code is what the API answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// The UID is one the company keeps already, so the event is not written again.
    Exists,
    /// A rule or a list of dates outside what the calendar keeps (`EXDATE`, `RDATE`, `BYMONTHDAY`).
    UnsupportedRecurrence,
    /// A `RECURRENCE-ID`: one changed occurrence of a series the file holds elsewhere.
    ExceptionInstance,
    /// No `DTSTART`, so there is no time to keep.
    NoStart,
    /// A time that does not parse, names a zone the calendar does not know, or a clock the zone skips.
    InvalidTime,
    /// A summary that is empty once read, or a UID longer than the calendar keeps.
    InvalidText,
    /// A rule that does not parse at all.
    InvalidRecurrence,
    /// The company already keeps the most events it can.
    CalendarLimit,
}

/// A file the reader refuses as a whole.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum IcsError {
    #[error("the file is not an iCalendar file")]
    Invalid,
    #[error("the file holds more events than one import takes")]
    TooManyEvents,
}
