//! The narrow SSE subset the gateway emits, fed one line at a time.
//!
//! Lines rather than byte chunks: the Dart side already reads a UTF-8 line
//! stream, and this keeps the framing rules in one testable place.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawFrame {
    pub event: String,
    pub data: String,
}

#[derive(Debug, Default)]
pub(crate) struct SseParser {
    data: String,
    has_data: bool,
    event: Option<String>,
}

impl SseParser {
    pub(crate) fn accept(&mut self, line: &str) -> Option<RawFrame> {
        let line = line.strip_suffix('\r').unwrap_or(line);

        if line.is_empty() {
            if !self.has_data {
                self.event = None;
                return None;
            }
            let frame = RawFrame {
                event: self.event.take().unwrap_or_else(|| "change".to_string()),
                data: std::mem::take(&mut self.data),
            };
            self.has_data = false;
            return Some(frame);
        }

        if line.starts_with(':') {
            return None;
        }

        let (field, raw_value) = line.split_once(':')?;
        let value = raw_value.strip_prefix(' ').unwrap_or(raw_value);
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => {
                if value.is_empty() {
                    return None;
                }
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(parser: &mut SseParser, lines: &[&str]) -> Vec<RawFrame> {
        lines
            .iter()
            .filter_map(|line| parser.accept(line))
            .collect()
    }

    #[test]
    fn a_simple_state_frame_parses() {
        let mut parser = SseParser::default();
        let frames = feed(
            &mut parser,
            &[
                "event: state",
                r#"data: {"event":"state","revision":7}"#,
                "",
            ],
        );
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "state");
        assert!(frames[0].data.contains("\"revision\":7"));
    }

    #[test]
    fn two_frames_in_one_read_are_two_frames() {
        let mut parser = SseParser::default();
        let frames = feed(
            &mut parser,
            &[
                "event: state",
                r#"data: {"revision":1}"#,
                "",
                "event: change",
                r#"data: {"revision":2}"#,
                "",
            ],
        );
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].data, r#"{"revision":2}"#);
    }

    #[test]
    fn keepalive_comments_do_not_emit_frames() {
        let mut parser = SseParser::default();
        assert!(feed(&mut parser, &[": keepalive", "", ": keepalive", ""]).is_empty());
    }

    #[test]
    fn a_comment_between_fields_does_not_break_the_frame() {
        let mut parser = SseParser::default();
        let frames = feed(
            &mut parser,
            &[
                "event: state",
                ": a comment mid-frame",
                r#"data: {"revision":3}"#,
                "",
            ],
        );
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, r#"{"revision":3}"#);
    }

    #[test]
    fn multi_line_data_is_rejoined_with_newlines() {
        let mut parser = SseParser::default();
        let frames = feed(
            &mut parser,
            &["event: state", "data: {\"a\":1,", "data: \"b\":2}", ""],
        );
        assert_eq!(frames[0].data, "{\"a\":1,\n\"b\":2}");
    }

    #[test]
    fn a_frame_without_an_event_defaults_to_change() {
        let mut parser = SseParser::default();
        let frames = feed(&mut parser, &[r#"data: {"revision":4}"#, ""]);
        assert_eq!(frames[0].event, "change");
    }

    #[test]
    fn an_empty_data_field_emits_nothing() {
        let mut parser = SseParser::default();
        assert!(feed(&mut parser, &["event: state", "data:", ""]).is_empty());
    }

    #[test]
    fn only_the_first_space_after_the_colon_is_stripped() {
        let mut parser = SseParser::default();
        let frames = feed(&mut parser, &["event: state", "data:  two spaces", ""]);
        assert_eq!(frames[0].data, " two spaces");
    }

    #[test]
    fn crlf_and_unknown_fields_are_tolerated() {
        let mut parser = SseParser::default();
        let frames = feed(
            &mut parser,
            &[
                "id: 42\r",
                "event: state\r",
                "retry: 3000\r",
                "data: {\"revision\":5}\r",
                "\r",
            ],
        );
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "{\"revision\":5}");
    }
}
