use super::reader::calendar;
use crate::ics::IcsError;

#[test]
fn a_calendar_reads_its_components_and_properties() {
    let root = calendar(
        b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:a@x\r\nSUMMARY:Hola\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    )
    .expect("a calendar");
    assert_eq!(root.name, "VCALENDAR");
    assert_eq!(root.prop("VERSION").expect("version").value, "2.0");
    let event = root.children.first().expect("the event");
    assert_eq!(event.name, "VEVENT");
    assert_eq!(event.prop("UID").expect("uid").value, "a@x");
    assert_eq!(event.prop("SUMMARY").expect("summary").value, "Hola");
}

#[test]
fn lf_endings_and_a_byte_order_mark_read_like_crlf() {
    let root = calendar("\u{feff}BEGIN:VCALENDAR\nSUMMARY:x\nEND:VCALENDAR\n".as_bytes())
        .expect("a calendar");
    assert_eq!(root.prop("SUMMARY").expect("summary").value, "x");
}

#[test]
fn folded_lines_join_and_a_continuation_may_start_with_a_tab() {
    let root = calendar(
        b"BEGIN:VCALENDAR\r\nDESCRIPTION:uno dos\r\n tres\r\n\tcuatro\r\nEND:VCALENDAR\r\n",
    )
    .expect("a calendar");
    assert_eq!(
        root.prop("DESCRIPTION").expect("description").value,
        "uno dostrescuatro"
    );
}

#[test]
fn quoted_parameters_keep_colons_and_semicolons() {
    let root = calendar(
        b"BEGIN:VCALENDAR\r\nDTSTART;TZID=\"Europe/Madrid\";X-NOTE=\"a:b;c\":20261103T093000\r\nEND:VCALENDAR\r\n",
    )
    .expect("a calendar");
    let start = root.prop("DTSTART").expect("start");
    assert_eq!(start.param("TZID"), Some("Europe/Madrid"));
    assert_eq!(start.param("X-NOTE"), Some("a:b;c"));
    assert_eq!(start.value, "20261103T093000");
}

#[test]
fn a_value_keeps_the_colons_after_its_first() {
    let root = calendar(b"BEGIN:VCALENDAR\r\nURL:https://example.com/a:b\r\nEND:VCALENDAR\r\n")
        .expect("a calendar");
    assert_eq!(
        root.prop("URL").expect("url").value,
        "https://example.com/a:b"
    );
}

#[test]
fn names_are_not_case_sensitive() {
    let root = calendar(b"begin:vcalendar\r\nsummary:x\r\nend:vcalendar\r\n").expect("a calendar");
    assert_eq!(root.name, "VCALENDAR");
    assert_eq!(root.prop("SUMMARY").expect("summary").value, "x");
}

/// `depth` open components, the calendar among them, each closed in turn.
fn nested(depth: usize) -> Vec<u8> {
    let names: Vec<String> = (0..depth)
        .map(|index| match index {
            0 => "VCALENDAR".to_string(),
            _ => format!("X-LEVEL{index}"),
        })
        .collect();
    let mut text = String::new();
    for name in &names {
        text.push_str(&format!("BEGIN:{name}\r\n"));
    }
    for name in names.iter().rev() {
        text.push_str(&format!("END:{name}\r\n"));
    }
    text.into_bytes()
}

#[test]
fn sixty_four_levels_are_read_and_one_more_is_refused() {
    assert!(calendar(&nested(64)).is_ok());
    assert_eq!(calendar(&nested(65)), Err(IcsError::Invalid));
}

#[test]
fn a_file_without_a_calendar_is_refused() {
    assert_eq!(calendar(b""), Err(IcsError::Invalid));
    assert_eq!(calendar(b"SUMMARY:x\r\n"), Err(IcsError::Invalid));
    assert_eq!(
        calendar(b"BEGIN:VEVENT\r\nEND:VEVENT\r\n"),
        Err(IcsError::Invalid)
    );
}

#[test]
fn a_component_left_open_or_closed_out_of_order_is_refused() {
    assert_eq!(calendar(b"BEGIN:VCALENDAR\r\n"), Err(IcsError::Invalid));
    assert_eq!(
        calendar(b"BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nEND:VCALENDAR\r\n"),
        Err(IcsError::Invalid)
    );
}

#[test]
fn a_physical_line_over_8_kib_is_refused_and_one_at_the_limit_is_read() {
    let at_limit = format!("SUMMARY:{}", "A".repeat(8192 - "SUMMARY:".len()));
    let file = format!("BEGIN:VCALENDAR\r\n{at_limit}\r\nEND:VCALENDAR\r\n");
    assert!(calendar(file.as_bytes()).is_ok());
    let over = format!("SUMMARY:{}", "A".repeat(8193 - "SUMMARY:".len()));
    let file = format!("BEGIN:VCALENDAR\r\n{over}\r\nEND:VCALENDAR\r\n");
    assert_eq!(calendar(file.as_bytes()), Err(IcsError::Invalid));
}

#[test]
fn bytes_that_are_not_utf8_are_refused() {
    assert_eq!(
        calendar(b"BEGIN:VCALENDAR\r\nSUMMARY:\xff\r\nEND:VCALENDAR\r\n"),
        Err(IcsError::Invalid)
    );
}

#[test]
fn a_property_outside_any_component_is_refused() {
    assert_eq!(
        calendar(b"UID:x\r\nBEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"),
        Err(IcsError::Invalid)
    );
}

#[test]
fn a_line_without_a_colon_or_with_a_bad_name_is_refused() {
    assert_eq!(
        calendar(b"BEGIN:VCALENDAR\r\nSUMMARY\r\nEND:VCALENDAR\r\n"),
        Err(IcsError::Invalid)
    );
    assert_eq!(
        calendar(b"BEGIN:VCALENDAR\r\nBAD NAME:1\r\nEND:VCALENDAR\r\n"),
        Err(IcsError::Invalid)
    );
}

#[test]
fn text_after_the_calendar_closes_is_refused() {
    assert_eq!(
        calendar(b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\nBEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"),
        Err(IcsError::Invalid)
    );
}
