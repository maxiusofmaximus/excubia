//! Calendar files for the tests of this module and of the store's import.

/// One `VEVENT` block, one property per line.
pub(crate) fn vevent(props: &[&str]) -> String {
    let mut text = String::from("BEGIN:VEVENT\r\n");
    for prop in props {
        text.push_str(prop);
        text.push_str("\r\n");
    }
    text.push_str("END:VEVENT\r\n");
    text
}

/// A calendar holding the given `VEVENT` blocks, as bytes the way a request body arrives.
pub(crate) fn calendar_of(events: &[String]) -> Vec<u8> {
    let mut text = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for event in events {
        text.push_str(event);
    }
    text.push_str("END:VCALENDAR\r\n");
    text.into_bytes()
}
