use super::text::{escape, push_folded, truncate_bytes, truncate_chars, unescape};

#[test]
fn text_escapes_the_characters_rfc_5545_names() {
    assert_eq!(escape(r"a\b;c,d"), r"a\\b\;c\,d");
    assert_eq!(escape("uno\ndos"), r"uno\ndos");
    assert_eq!(escape("uno\r\ndos"), r"uno\ndos");
}

#[test]
fn escaped_text_reads_back_to_the_same_text() {
    let text = "Café ñandú 🎉, con; comas\\barra\nsegunda línea";
    assert_eq!(unescape(&escape(text)), text);
}

#[test]
fn unescape_takes_an_uppercase_n_and_keeps_other_escapes_as_their_character() {
    assert_eq!(unescape(r"a\Nb"), "a\nb");
    assert_eq!(unescape(r"x\yz"), "xyz");
}

#[test]
fn a_long_line_folds_at_75_octets_and_unfolds_to_itself() {
    let line = format!("SUMMARY:{}", "a".repeat(200));
    let mut out = String::new();
    push_folded(&mut out, &line);
    assert!(out.ends_with("\r\n"));
    let parts: Vec<&str> = out.trim_end_matches("\r\n").split("\r\n").collect();
    assert!(parts.len() > 1);
    for part in &parts {
        assert!(part.len() <= 75, "{part:?} is longer than 75 octets");
    }
    let unfolded: String = parts
        .iter()
        .enumerate()
        .map(|(index, part)| if index == 0 { *part } else { &part[1..] })
        .collect();
    assert_eq!(unfolded, line);
}

#[test]
fn folding_never_splits_a_utf8_character() {
    let line = format!("DESCRIPTION:{}", "🎉ñ".repeat(60));
    let mut out = String::new();
    push_folded(&mut out, &line);
    let parts: Vec<&str> = out.trim_end_matches("\r\n").split("\r\n").collect();
    for part in &parts {
        assert!(part.len() <= 75, "{part:?}");
    }
    let unfolded: String = parts
        .iter()
        .enumerate()
        .map(|(index, part)| if index == 0 { *part } else { &part[1..] })
        .collect();
    assert_eq!(unfolded, line);
}

#[test]
fn a_short_line_is_written_once_with_its_crlf() {
    let mut out = String::new();
    push_folded(&mut out, "UID:x@excubia");
    assert_eq!(out, "UID:x@excubia\r\n");
}

#[test]
fn truncation_counts_characters_or_bytes_and_stops_on_a_boundary() {
    assert_eq!(truncate_chars("ñandú", 3), "ñan");
    assert_eq!(truncate_chars("abc", 10), "abc");
    assert_eq!(truncate_bytes("ñññ", 3), "ñ");
    assert_eq!(truncate_bytes("abc", 10), "abc");
}
